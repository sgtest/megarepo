/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.cluster.routing.allocation.allocator;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateTaskExecutor;
import org.elasticsearch.cluster.ClusterStateTaskListener;
import org.elasticsearch.cluster.routing.RoutingNode;
import org.elasticsearch.cluster.routing.RoutingNodes;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.cluster.routing.allocation.RoutingAllocation;
import org.elasticsearch.cluster.routing.allocation.RoutingExplanations;
import org.elasticsearch.cluster.routing.allocation.ShardAllocationDecision;
import org.elasticsearch.cluster.routing.allocation.command.AllocationCommand;
import org.elasticsearch.cluster.routing.allocation.command.AllocationCommands;
import org.elasticsearch.cluster.routing.allocation.command.MoveAllocationCommand;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.cluster.service.MasterService;
import org.elasticsearch.cluster.service.MasterServiceTaskQueue;
import org.elasticsearch.common.Priority;
import org.elasticsearch.common.metrics.CounterMetric;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.util.concurrent.EsRejectedExecutionException;
import org.elasticsearch.common.util.set.Sets;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.threadpool.ThreadPool;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Objects;
import java.util.Set;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Consumer;

import static java.util.stream.Collectors.toSet;

/**
 * A {@link ShardsAllocator} which asynchronously refreshes the desired balance held by the {@link DesiredBalanceComputer} and then takes
 * steps towards the desired balance using the {@link DesiredBalanceReconciler}.
 */
public class DesiredBalanceShardsAllocator implements ShardsAllocator {

    private static final Logger logger = LogManager.getLogger(DesiredBalanceShardsAllocator.class);

    private final ShardsAllocator delegateAllocator;
    private final ThreadPool threadPool;
    private final DesiredBalanceReconcilerAction reconciler;
    private final DesiredBalanceComputer desiredBalanceComputer;
    private final ContinuousComputation<DesiredBalanceInput> desiredBalanceComputation;
    private final PendingListenersQueue queue;
    private final AtomicLong indexGenerator = new AtomicLong(-1);
    private final ConcurrentLinkedQueue<List<MoveAllocationCommand>> pendingDesiredBalanceMoves = new ConcurrentLinkedQueue<>();
    private final MasterServiceTaskQueue<ReconcileDesiredBalanceTask> masterServiceTaskQueue;
    private final NodeAllocationOrdering allocationOrdering = new NodeAllocationOrdering();
    private volatile DesiredBalance currentDesiredBalance = DesiredBalance.INITIAL;

    // stats
    protected final CounterMetric computationsSubmitted = new CounterMetric();
    protected final CounterMetric computationsExecuted = new CounterMetric();
    protected final CounterMetric computationsConverged = new CounterMetric();
    protected final CounterMetric cumulativeComputationTime = new CounterMetric();
    protected final CounterMetric cumulativeReconciliationTime = new CounterMetric();

    @FunctionalInterface
    public interface DesiredBalanceReconcilerAction {
        ClusterState apply(ClusterState clusterState, Consumer<RoutingAllocation> routingAllocationAction);
    }

    public DesiredBalanceShardsAllocator(
        ClusterSettings clusterSettings,
        ShardsAllocator delegateAllocator,
        ThreadPool threadPool,
        ClusterService clusterService,
        DesiredBalanceReconcilerAction reconciler
    ) {
        this(
            delegateAllocator,
            threadPool,
            clusterService,
            new DesiredBalanceComputer(clusterSettings, threadPool, delegateAllocator),
            reconciler
        );
    }

    public DesiredBalanceShardsAllocator(
        ShardsAllocator delegateAllocator,
        ThreadPool threadPool,
        ClusterService clusterService,
        DesiredBalanceComputer desiredBalanceComputer,
        DesiredBalanceReconcilerAction reconciler
    ) {
        this.delegateAllocator = delegateAllocator;
        this.threadPool = threadPool;
        this.reconciler = reconciler;
        this.desiredBalanceComputer = desiredBalanceComputer;
        this.desiredBalanceComputation = new ContinuousComputation<>(threadPool) {

            @Override
            protected void processInput(DesiredBalanceInput desiredBalanceInput) {

                long index = desiredBalanceInput.index();
                logger.debug("Starting desired balance computation for [{}]", index);

                recordTime(
                    cumulativeComputationTime,
                    () -> setCurrentDesiredBalance(
                        desiredBalanceComputer.compute(
                            currentDesiredBalance,
                            desiredBalanceInput,
                            pendingDesiredBalanceMoves,
                            this::isFresh
                        )
                    )
                );
                computationsExecuted.inc();
                if (isFresh(desiredBalanceInput)) {
                    logger.debug("Desired balance computation for [{}] is completed, scheduling reconciliation", index);
                    computationsConverged.inc();
                    submitReconcileTask(currentDesiredBalance);
                } else {
                    logger.debug("Desired balance computation for [{}] is discarded as newer one is submitted", index);
                }
            }

            @Override
            public String toString() {
                return "DesiredBalanceShardsAllocator#updateDesiredBalanceAndReroute";
            }
        };
        this.queue = new PendingListenersQueue(threadPool);
        this.masterServiceTaskQueue = clusterService.createTaskQueue(
            "reconcile-desired-balance",
            Priority.URGENT,
            new ReconcileDesiredBalanceExecutor()
        );
    }

    @Override
    public ShardAllocationDecision decideShardAllocation(ShardRouting shard, RoutingAllocation allocation) {
        return delegateAllocator.decideShardAllocation(shard, allocation);
    }

    @Override
    public void allocate(RoutingAllocation allocation) {
        throw new UnsupportedOperationException();
    }

    @Override
    public void allocate(RoutingAllocation allocation, ActionListener<Void> listener) {
        assert MasterService.assertMasterUpdateOrTestThread() : Thread.currentThread().getName();
        assert allocation.ignoreDisable() == false;

        computationsSubmitted.inc();

        var index = indexGenerator.incrementAndGet();
        logger.debug("Executing allocate for [{}]", index);
        queue.add(index, listener);
        desiredBalanceComputation.onNewInput(DesiredBalanceInput.create(index, allocation));

        // Starts reconciliation towards desired balance that might have not been updated with a recent calculation yet.
        // This is fine as balance should have incremental rather than radical changes.
        // This should speed up achieving the desired balance in cases current state is still different from it (due to THROTTLING).
        reconcile(currentDesiredBalance, allocation);
    }

    @Override
    public RoutingExplanations execute(RoutingAllocation allocation, AllocationCommands commands, boolean explain, boolean retryFailed) {
        var explanations = ShardsAllocator.super.execute(allocation, commands, explain, retryFailed);
        var moves = getMoveCommands(commands);
        if (moves.isEmpty() == false) {
            pendingDesiredBalanceMoves.add(moves);
        }
        return explanations;
    }

    private static List<MoveAllocationCommand> getMoveCommands(AllocationCommands commands) {
        var moves = new ArrayList<MoveAllocationCommand>();
        for (AllocationCommand command : commands.commands()) {
            if (command instanceof MoveAllocationCommand move) {
                moves.add(move);
            }
        }
        return moves;
    }

    private void setCurrentDesiredBalance(DesiredBalance newDesiredBalance) {
        if (logger.isTraceEnabled()) {
            var diff = DesiredBalance.hasChanges(currentDesiredBalance, newDesiredBalance)
                ? "Diff: " + diff(currentDesiredBalance, newDesiredBalance)
                : "No changes";
            logger.trace("Desired balance updated: {}. {}", newDesiredBalance, diff);
        } else {
            logger.debug("Desired balance updated for [{}]", newDesiredBalance.lastConvergedIndex());
        }
        currentDesiredBalance = newDesiredBalance;
    }

    protected void submitReconcileTask(DesiredBalance desiredBalance) {
        masterServiceTaskQueue.submitTask("reconcile-desired-balance", new ReconcileDesiredBalanceTask(desiredBalance), null);
    }

    protected void reconcile(DesiredBalance desiredBalance, RoutingAllocation allocation) {
        if (logger.isTraceEnabled()) {
            logger.trace("Reconciling desired balance: {}", desiredBalance);
        } else {
            logger.debug("Reconciling desired balance for [{}]", desiredBalance.lastConvergedIndex());
        }
        allocationOrdering.retainNodes(getNodeIds(allocation.routingNodes()));
        recordTime(cumulativeReconciliationTime, new DesiredBalanceReconciler(desiredBalance, allocation, allocationOrdering)::run);
        if (logger.isTraceEnabled()) {
            logger.trace("Reconciled desired balance: {}", desiredBalance);
        } else {
            logger.debug("Reconciled desired balance for [{}]", desiredBalance.lastConvergedIndex());
        }
    }

    public DesiredBalance getDesiredBalance() {
        return currentDesiredBalance;
    }

    public DesiredBalanceStats getStats() {
        return new DesiredBalanceStats(
            currentDesiredBalance.lastConvergedIndex(),
            desiredBalanceComputation.isActive(),
            computationsSubmitted.count(),
            computationsExecuted.count(),
            computationsConverged.count(),
            desiredBalanceComputer.iterations.sum(),
            cumulativeComputationTime.count(),
            cumulativeReconciliationTime.count()
        );
    }

    private void onNoLongerMaster() {
        if (indexGenerator.getAndSet(-1) != -1) {
            currentDesiredBalance = DesiredBalance.INITIAL;
            queue.completeAllAsNotMaster();
            pendingDesiredBalanceMoves.clear();
            allocationOrdering.clear();
        }
    }

    private final class ReconcileDesiredBalanceTask implements ClusterStateTaskListener {
        private final DesiredBalance desiredBalance;

        private ReconcileDesiredBalanceTask(DesiredBalance desiredBalance) {
            this.desiredBalance = desiredBalance;
        }

        @Override
        public void onFailure(Exception e) {
            assert MasterService.isPublishFailureException(e) : e;
            if (e.getCause() != null && e.getCause()instanceof EsRejectedExecutionException esRejectedExecutionException) {
                assert esRejectedExecutionException.isExecutorShutdown();
                // TODO now what? onNoLongerMaster() asserts it's on the master thread but we could be anywhere here
            } else {
                onNoLongerMaster();
            }
        }

        @Override
        public String toString() {
            return "ReconcileDesiredBalanceTask[lastConvergedIndex=" + desiredBalance.lastConvergedIndex() + "]";
        }
    }

    private final class ReconcileDesiredBalanceExecutor implements ClusterStateTaskExecutor<ReconcileDesiredBalanceTask> {

        @Override
        public ClusterState execute(BatchExecutionContext<ReconcileDesiredBalanceTask> batchExecutionContext) {
            var latest = findLatest(batchExecutionContext.taskContexts());
            var newState = applyBalance(batchExecutionContext, latest);
            discardSupersededTasks(batchExecutionContext.taskContexts(), latest);
            return newState;
        }

        private TaskContext<ReconcileDesiredBalanceTask> findLatest(List<? extends TaskContext<ReconcileDesiredBalanceTask>> taskContexts) {
            return taskContexts.stream().max(Comparator.comparing(context -> context.getTask().desiredBalance.lastConvergedIndex())).get();
        }

        private ClusterState applyBalance(
            BatchExecutionContext<ReconcileDesiredBalanceTask> batchExecutionContext,
            TaskContext<ReconcileDesiredBalanceTask> latest
        ) {
            try (var ignored = batchExecutionContext.dropHeadersContext()) {
                var newState = reconciler.apply(
                    batchExecutionContext.initialState(),
                    routingAllocation -> reconcile(latest.getTask().desiredBalance, routingAllocation)
                );
                latest.success(() -> queue.complete(latest.getTask().desiredBalance.lastConvergedIndex()));
                return newState;
            }
        }

        private void discardSupersededTasks(
            List<? extends TaskContext<ReconcileDesiredBalanceTask>> taskContexts,
            TaskContext<ReconcileDesiredBalanceTask> latest
        ) {
            for (TaskContext<ReconcileDesiredBalanceTask> taskContext : taskContexts) {
                if (taskContext != latest) {
                    taskContext.success(() -> {});
                }
            }
        }
    }

    private void recordTime(CounterMetric metric, Runnable action) {
        final long started = threadPool.relativeTimeInMillis();
        try {
            action.run();
        } finally {
            final long finished = threadPool.relativeTimeInMillis();
            metric.inc(finished - started);
        }
    }

    private static String diff(DesiredBalance old, DesiredBalance updated) {
        var intersection = Sets.intersection(old.assignments().keySet(), updated.assignments().keySet());
        var diff = Sets.difference(Sets.union(old.assignments().keySet(), updated.assignments().keySet()), intersection);

        var newLine = System.lineSeparator();
        var builder = new StringBuilder();
        for (ShardId shardId : intersection) {
            var oldAssignment = old.getAssignment(shardId);
            var updatedAssignment = updated.getAssignment(shardId);
            if (Objects.equals(oldAssignment, updatedAssignment) == false) {
                builder.append(newLine).append(shardId).append(": ").append(oldAssignment).append(" -> ").append(updatedAssignment);
            }
        }
        for (ShardId shardId : diff) {
            var oldAssignment = old.getAssignment(shardId);
            var updatedAssignment = updated.getAssignment(shardId);
            builder.append(newLine).append(shardId).append(": ").append(oldAssignment).append(" -> ").append(updatedAssignment);
        }
        return builder.append(newLine).toString();
    }

    private static Set<String> getNodeIds(RoutingNodes nodes) {
        return nodes.stream().map(RoutingNode::nodeId).collect(toSet());
    }
}
