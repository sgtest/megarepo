/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.inference.allocation;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.cluster.ClusterChangedEvent;
import org.elasticsearch.cluster.ClusterStateListener;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.component.LifecycleListener;
import org.elasticsearch.common.util.set.Sets;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.tasks.TaskAwareRequest;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.tasks.TaskManager;
import org.elasticsearch.threadpool.Scheduler;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.ml.MlMetadata;
import org.elasticsearch.xpack.core.ml.action.StartTrainedModelDeploymentAction;
import org.elasticsearch.xpack.core.ml.action.UpdateTrainedModelAllocationStateAction;
import org.elasticsearch.xpack.core.ml.inference.allocation.RoutingState;
import org.elasticsearch.xpack.core.ml.inference.allocation.RoutingStateAndReason;
import org.elasticsearch.xpack.core.ml.inference.allocation.TrainedModelAllocation;
import org.elasticsearch.xpack.core.ml.inference.results.InferenceResults;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.ml.MachineLearning;
import org.elasticsearch.xpack.ml.inference.deployment.DeploymentManager;
import org.elasticsearch.xpack.ml.inference.deployment.ModelStats;
import org.elasticsearch.xpack.ml.inference.deployment.TrainedModelDeploymentTask;

import java.util.ArrayList;
import java.util.Deque;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentLinkedDeque;

import static org.elasticsearch.xpack.core.ml.MlTasks.TRAINED_MODEL_ALLOCATION_TASK_NAME_PREFIX;
import static org.elasticsearch.xpack.core.ml.MlTasks.TRAINED_MODEL_ALLOCATION_TASK_TYPE;

public class TrainedModelAllocationNodeService implements ClusterStateListener {

    private static final String NODE_NO_LONGER_REFERENCED = "node no longer referenced in model routing table";
    private static final String ALLOCATION_NO_LONGER_EXISTS = "model allocation no longer exists";
    private static final TimeValue MODEL_LOADING_CHECK_INTERVAL = TimeValue.timeValueSeconds(1);
    private static final Logger logger = LogManager.getLogger(TrainedModelAllocationNodeService.class);
    private final TrainedModelAllocationService trainedModelAllocationService;
    private final DeploymentManager deploymentManager;
    private final TaskManager taskManager;
    private final Map<String, TrainedModelDeploymentTask> modelIdToTask;
    private final ThreadPool threadPool;
    private final Deque<TrainedModelDeploymentTask> loadingModels;
    private volatile Scheduler.Cancellable scheduledFuture;
    private volatile boolean stopped;
    private volatile String nodeId;

    public TrainedModelAllocationNodeService(
        TrainedModelAllocationService trainedModelAllocationService,
        ClusterService clusterService,
        DeploymentManager deploymentManager,
        TaskManager taskManager,
        ThreadPool threadPool
    ) {
        this.trainedModelAllocationService = trainedModelAllocationService;
        this.deploymentManager = deploymentManager;
        this.taskManager = taskManager;
        this.modelIdToTask = new ConcurrentHashMap<>();
        this.loadingModels = new ConcurrentLinkedDeque<>();
        this.threadPool = threadPool;
        clusterService.addLifecycleListener(new LifecycleListener() {
            @Override
            public void afterStart() {
                nodeId = clusterService.localNode().getId();
                start();
            }

            @Override
            public void beforeStop() {
                stop();
            }
        });
    }

    TrainedModelAllocationNodeService(
        TrainedModelAllocationService trainedModelAllocationService,
        ClusterService clusterService,
        DeploymentManager deploymentManager,
        TaskManager taskManager,
        ThreadPool threadPool,
        String nodeId
    ) {
        this.trainedModelAllocationService = trainedModelAllocationService;
        this.deploymentManager = deploymentManager;
        this.taskManager = taskManager;
        this.modelIdToTask = new ConcurrentHashMap<>();
        this.loadingModels = new ConcurrentLinkedDeque<>();
        this.threadPool = threadPool;
        this.nodeId = nodeId;
        clusterService.addLifecycleListener(new LifecycleListener() {
            @Override
            public void afterStart() {
                start();
            }

            @Override
            public void beforeStop() {
                stop();
            }
        });
    }

    void stopDeploymentAsync(TrainedModelDeploymentTask task, String reason, ActionListener<Void> listener) {
        if (stopped) {
            return;
        }
        task.stopWithoutNotification(reason);
        threadPool.executor(MachineLearning.UTILITY_THREAD_POOL_NAME).execute(() -> {
            try {
                deploymentManager.stopDeployment(task);
                taskManager.unregister(task);
                modelIdToTask.remove(task.getModelId());
                listener.onResponse(null);
            } catch (Exception e) {
                listener.onFailure(e);
            }
        });
    }

    public void start() {
        stopped = false;
        scheduledFuture = threadPool.scheduleWithFixedDelay(
            this::loadQueuedModels,
            MODEL_LOADING_CHECK_INTERVAL,
            MachineLearning.UTILITY_THREAD_POOL_NAME
        );
    }

    public void stop() {
        stopped = true;
        ThreadPool.Cancellable cancellable = this.scheduledFuture;
        if (cancellable != null) {
            cancellable.cancel();
        }
    }

    void loadQueuedModels() {
        TrainedModelDeploymentTask loadingTask;
        logger.trace("attempting to load all currently queued models");
        // NOTE: As soon as this method exits, the timer for the scheduler starts ticking
        while ((loadingTask = loadingModels.poll()) != null) {
            final String modelId = loadingTask.getModelId();
            if (loadingTask.isStopped()) {
                if (logger.isTraceEnabled()) {
                    String reason = loadingTask.stoppedReason().orElse("_unknown_");
                    logger.trace("[{}] attempted to load stopped task with reason [{}]", modelId, reason);
                }
                continue;
            }
            if (stopped) {
                return;
            }
            logger.trace(() -> new ParameterizedMessage("[{}] attempting to load model", modelId));
            final PlainActionFuture<TrainedModelDeploymentTask> listener = new PlainActionFuture<>();
            deploymentManager.startDeployment(loadingTask, listener);
            try {
                // This needs to be synchronous here in the utility thread to keep queueing order
                TrainedModelDeploymentTask deployedTask = listener.actionGet();
                // kicks off asynchronous cluster state update
                handleLoadSuccess(deployedTask);
            } catch (Exception ex) {
                // kicks off asynchronous cluster state update
                handleLoadFailure(loadingTask, ex);
            }
        }
    }

    public void stopDeploymentAndNotify(TrainedModelDeploymentTask task, String reason) {
        ActionListener<Void> notifyDeploymentOfStopped = ActionListener.wrap(
            stopped -> updateStoredState(
                task.getModelId(),
                new RoutingStateAndReason(RoutingState.STOPPED, reason),
                ActionListener.wrap(s -> {}, failure -> {})
            ),
            failed -> { // if we failed to stop the process, something strange is going on, but we should still notify of stop
                logger.warn(() -> new ParameterizedMessage("[{}] failed to stop due to error", task.getModelId()), failed);
                updateStoredState(
                    task.getModelId(),
                    new RoutingStateAndReason(RoutingState.STOPPED, reason),
                    ActionListener.wrap(s -> {}, failure -> {})
                );
            }
        );
        updateStoredState(
            task.getModelId(),
            new RoutingStateAndReason(RoutingState.STOPPING, reason),
            ActionListener.wrap(success -> stopDeploymentAsync(task, "task locally canceled", notifyDeploymentOfStopped), e -> {
                if (ExceptionsHelper.unwrapCause(e) instanceof ResourceNotFoundException) {
                    logger.debug(
                        () -> new ParameterizedMessage(
                            "[{}] failed to set routing state to stopping as allocation already removed",
                            task.getModelId()
                        ),
                        e
                    );
                } else {
                    // this is an unexpected error
                    // TODO this means requests may still be routed here, should we not stop deployment?
                    logger.warn(
                        () -> new ParameterizedMessage("[{}] failed to set routing state to stopping due to error", task.getModelId()),
                        e
                    );
                }
                stopDeploymentAsync(task,  reason, notifyDeploymentOfStopped);
            })
        );
    }

    public void infer(TrainedModelDeploymentTask task, Map<String, Object> doc, TimeValue timeout,
                      ActionListener<InferenceResults> listener) {
        deploymentManager.infer(task, doc, timeout, listener);
    }

    public Optional<ModelStats> modelStats(TrainedModelDeploymentTask task) {
        return deploymentManager.getStats(task);
    }

    private TaskAwareRequest taskAwareRequest(StartTrainedModelDeploymentAction.TaskParams params) {
        final TrainedModelAllocationNodeService trainedModelAllocationNodeService = this;
        return new TaskAwareRequest() {
            @Override
            public void setParentTask(TaskId taskId) {
                throw new UnsupportedOperationException("parent task id for model allocation tasks shouldn't change");
            }

            @Override
            public TaskId getParentTask() {
                return TaskId.EMPTY_TASK_ID;
            }

            @Override
            public Task createTask(long id, String type, String action, TaskId parentTaskId, Map<String, String> headers) {
                return new TrainedModelDeploymentTask(id, type, action, parentTaskId, headers, params, trainedModelAllocationNodeService);
            }
        };
    }

    @Override
    public void clusterChanged(ClusterChangedEvent event) {
        if (event.metadataChanged()) {
            final boolean isResetMode = MlMetadata.getMlMetadata(event.state()).isResetMode();
            TrainedModelAllocationMetadata modelAllocationMetadata = TrainedModelAllocationMetadata.fromState(event.state());
            final String currentNode = event.state().nodes().getLocalNodeId();
            for (TrainedModelAllocation trainedModelAllocation : modelAllocationMetadata.modelAllocations().values()) {
                RoutingStateAndReason routingStateAndReason = trainedModelAllocation.getNodeRoutingTable().get(currentNode);
                // Add new models to start loading
                if (routingStateAndReason != null
                    // periodic retries should be handled in a separate thread think
                    && routingStateAndReason.getState().equals(RoutingState.STARTING)
                    // This means we don't already have a task and should attempt creating one and starting the model loading
                    && modelIdToTask.containsKey(trainedModelAllocation.getTaskParams().getModelId()) == false
                    // If we are in reset mode, don't start loading a new model on this node.
                    && isResetMode == false) {
                    prepareModelToLoad(trainedModelAllocation.getTaskParams());
                }
                // This mode is not routed to the current node at all
                if (routingStateAndReason == null) {
                    TrainedModelDeploymentTask task = modelIdToTask.remove(trainedModelAllocation.getTaskParams().getModelId());
                    if (task != null) {
                        stopDeploymentAsync(
                            task,
                            NODE_NO_LONGER_REFERENCED,
                            ActionListener.wrap(
                                r -> logger.trace(() -> new ParameterizedMessage("[{}] stopped deployment", task.getModelId())),
                                e -> logger.warn(
                                    () -> new ParameterizedMessage("[{}] failed to fully stop deployment", task.getModelId()),
                                    e
                                )
                            )
                        );
                    }
                }
            }
            List<TrainedModelDeploymentTask> toCancel = new ArrayList<>();
            for (String modelIds : Sets.difference(modelIdToTask.keySet(), modelAllocationMetadata.modelAllocations().keySet())) {
                toCancel.add(modelIdToTask.remove(modelIds));
            }
            // should all be stopped in the same executor thread?
            for (TrainedModelDeploymentTask t : toCancel) {
                stopDeploymentAsync(
                    t,
                    ALLOCATION_NO_LONGER_EXISTS,
                    ActionListener.wrap(
                        r -> logger.trace(() -> new ParameterizedMessage("[{}] stopped deployment", t.getModelId())),
                        e -> logger.warn(() -> new ParameterizedMessage("[{}] failed to fully stop deployment", t.getModelId()), e)
                    )
                );
            }
        }
    }

    // For testing purposes
    TrainedModelDeploymentTask getTask(String modelId) {
        return modelIdToTask.get(modelId);
    }

    void prepareModelToLoad(StartTrainedModelDeploymentAction.TaskParams taskParams) {
        TrainedModelDeploymentTask task = (TrainedModelDeploymentTask) taskManager.register(
            TRAINED_MODEL_ALLOCATION_TASK_TYPE,
            TRAINED_MODEL_ALLOCATION_TASK_NAME_PREFIX + taskParams.getModelId(),
            taskAwareRequest(taskParams)
        );
        // threadsafe check to verify we are not loading/loaded the model
        if (modelIdToTask.putIfAbsent(taskParams.getModelId(), task) == null) {
            loadingModels.add(task);
        } else {
            // If there is already a task for the model, unregister the new task
            taskManager.unregister(task);
        }
    }

    private void handleLoadSuccess(TrainedModelDeploymentTask task) {
        final String modelId = task.getModelId();
        logger.debug(
            () -> new ParameterizedMessage("[{}] model successfully loaded and ready for inference. Notifying master node", modelId)
        );
        if (task.isStopped()) {
            logger.debug(
                () -> new ParameterizedMessage(
                    "[{}] model loaded successfully, but stopped before routing table was updated; reason [{}]",
                    modelId,
                    task.stoppedReason().orElse("_unknown_")
                )
            );
            return;
        }
        updateStoredState(
            modelId,
            new RoutingStateAndReason(RoutingState.STARTED, ""),
            ActionListener.wrap(
                r -> logger.debug(() -> new ParameterizedMessage("[{}] model loaded and accepting routes", modelId)),
                e -> {
                    // This means that either the allocation has been deleted, or this node's particular route has been removed
                    if (ExceptionsHelper.unwrapCause(e) instanceof ResourceNotFoundException) {
                        logger.debug(
                            () -> new ParameterizedMessage(
                                "[{}] model loaded but failed to start accepting routes as allocation to this node was removed",
                                modelId
                            ),
                            e
                        );
                    }
                    // this is an unexpected error
                    logger.warn(() -> new ParameterizedMessage("[{}] model loaded but failed to start accepting routes", modelId), e);
                }
            )
        );
    }

    private void updateStoredState(
        String modelId,
        RoutingStateAndReason routingStateAndReason,
        ActionListener<AcknowledgedResponse> listener
    ) {
        if (stopped) {
            return;
        }
        trainedModelAllocationService.updateModelAllocationState(
            new UpdateTrainedModelAllocationStateAction.Request(nodeId, modelId, routingStateAndReason),
            ActionListener.wrap(success -> {
                logger.debug(
                    () -> new ParameterizedMessage("[{}] model is [{}] and master notified", modelId, routingStateAndReason.getState())
                );
                listener.onResponse(AcknowledgedResponse.TRUE);
            },
                error -> {
                    logger.warn(
                        () -> new ParameterizedMessage(
                            "[{}] model is [{}] but failed to notify master",
                            modelId,
                            routingStateAndReason.getState()
                        ),
                        error
                    );
                    listener.onFailure(error);
                }
            )
        );
    }

    private void handleLoadFailure(TrainedModelDeploymentTask task, Exception ex) {
        logger.error(() -> new ParameterizedMessage("[{}] model failed to load", task.getModelId()), ex);
        if (task.isStopped()) {
            logger.debug(() -> new ParameterizedMessage(
                "[{}] model failed to load, but is now stopped; reason [{}]",
                task.getModelId(),
                task.stoppedReason().orElse("_unknown_")
            ));
        }
        // TODO: Do we want to stop the task? This would cause it to be reloaded by state updates on INITIALIZING
        // We should stop the local task so that future task actions won't get routed to the older one.
        Runnable stopTask = () -> stopDeploymentAsync(task, "model failed to load; reason [" + ex.getMessage() + "]",
            ActionListener.wrap(r -> {}, e -> {}));
        updateStoredState(
            task.getModelId(),
            new RoutingStateAndReason(RoutingState.FAILED, ExceptionsHelper.unwrapCause(ex).getMessage()),
            ActionListener.wrap(r -> stopTask.run(), e -> stopTask.run())
        );
    }
}
