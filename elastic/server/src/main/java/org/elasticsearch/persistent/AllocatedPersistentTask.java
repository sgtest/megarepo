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
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.cluster.node.tasks.cancel.CancelTasksRequest;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.tasks.CancellableTask;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.tasks.TaskManager;

import java.util.Map;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.Predicate;

/**
 * Represents a executor node operation that corresponds to a persistent task
 */
public class AllocatedPersistentTask extends CancellableTask {

    private static final Logger logger = LogManager.getLogger(AllocatedPersistentTask.class);
    private final AtomicReference<State> state;

    private volatile String persistentTaskId;
    private volatile long allocationId;
    private volatile @Nullable Exception failure;
    private volatile PersistentTasksService persistentTasksService;
    private volatile TaskManager taskManager;

    public AllocatedPersistentTask(long id, String type, String action, String description, TaskId parentTask,
                                   Map<String, String> headers) {
        super(id, type, action, description, parentTask, headers);
        this.state = new AtomicReference<>(State.STARTED);
    }

    @Override
    public Status getStatus() {
        return new PersistentTasksNodeService.Status(state.get());
    }

    /**
     * Updates the persistent state for the corresponding persistent task.
     * <p>
     * This doesn't affect the status of this allocated task.
     */
    public void updatePersistentTaskState(final PersistentTaskState state,
                                          final ActionListener<PersistentTasksCustomMetadata.PersistentTask<?>> listener) {
        persistentTasksService.sendUpdateStateRequest(persistentTaskId, allocationId, state, listener);
    }

    public String getPersistentTaskId() {
        return persistentTaskId;
    }

    protected void init(PersistentTasksService persistentTasksService, TaskManager taskManager,
                        String persistentTaskId, long allocationId) {
        this.persistentTasksService = persistentTasksService;
        this.taskManager = taskManager;
        this.persistentTaskId = persistentTaskId;
        this.allocationId = allocationId;
    }

    public Exception getFailure() {
        return failure;
    }

    public long getAllocationId() {
        return allocationId;
    }

    /**
     * Waits for a given persistent task to comply with a given predicate, then call back the listener accordingly.
     *
     * @param predicate the persistent task predicate to evaluate
     * @param timeout a timeout for waiting
     * @param listener the callback listener
     */
    public void waitForPersistentTask(final Predicate<PersistentTasksCustomMetadata.PersistentTask<?>> predicate,
                                      final @Nullable TimeValue timeout,
                                      final PersistentTasksService.WaitForPersistentTaskListener<?> listener) {
        persistentTasksService.waitForPersistentTaskCondition(persistentTaskId, predicate, timeout, listener);
    }

    protected final boolean isCompleted() {
        return state.get() == State.COMPLETED;
    }

    boolean markAsCancelled() {
        return state.compareAndSet(State.STARTED, State.PENDING_CANCEL);
    }

    public void markAsCompleted() {
        completeAndNotifyIfNeeded(null);
    }

    public void markAsFailed(Exception e) {
        if (CancelTasksRequest.DEFAULT_REASON.equals(getReasonCancelled())) {
            completeAndNotifyIfNeeded(null);
        } else {
            completeAndNotifyIfNeeded(e);
        }
    }

    private void completeAndNotifyIfNeeded(@Nullable Exception failure) {
        final State prevState = state.getAndSet(State.COMPLETED);
        if (prevState == State.COMPLETED) {
            logger.warn("attempt to complete task [{}] with id [{}] in the [{}] state", getAction(), getPersistentTaskId(), prevState);
        } else {
            if (failure != null) {
                logger.warn(() -> new ParameterizedMessage("task {} failed with an exception", getPersistentTaskId()), failure);
            }
            try {
                this.failure = failure;
                if (prevState == State.STARTED) {
                    logger.trace("sending notification for completed task [{}] with id [{}]", getAction(), getPersistentTaskId());
                    persistentTasksService.sendCompletionRequest(getPersistentTaskId(), getAllocationId(), failure, new
                            ActionListener<PersistentTasksCustomMetadata.PersistentTask<?>>() {
                                @Override
                                public void onResponse(PersistentTasksCustomMetadata.PersistentTask<?> persistentTask) {
                                    logger.trace("notification for task [{}] with id [{}] was successful", getAction(),
                                            getPersistentTaskId());
                                }

                                @Override
                                public void onFailure(Exception e) {
                                    logger.warn(() -> new ParameterizedMessage(
                                        "notification for task [{}] with id [{}] failed", getAction(), getPersistentTaskId()), e);
                                }
                            });
                }
            } finally {
                taskManager.unregister(this);
            }
        }
    }

    public enum State {
        STARTED,  // the task is currently running
        PENDING_CANCEL, // the task is cancelled on master, cancelling it locally
        COMPLETED     // the task is done running and trying to notify caller
    }
}
