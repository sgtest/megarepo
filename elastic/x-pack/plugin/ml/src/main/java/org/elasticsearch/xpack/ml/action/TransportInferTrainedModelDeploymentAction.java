/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.action;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.FailedNodeException;
import org.elasticsearch.action.TaskOperationFailure;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.tasks.TransportTasksAction;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.persistent.PersistentTasksCustomMetadata;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.ml.MlTasks;
import org.elasticsearch.xpack.core.ml.action.InferTrainedModelDeploymentAction;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.ml.inference.deployment.TrainedModelDeploymentTask;

import java.util.List;

public class TransportInferTrainedModelDeploymentAction extends TransportTasksAction<TrainedModelDeploymentTask,
    InferTrainedModelDeploymentAction.Request, InferTrainedModelDeploymentAction.Response, InferTrainedModelDeploymentAction.Response> {

    @Inject
    public TransportInferTrainedModelDeploymentAction(ClusterService clusterService, TransportService transportService,
                                                      ActionFilters actionFilters) {
        super(InferTrainedModelDeploymentAction.NAME, clusterService, transportService, actionFilters,
            InferTrainedModelDeploymentAction.Request::new, InferTrainedModelDeploymentAction.Response::new,
            InferTrainedModelDeploymentAction.Response::new, ThreadPool.Names.SAME);
    }

    @Override
    protected void doExecute(Task task, InferTrainedModelDeploymentAction.Request request,
                             ActionListener<InferTrainedModelDeploymentAction.Response> listener) {
        String deploymentId = request.getDeploymentId();
        // We need to check whether there is at least an assigned task here, otherwise we cannot redirect to the
        // node running the job task.
        PersistentTasksCustomMetadata tasks = clusterService.state().getMetadata().custom(PersistentTasksCustomMetadata.TYPE);
        PersistentTasksCustomMetadata.PersistentTask<?> deploymentTask = MlTasks.getTrainedModelDeploymentTask(deploymentId, tasks);
        if (deploymentTask == null || deploymentTask.isAssigned() == false) {
            String message = "Cannot perform requested action because deployment [" + deploymentId + "] is not started";
            listener.onFailure(ExceptionsHelper.conflictStatusException(message));
        } else {
            request.setNodes(deploymentTask.getExecutorNode());
            super.doExecute(task, request, listener);
        }
    }

    @Override
    protected InferTrainedModelDeploymentAction.Response newResponse(InferTrainedModelDeploymentAction.Request request,
                                                                     List<InferTrainedModelDeploymentAction.Response> tasks,
                                                                     List<TaskOperationFailure> taskOperationFailures,
                                                                     List<FailedNodeException> failedNodeExceptions) {
        if (taskOperationFailures.isEmpty() == false) {
            throw org.elasticsearch.ExceptionsHelper.convertToElastic(taskOperationFailures.get(0).getCause());
        } else if (failedNodeExceptions.isEmpty() == false) {
            throw org.elasticsearch.ExceptionsHelper.convertToElastic(failedNodeExceptions.get(0));
        } else {
            return tasks.get(0);
        }
    }

    @Override
    protected void taskOperation(InferTrainedModelDeploymentAction.Request request, TrainedModelDeploymentTask task,
                                 ActionListener<InferTrainedModelDeploymentAction.Response> listener) {
        task.infer(request.getInput(), request.getTimeout(),
            ActionListener.wrap(
                pyTorchResult -> listener.onResponse(new InferTrainedModelDeploymentAction.Response(pyTorchResult)),
                listener::onFailure)
        );
    }
}
