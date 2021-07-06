/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.persistent;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionType;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.admin.cluster.node.tasks.cancel.CancelTasksRequest;
import org.elasticsearch.action.admin.cluster.node.tasks.cancel.CancelTasksResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.client.OriginSettingClient;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateObserver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.node.NodeClosedException;
import org.elasticsearch.persistent.PersistentTasksCustomMetadata.PersistentTask;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.threadpool.ThreadPool;

import java.util.function.Predicate;

import static org.elasticsearch.persistent.CompletionPersistentTaskAction.LOCAL_ABORT_AVAILABLE_VERSION;

/**
 * This service is used by persistent tasks and allocated persistent tasks to communicate changes
 * to the master node so that the master can update the cluster state and can track of the states
 * of the persistent tasks.
 */
public class PersistentTasksService {

    private static final Logger logger = LogManager.getLogger(PersistentTasksService.class);

    public static final String PERSISTENT_TASK_ORIGIN = "persistent_tasks";

    private final Client client;
    private final ClusterService clusterService;
    private final ThreadPool threadPool;

    public PersistentTasksService(ClusterService clusterService, ThreadPool threadPool, Client client) {
        this.client = new OriginSettingClient(client, PERSISTENT_TASK_ORIGIN);
        this.clusterService = clusterService;
        this.threadPool = threadPool;
    }

    /**
     * Notifies the master node to create new persistent task and to assign it to a node.
     */
    public <Params extends PersistentTaskParams> void sendStartRequest(final String taskId,
                                                                       final String taskName,
                                                                       final Params taskParams,
                                                                       final ActionListener<PersistentTask<Params>> listener) {
        @SuppressWarnings("unchecked")
        final ActionListener<PersistentTask<?>> wrappedListener = listener.map(t -> (PersistentTask<Params>) t);
        StartPersistentTaskAction.Request request = new StartPersistentTaskAction.Request(taskId, taskName, taskParams);
        execute(request, StartPersistentTaskAction.INSTANCE, wrappedListener);
    }

    /**
     * Notifies the master node about the completion of a persistent task.
     * <p>
     * At most one of {@code failure} and {@code localAbortReason} may be
     * provided. When both {@code failure} and {@code localAbortReason} are
     * {@code null}, the persistent task is considered as successfully completed.
     * {@code localAbortReason} must not be provided unless all nodes in the cluster
     * are on version {@link CompletionPersistentTaskAction#LOCAL_ABORT_AVAILABLE_VERSION}
     * or higher.
     */
    public void sendCompletionRequest(final String taskId,
                                      final long taskAllocationId,
                                      final @Nullable Exception taskFailure,
                                      final @Nullable String localAbortReason,
                                      final ActionListener<PersistentTask<?>> listener) {
        if (localAbortReason != null) {
            validateLocalAbortSupported();
        }
        CompletionPersistentTaskAction.Request request =
            new CompletionPersistentTaskAction.Request(taskId, taskAllocationId, taskFailure, localAbortReason);
        execute(request, CompletionPersistentTaskAction.INSTANCE, listener);
    }

    /**
     * Cancels a locally running task using the Task Manager API
     */
    void sendCancelRequest(final long taskId, final String reason, final ActionListener<CancelTasksResponse> listener) {
        CancelTasksRequest request = new CancelTasksRequest();
        request.setTaskId(new TaskId(clusterService.localNode().getId(), taskId));
        request.setReason(reason);
        try {
            client.admin().cluster().cancelTasks(request, listener);
        } catch (Exception e) {
            listener.onFailure(e);
        }
    }

    /**
     * Notifies the master node that the state of a persistent task has changed.
     * <p>
     * Persistent task implementers shouldn't call this method directly and use
     * {@link AllocatedPersistentTask#updatePersistentTaskState} instead
     */
    void sendUpdateStateRequest(final String taskId,
                                final long taskAllocationID,
                                final PersistentTaskState taskState,
                                final ActionListener<PersistentTask<?>> listener) {
        UpdatePersistentTaskStatusAction.Request request =
            new UpdatePersistentTaskStatusAction.Request(taskId, taskAllocationID, taskState);
        execute(request, UpdatePersistentTaskStatusAction.INSTANCE, listener);
    }

    /**
     * Notifies the master node to remove a persistent task from the cluster state
     */
    public void sendRemoveRequest(final String taskId, final ActionListener<PersistentTask<?>> listener) {
        RemovePersistentTaskAction.Request request = new RemovePersistentTaskAction.Request(taskId);
        execute(request, RemovePersistentTaskAction.INSTANCE, listener);
    }

    /**
     * Is the cluster able to support locally aborting persistent tasks?
     * This requires that every node in the cluster is on version
     * {@link CompletionPersistentTaskAction#LOCAL_ABORT_AVAILABLE_VERSION}
     * or above.
     */
    public boolean isLocalAbortSupported() {
        return clusterService.state().nodes().getMinNodeVersion().onOrAfter(LOCAL_ABORT_AVAILABLE_VERSION);
    }

    /**
     * Throw an exception if the cluster is not able locally abort persistent tasks.
     */
    public void validateLocalAbortSupported() {
        Version minNodeVersion = clusterService.state().nodes().getMinNodeVersion();
        if (minNodeVersion.before(LOCAL_ABORT_AVAILABLE_VERSION)) {
            throw new IllegalStateException("attempt to abort a persistent task locally in a cluster that does not support this: "
                + "minimum node version [" + minNodeVersion + "], version required [" + LOCAL_ABORT_AVAILABLE_VERSION + "]");
        }
    }

    /**
     * Executes an asynchronous persistent task action using the client.
     * <p>
     * The origin is set in the context and the listener is wrapped to ensure the proper context is restored
     */
    private <Req extends ActionRequest, Resp extends PersistentTaskResponse>
        void execute(final Req request, final ActionType<Resp> action, final ActionListener<PersistentTask<?>> listener) {
            try {
                client.execute(action, request, listener.map(PersistentTaskResponse::getTask));
            } catch (Exception e) {
                listener.onFailure(e);
            }
    }

    /**
     * Waits for a given persistent task to comply with a given predicate, then call back the listener accordingly.
     *
     * @param taskId the persistent task id
     * @param predicate the persistent task predicate to evaluate
     * @param timeout a timeout for waiting
     * @param listener the callback listener
     */
    public void waitForPersistentTaskCondition(final String taskId,
                                               final Predicate<PersistentTask<?>> predicate,
                                               final @Nullable TimeValue timeout,
                                               final WaitForPersistentTaskListener<?> listener) {
        final Predicate<ClusterState> clusterStatePredicate = clusterState ->
            predicate.test(PersistentTasksCustomMetadata.getTaskWithId(clusterState, taskId));

        final ClusterStateObserver observer = new ClusterStateObserver(clusterService, timeout, logger, threadPool.getThreadContext());
        final ClusterState clusterState = observer.setAndGetObservedState();
        if (clusterStatePredicate.test(clusterState)) {
            listener.onResponse(PersistentTasksCustomMetadata.getTaskWithId(clusterState, taskId));
        } else {
            observer.waitForNextChange(new ClusterStateObserver.Listener() {
                @Override
                public void onNewClusterState(ClusterState state) {
                    listener.onResponse(PersistentTasksCustomMetadata.getTaskWithId(state, taskId));
                }

                @Override
                public void onClusterServiceClose() {
                    listener.onFailure(new NodeClosedException(clusterService.localNode()));
                }

                @Override
                public void onTimeout(TimeValue timeout) {
                    listener.onTimeout(timeout);
                }
            }, clusterStatePredicate);
        }
    }

    /**
     * Waits for persistent tasks to comply with a given predicate, then call back the listener accordingly.
     *
     * @param predicate the predicate to evaluate
     * @param timeout a timeout for waiting
     * @param listener the callback listener
     */
    public void waitForPersistentTasksCondition(final Predicate<PersistentTasksCustomMetadata> predicate,
                                                final @Nullable TimeValue timeout,
                                                final ActionListener<Boolean> listener) {
        final Predicate<ClusterState> clusterStatePredicate = clusterState ->
            predicate.test(clusterState.metadata().custom(PersistentTasksCustomMetadata.TYPE));

        final ClusterStateObserver observer = new ClusterStateObserver(clusterService, timeout, logger, threadPool.getThreadContext());
        if (clusterStatePredicate.test(observer.setAndGetObservedState())) {
            listener.onResponse(true);
        } else {
            observer.waitForNextChange(new ClusterStateObserver.Listener() {
                @Override
                public void onNewClusterState(ClusterState state) {
                    listener.onResponse(true);
                }

                @Override
                public void onClusterServiceClose() {
                    listener.onFailure(new NodeClosedException(clusterService.localNode()));
                }

                @Override
                public void onTimeout(TimeValue timeout) {
                    listener.onFailure(new IllegalStateException("Timed out when waiting for persistent tasks after " + timeout));
                }
            }, clusterStatePredicate, timeout);
        }
    }

    public interface WaitForPersistentTaskListener<P extends PersistentTaskParams> extends ActionListener<PersistentTask<P>> {
        default void onTimeout(TimeValue timeout) {
            onFailure(new IllegalStateException("Timed out when waiting for persistent task after " + timeout));
        }
    }
}
