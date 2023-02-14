/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.ml.action;

import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.util.concurrent.AtomicArray;
import org.elasticsearch.core.Tuple;
import org.elasticsearch.license.License;
import org.elasticsearch.license.LicenseUtils;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.tasks.CancellableTask;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.tasks.TaskCancelledException;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.XPackField;
import org.elasticsearch.xpack.core.ml.action.GetTrainedModelsAction;
import org.elasticsearch.xpack.core.ml.action.InferModelAction;
import org.elasticsearch.xpack.core.ml.action.InferModelAction.Request;
import org.elasticsearch.xpack.core.ml.action.InferModelAction.Response;
import org.elasticsearch.xpack.core.ml.action.InferTrainedModelDeploymentAction;
import org.elasticsearch.xpack.core.ml.inference.TrainedModelType;
import org.elasticsearch.xpack.core.ml.inference.assignment.AssignmentState;
import org.elasticsearch.xpack.core.ml.inference.assignment.TrainedModelAssignment;
import org.elasticsearch.xpack.core.ml.inference.results.InferenceResults;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.ml.MachineLearning;
import org.elasticsearch.xpack.ml.inference.ModelAliasMetadata;
import org.elasticsearch.xpack.ml.inference.assignment.TrainedModelAssignmentMetadata;
import org.elasticsearch.xpack.ml.inference.loadingservice.LocalModel;
import org.elasticsearch.xpack.ml.inference.loadingservice.ModelLoadingService;
import org.elasticsearch.xpack.ml.inference.persistence.TrainedModelProvider;
import org.elasticsearch.xpack.ml.utils.TypedChainTaskExecutor;

import java.util.List;
import java.util.Optional;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

import static org.elasticsearch.core.Strings.format;
import static org.elasticsearch.xpack.core.ClientHelper.ML_ORIGIN;
import static org.elasticsearch.xpack.core.ClientHelper.executeAsyncWithOrigin;

public class TransportInternalInferModelAction extends HandledTransportAction<Request, Response> {

    private final ModelLoadingService modelLoadingService;
    private final Client client;
    private final ClusterService clusterService;
    private final XPackLicenseState licenseState;
    private final TrainedModelProvider trainedModelProvider;

    TransportInternalInferModelAction(
        String actionName,
        TransportService transportService,
        ActionFilters actionFilters,
        ModelLoadingService modelLoadingService,
        Client client,
        ClusterService clusterService,
        XPackLicenseState licenseState,
        TrainedModelProvider trainedModelProvider
    ) {
        super(actionName, transportService, actionFilters, InferModelAction.Request::new);
        this.modelLoadingService = modelLoadingService;
        this.client = client;
        this.clusterService = clusterService;
        this.licenseState = licenseState;
        this.trainedModelProvider = trainedModelProvider;
    }

    @Inject
    public TransportInternalInferModelAction(
        TransportService transportService,
        ActionFilters actionFilters,
        ModelLoadingService modelLoadingService,
        Client client,
        ClusterService clusterService,
        XPackLicenseState licenseState,
        TrainedModelProvider trainedModelProvider
    ) {
        this(
            InferModelAction.NAME,
            transportService,
            actionFilters,
            modelLoadingService,
            client,
            clusterService,
            licenseState,
            trainedModelProvider
        );
    }

    @Override
    protected void doExecute(Task task, Request request, ActionListener<Response> listener) {

        Response.Builder responseBuilder = Response.builder();
        TaskId parentTaskId = new TaskId(clusterService.localNode().getId(), task.getId());

        if (MachineLearning.INFERENCE_AGG_FEATURE.check(licenseState)) {
            responseBuilder.setLicensed(true);
            doInfer(task, request, responseBuilder, parentTaskId, listener);
        } else {
            trainedModelProvider.getTrainedModel(
                request.getModelId(),
                GetTrainedModelsAction.Includes.empty(),
                parentTaskId,
                ActionListener.wrap(trainedModelConfig -> {
                    // Since we just checked MachineLearningField.ML_API_FEATURE.check(licenseState) and that check failed
                    // That means we don't have a plat+ license. The only licenses for trained models are basic (free) and plat.
                    boolean allowed = trainedModelConfig.getLicenseLevel() == License.OperationMode.BASIC;
                    responseBuilder.setLicensed(allowed);
                    if (allowed || request.isPreviouslyLicensed()) {
                        doInfer(task, request, responseBuilder, parentTaskId, listener);
                    } else {
                        listener.onFailure(LicenseUtils.newComplianceException(XPackField.MACHINE_LEARNING));
                    }
                }, listener::onFailure)
            );
        }
    }

    private void doInfer(
        Task task,
        Request request,
        Response.Builder responseBuilder,
        TaskId parentTaskId,
        ActionListener<Response> listener
    ) {
        String concreteModelId = Optional.ofNullable(ModelAliasMetadata.fromState(clusterService.state()).getModelId(request.getModelId()))
            .orElse(request.getModelId());

        responseBuilder.setModelId(concreteModelId);

        TrainedModelAssignmentMetadata trainedModelAssignmentMetadata = TrainedModelAssignmentMetadata.fromState(clusterService.state());

        if (trainedModelAssignmentMetadata.isAssigned(concreteModelId)) {
            // It is important to use the resolved model ID here as the alias could change between transport calls.
            inferAgainstAllocatedModel(trainedModelAssignmentMetadata, request, concreteModelId, responseBuilder, parentTaskId, listener);
        } else {
            getModelAndInfer(request, responseBuilder, parentTaskId, (CancellableTask) task, listener);
        }
    }

    private void getModelAndInfer(
        Request request,
        Response.Builder responseBuilder,
        TaskId parentTaskId,
        CancellableTask task,
        ActionListener<Response> listener
    ) {
        ActionListener<LocalModel> getModelListener = ActionListener.wrap(model -> {
            TypedChainTaskExecutor<InferenceResults> typedChainTaskExecutor = new TypedChainTaskExecutor<>(
                client.threadPool().executor(ThreadPool.Names.SAME),
                // run through all tasks
                r -> true,
                // Always fail immediately and return an error
                ex -> true
            );
            request.getObjectsToInfer().forEach(stringObjectMap -> typedChainTaskExecutor.add(chainedTask -> {
                if (task.isCancelled()) {
                    throw new TaskCancelledException(format("Inference task cancelled with reason [%s]", task.getReasonCancelled()));
                }
                model.infer(stringObjectMap, request.getUpdate(), chainedTask);
            }));

            typedChainTaskExecutor.execute(ActionListener.wrap(inferenceResultsInterfaces -> {
                model.release();
                listener.onResponse(responseBuilder.addInferenceResults(inferenceResultsInterfaces).build());
            }, e -> {
                model.release();
                listener.onFailure(e);
            }));
        }, e -> {
            if (ExceptionsHelper.unwrapCause(e) instanceof ResourceNotFoundException) {
                listener.onFailure(e);
                return;
            }

            // The model was found, check if a more relevant error message can be returned
            trainedModelProvider.getTrainedModel(
                request.getModelId(),
                GetTrainedModelsAction.Includes.empty(),
                parentTaskId,
                ActionListener.wrap(trainedModelConfig -> {
                    if (trainedModelConfig.getModelType() == TrainedModelType.PYTORCH) {
                        // The PyTorch model cannot be allocated if we got here
                        listener.onFailure(
                            ExceptionsHelper.conflictStatusException(
                                "Model ["
                                    + request.getModelId()
                                    + "] must be deployed to use. Please deploy with the start trained model deployment API.",
                                request.getModelId()
                            )
                        );
                    } else {
                        // return the original error
                        listener.onFailure(e);
                    }
                }, listener::onFailure)
            );
        });

        // TODO should `getModelForInternalInference` be used here??
        modelLoadingService.getModelForPipeline(request.getModelId(), parentTaskId, getModelListener);
    }

    private void inferAgainstAllocatedModel(
        TrainedModelAssignmentMetadata assignmentMeta,
        Request request,
        String concreteModelId,
        Response.Builder responseBuilder,
        TaskId parentTaskId,
        ActionListener<Response> listener
    ) {
        TrainedModelAssignment assignment = assignmentMeta.getModelAssignment(concreteModelId);

        if (assignment.getAssignmentState() == AssignmentState.STOPPING) {
            String message = "Trained model [" + request.getModelId() + "] is STOPPING";
            listener.onFailure(ExceptionsHelper.conflictStatusException(message));
            return;
        }

        // Get a list of nodes to send the requests to and the number of
        // documents for each node.
        var nodes = assignment.selectRandomStartedNodesWeighedOnAllocationsForNRequests(request.numberOfDocuments());
        if (nodes.isEmpty()) {
            logger.trace(() -> format("[%s] model not allocated to any node [%s]", assignment.getModelId()));
            listener.onFailure(
                ExceptionsHelper.conflictStatusException("Trained model [" + request.getModelId() + "] is not allocated to any nodes")
            );
            return;
        }

        assert nodes.stream().mapToInt(Tuple::v2).sum() == request.numberOfDocuments()
            : "mismatch; sum of node requests does not match number of documents in request";

        AtomicInteger count = new AtomicInteger();
        AtomicArray<List<InferenceResults>> results = new AtomicArray<>(nodes.size());
        AtomicReference<Exception> failure = new AtomicReference<>();

        int startPos = 0;
        int slot = 0;
        for (var node : nodes) {
            InferTrainedModelDeploymentAction.Request deploymentRequest;
            if (request.getTextInput() == null) {
                deploymentRequest = InferTrainedModelDeploymentAction.Request.forDocs(
                    concreteModelId,
                    request.getUpdate(),
                    request.getObjectsToInfer().subList(startPos, startPos + node.v2()),
                    request.getInferenceTimeout()
                );
            } else {
                deploymentRequest = InferTrainedModelDeploymentAction.Request.forTextInput(
                    concreteModelId,
                    request.getUpdate(),
                    request.getTextInput().subList(startPos, startPos + node.v2()),
                    request.getInferenceTimeout()
                );
            }
            deploymentRequest.setHighPriority(request.isHighPriority());
            deploymentRequest.setNodes(node.v1());
            deploymentRequest.setParentTask(parentTaskId);

            startPos += node.v2();

            executeAsyncWithOrigin(
                client,
                ML_ORIGIN,
                InferTrainedModelDeploymentAction.INSTANCE,
                deploymentRequest,
                collectingListener(count, results, failure, slot, nodes.size(), responseBuilder, listener)
            );

            slot++;
        }
    }

    private ActionListener<InferTrainedModelDeploymentAction.Response> collectingListener(
        AtomicInteger count,
        AtomicArray<List<InferenceResults>> results,
        AtomicReference<Exception> failure,
        int slot,
        int totalNumberOfResponses,
        Response.Builder responseBuilder,
        ActionListener<Response> finalListener
    ) {
        return new ActionListener<>() {
            @Override
            public void onResponse(InferTrainedModelDeploymentAction.Response response) {
                results.setOnce(slot, response.getResults());
                if (count.incrementAndGet() == totalNumberOfResponses) {
                    sendResponse();
                }
            }

            @Override
            public void onFailure(Exception e) {
                failure.set(e);
                if (count.incrementAndGet() == totalNumberOfResponses) {
                    sendResponse();
                }
            }

            private void sendResponse() {
                if (results.nonNullLength() > 0) {
                    for (int i = 0; i < results.length(); i++) {
                        if (results.get(i) != null) {
                            responseBuilder.addInferenceResults(results.get(i));
                        }
                    }
                    finalListener.onResponse(responseBuilder.build());
                } else {
                    finalListener.onFailure(failure.get());
                }
            }
        };
    }
}
