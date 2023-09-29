/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.compute.operator;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.SubscribableListener;
import org.elasticsearch.common.util.concurrent.AbstractRunnable;
import org.elasticsearch.compute.Describable;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.core.Releasable;
import org.elasticsearch.core.Releasables;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.tasks.TaskCancelledException;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.concurrent.Executor;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.Supplier;
import java.util.stream.Collectors;

/**
 * A driver operates single-threadedly on a simple chain of {@link Operator}s, passing
 * {@link Page}s from one operator to the next. It also controls the lifecycle of the
 * operators.
 * The operator chain typically starts with a source operator (i.e. an operator that purely produces pages)
 * and ends with a sink operator (i.e. an operator that purely consumes pages).
 *
 * More details on how this integrates with other components can be found in the package documentation of
 * {@link org.elasticsearch.compute}
 */

public class Driver implements Releasable, Describable {
    public static final TimeValue DEFAULT_TIME_BEFORE_YIELDING = TimeValue.timeValueMinutes(5);
    public static final int DEFAULT_MAX_ITERATIONS = 10_000;
    /**
     * Minimum time between updating status.
     */
    public static final TimeValue DEFAULT_STATUS_INTERVAL = TimeValue.timeValueSeconds(1);

    private final String sessionId;
    private final DriverContext driverContext;
    private final Supplier<String> description;
    private final List<Operator> activeOperators;
    private final Releasable releasable;
    private final long statusNanos;

    private final AtomicReference<String> cancelReason = new AtomicReference<>();
    private final AtomicReference<SubscribableListener<Void>> blocked = new AtomicReference<>();
    /**
     * Status reported to the tasks API. We write the status at most once every
     * {@link #statusNanos}, as soon as loop has finished and after {@link #statusNanos}
     * have passed.
     */
    private final AtomicReference<DriverStatus> status;

    /**
     * Creates a new driver with a chain of operators.
     * @param sessionId session Id
     * @param driverContext the driver context
     * @param source source operator
     * @param intermediateOperators  the chain of operators to execute
     * @param sink sink operator
     * @param statusInterval minimum status reporting interval
     * @param releasable a {@link Releasable} to invoked once the chain of operators has run to completion
     */
    public Driver(
        String sessionId,
        DriverContext driverContext,
        Supplier<String> description,
        SourceOperator source,
        List<Operator> intermediateOperators,
        SinkOperator sink,
        TimeValue statusInterval,
        Releasable releasable
    ) {
        this.sessionId = sessionId;
        this.driverContext = driverContext;
        this.description = description;
        this.activeOperators = new ArrayList<>();
        this.activeOperators.add(source);
        this.activeOperators.addAll(intermediateOperators);
        this.activeOperators.add(sink);
        this.statusNanos = statusInterval.nanos();
        this.releasable = releasable;
        this.status = new AtomicReference<>(new DriverStatus(sessionId, System.currentTimeMillis(), DriverStatus.Status.QUEUED, List.of()));
    }

    /**
     * Creates a new driver with a chain of operators.
     * @param driverContext the driver context
     * @param source source operator
     * @param intermediateOperators  the chain of operators to execute
     * @param sink sink operator
     * @param releasable a {@link Releasable} to invoked once the chain of operators has run to completion
     */
    public Driver(
        DriverContext driverContext,
        SourceOperator source,
        List<Operator> intermediateOperators,
        SinkOperator sink,
        Releasable releasable
    ) {
        this("unset", driverContext, () -> null, source, intermediateOperators, sink, DEFAULT_STATUS_INTERVAL, releasable);
    }

    public DriverContext driverContext() {
        return driverContext;
    }

    /**
     * Runs computations on the chain of operators for a given maximum amount of time or iterations.
     * Returns a blocked future when the chain of operators is blocked, allowing the caller
     * thread to do other work instead of blocking or busy-spinning on the blocked operator.
     */
    private SubscribableListener<Void> run(TimeValue maxTime, int maxIterations) {
        long maxTimeNanos = maxTime.nanos();
        long startTime = System.nanoTime();
        long nextStatus = startTime + statusNanos;
        int iter = 0;
        while (isFinished() == false) {
            SubscribableListener<Void> fut = runSingleLoopIteration();
            if (fut.isDone() == false) {
                status.set(updateStatus(DriverStatus.Status.ASYNC));
                return fut;
            }
            if (iter >= maxIterations) {
                break;
            }
            long now = System.nanoTime();
            if (now > nextStatus) {
                status.set(updateStatus(DriverStatus.Status.RUNNING));
                nextStatus = now + statusNanos;
            }
            iter++;
            if (now - startTime > maxTimeNanos) {
                break;
            }
        }
        if (isFinished()) {
            status.set(updateStatus(DriverStatus.Status.DONE));
            driverContext.finish();
            releasable.close();
        } else {
            status.set(updateStatus(DriverStatus.Status.WAITING));
        }
        return Operator.NOT_BLOCKED;
    }

    /**
     * Whether the driver has run the chain of operators to completion.
     */
    public boolean isFinished() {
        return activeOperators.isEmpty();
    }

    @Override
    public void close() {
        drainAndCloseOperators(null);
    }

    private SubscribableListener<Void> runSingleLoopIteration() {
        ensureNotCancelled();
        boolean movedPage = false;

        for (int i = 0; i < activeOperators.size() - 1; i++) {
            Operator op = activeOperators.get(i);
            Operator nextOp = activeOperators.get(i + 1);

            // skip blocked operator
            if (op.isBlocked().isDone() == false) {
                continue;
            }

            if (op.isFinished() == false && nextOp.needsInput()) {
                Page page = op.getOutput();
                if (page == null) {
                    // No result, just move to the next iteration
                } else if (page.getPositionCount() == 0) {
                    // Empty result, release any memory it holds immediately and move to the next iteration
                    page.releaseBlocks();
                } else {
                    // Non-empty result from the previous operation, move it to the next operation
                    nextOp.addInput(page);
                    movedPage = true;
                }
            }

            if (op.isFinished()) {
                nextOp.finish();
            }
        }

        for (int index = activeOperators.size() - 1; index >= 0; index--) {
            if (activeOperators.get(index).isFinished()) {
                /*
                 * Close and remove this operator and all source operators in the
                 * most paranoid possible way. Closing operators shouldn't throw,
                 * but if it does, this will make sure we don't try to close any
                 * that succeed twice.
                 */
                List<Operator> finishedOperators = this.activeOperators.subList(0, index + 1);
                Iterator<Operator> itr = finishedOperators.iterator();
                while (itr.hasNext()) {
                    itr.next().close();
                    itr.remove();
                }

                // Finish the next operator, which is now the first operator.
                if (activeOperators.isEmpty() == false) {
                    Operator newRootOperator = activeOperators.get(0);
                    newRootOperator.finish();
                }
                break;
            }
        }

        if (movedPage == false) {
            return oneOf(
                activeOperators.stream().map(Operator::isBlocked).filter(laf -> laf.isDone() == false).collect(Collectors.toList())
            );
        }
        return Operator.NOT_BLOCKED;
    }

    public void cancel(String reason) {
        if (cancelReason.compareAndSet(null, reason)) {
            synchronized (this) {
                SubscribableListener<Void> fut = this.blocked.get();
                if (fut != null) {
                    fut.onFailure(new TaskCancelledException(reason));
                }
            }
        }
    }

    private boolean isCancelled() {
        return cancelReason.get() != null;
    }

    private void ensureNotCancelled() {
        String reason = cancelReason.get();
        if (reason != null) {
            throw new TaskCancelledException(reason);
        }
    }

    public static void start(Executor executor, Driver driver, int maxIterations, ActionListener<Void> listener) {
        driver.status.set(driver.updateStatus(DriverStatus.Status.STARTING));
        schedule(DEFAULT_TIME_BEFORE_YIELDING, maxIterations, executor, driver, listener);
    }

    // Drains all active operators and closes them.
    private void drainAndCloseOperators(@Nullable Exception e) {
        Iterator<Operator> itr = activeOperators.iterator();
        while (itr.hasNext()) {
            try {
                Releasables.closeWhileHandlingException(itr.next());
            } catch (Exception x) {
                if (e != null) {
                    e.addSuppressed(x);
                }
            }
            itr.remove();
        }
        driverContext.finish();
        Releasables.closeWhileHandlingException(releasable);
    }

    private static void schedule(TimeValue maxTime, int maxIterations, Executor executor, Driver driver, ActionListener<Void> listener) {
        executor.execute(new AbstractRunnable() {
            @Override
            protected void doRun() {
                if (driver.isFinished()) {
                    listener.onResponse(null);
                    return;
                }
                SubscribableListener<Void> fut = driver.run(maxTime, maxIterations);
                if (fut.isDone()) {
                    schedule(maxTime, maxIterations, executor, driver, listener);
                } else {
                    synchronized (driver) {
                        if (driver.isCancelled() == false) {
                            driver.blocked.set(fut);
                        }
                    }
                    fut.addListener(
                        ActionListener.wrap(ignored -> schedule(maxTime, maxIterations, executor, driver, listener), this::onFailure)
                    );
                }
            }

            @Override
            public void onFailure(Exception e) {
                driver.drainAndCloseOperators(e);
                listener.onFailure(e);
            }
        });
    }

    private static SubscribableListener<Void> oneOf(List<SubscribableListener<Void>> futures) {
        if (futures.isEmpty()) {
            return Operator.NOT_BLOCKED;
        }
        if (futures.size() == 1) {
            return futures.get(0);
        }
        SubscribableListener<Void> oneOf = new SubscribableListener<>();
        for (SubscribableListener<Void> fut : futures) {
            fut.addListener(oneOf);
        }
        return oneOf;
    }

    @Override
    public String toString() {
        return this.getClass().getSimpleName() + "[activeOperators=" + activeOperators + "]";
    }

    @Override
    public String describe() {
        return description.get();
    }

    public String sessionId() {
        return sessionId;
    }

    /**
     * Get the last status update from the driver. These updates are made
     * when the driver is queued and after every
     * processing {@link #run(TimeValue, int) batch}.
     */
    public DriverStatus status() {
        return status.get();
    }

    /**
     * Update the status.
     * @param status the status of the overall driver request
     */
    private DriverStatus updateStatus(DriverStatus.Status status) {
        return new DriverStatus(
            sessionId,
            System.currentTimeMillis(),
            status,
            activeOperators.stream().map(o -> new DriverStatus.OperatorStatus(o.toString(), o.status())).toList()
        );
    }
}
