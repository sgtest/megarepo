/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.inference.action;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.SubscribableListener;
import org.elasticsearch.action.support.master.TransportMasterNodeAction;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.util.concurrent.EsExecutors;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.inference.InferenceService;
import org.elasticsearch.inference.InferenceServiceRegistry;
import org.elasticsearch.inference.Model;
import org.elasticsearch.inference.ModelConfigurations;
import org.elasticsearch.inference.TaskType;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xcontent.XContentParser;
import org.elasticsearch.xcontent.XContentParserConfiguration;
import org.elasticsearch.xpack.core.inference.action.PutInferenceModelAction;
import org.elasticsearch.xpack.core.ml.MachineLearningField;
import org.elasticsearch.xpack.core.ml.inference.assignment.TrainedModelAssignmentUtils;
import org.elasticsearch.xpack.core.ml.job.messages.Messages;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.core.ml.utils.MlPlatformArchitecturesUtil;
import org.elasticsearch.xpack.inference.InferencePlugin;
import org.elasticsearch.xpack.inference.registry.ModelRegistry;

import java.io.IOException;
import java.util.Map;
import java.util.Set;

import static org.elasticsearch.core.Strings.format;

public class TransportPutInferenceModelAction extends TransportMasterNodeAction<
    PutInferenceModelAction.Request,
    PutInferenceModelAction.Response> {

    private static final Logger logger = LogManager.getLogger(TransportPutInferenceModelAction.class);

    private final ModelRegistry modelRegistry;
    private final InferenceServiceRegistry serviceRegistry;
    private final Client client;

    @Inject
    public TransportPutInferenceModelAction(
        TransportService transportService,
        ClusterService clusterService,
        ThreadPool threadPool,
        ActionFilters actionFilters,
        IndexNameExpressionResolver indexNameExpressionResolver,
        ModelRegistry modelRegistry,
        InferenceServiceRegistry serviceRegistry,
        Client client
    ) {
        super(
            PutInferenceModelAction.NAME,
            transportService,
            clusterService,
            threadPool,
            actionFilters,
            PutInferenceModelAction.Request::new,
            indexNameExpressionResolver,
            PutInferenceModelAction.Response::new,
            EsExecutors.DIRECT_EXECUTOR_SERVICE
        );
        this.modelRegistry = modelRegistry;
        this.serviceRegistry = serviceRegistry;
        this.client = client;
    }

    @Override
    protected void masterOperation(
        Task task,
        PutInferenceModelAction.Request request,
        ClusterState state,
        ActionListener<PutInferenceModelAction.Response> listener
    ) throws Exception {

        var requestAsMap = requestToMap(request);
        String serviceName = (String) requestAsMap.remove(ModelConfigurations.SERVICE);
        if (serviceName == null) {
            listener.onFailure(new ElasticsearchStatusException("Model configuration is missing a service", RestStatus.BAD_REQUEST));
            return;
        }

        var service = serviceRegistry.getService(serviceName);
        if (service.isEmpty()) {
            listener.onFailure(new ElasticsearchStatusException("Unknown service [{}]", RestStatus.BAD_REQUEST, serviceName));
            return;
        }

        // Check if all the nodes in this cluster know about the service
        if (service.get().getMinimalSupportedVersion().after(state.getMinTransportVersion())) {
            logger.warn(
                format(
                    "Service [%s] requires version [%s] but minimum cluster version is [%s]",
                    serviceName,
                    service.get().getMinimalSupportedVersion(),
                    state.getMinTransportVersion()
                )
            );

            listener.onFailure(
                new ElasticsearchStatusException(
                    format(
                        "All nodes in the cluster are not aware of the service [%s]."
                            + "Wait for the cluster to finish upgrading and try again.",
                        serviceName
                    ),
                    RestStatus.BAD_REQUEST
                )
            );
            return;
        }

        var assignments = TrainedModelAssignmentUtils.modelAssignments(request.getModelId(), clusterService.state());
        if ((assignments == null || assignments.isEmpty()) == false) {
            listener.onFailure(
                ExceptionsHelper.badRequestException(Messages.MODEL_ID_MATCHES_EXISTING_MODEL_IDS_BUT_MUST_NOT, request.getModelId())
            );
            return;
        }

        if (service.get().isInClusterService()) {
            // Find the cluster platform as the service may need that
            // information when creating the model
            MlPlatformArchitecturesUtil.getMlNodesArchitecturesSet(listener.delegateFailureAndWrap((delegate, architectures) -> {
                if (architectures.isEmpty() && clusterIsInElasticCloud(clusterService.getClusterSettings())) {
                    parseAndStoreModel(
                        service.get(),
                        request.getModelId(),
                        request.getTaskType(),
                        requestAsMap,
                        // In Elastic cloud ml nodes run on Linux x86
                        Set.of("linux-x86_64"),
                        delegate
                    );
                } else {
                    // The architecture field could be an empty set, the individual services will need to handle that
                    parseAndStoreModel(service.get(), request.getModelId(), request.getTaskType(), requestAsMap, architectures, delegate);
                }
            }), client, threadPool.executor(InferencePlugin.UTILITY_THREAD_POOL_NAME));
        } else {
            // Not an in cluster service, it does not care about the cluster platform
            parseAndStoreModel(service.get(), request.getModelId(), request.getTaskType(), requestAsMap, Set.of(), listener);
        }
    }

    private void parseAndStoreModel(
        InferenceService service,
        String modelId,
        TaskType taskType,
        Map<String, Object> config,
        Set<String> platformArchitectures,
        ActionListener<PutInferenceModelAction.Response> listener
    ) {
        var model = service.parseRequestConfig(modelId, taskType, config, platformArchitectures);

        service.checkModelConfig(
            model,
            listener.delegateFailureAndWrap(
                // model is valid good to persist then start
                (delegate, verifiedModel) -> modelRegistry.storeModel(
                    verifiedModel,
                    delegate.delegateFailureAndWrap((l, r) -> startModel(service, verifiedModel, l))
                )
            )
        );
    }

    private static void startModel(InferenceService service, Model model, ActionListener<PutInferenceModelAction.Response> finalListener) {
        SubscribableListener.<Boolean>newForked((listener1) -> { service.putModel(model, listener1); }).<
            PutInferenceModelAction.Response>andThen((listener2, modelDidPut) -> {
                if (modelDidPut) {
                    service.start(
                        model,
                        listener2.delegateFailureAndWrap(
                            (l3, ok) -> l3.onResponse(new PutInferenceModelAction.Response(model.getConfigurations()))
                        )
                    );
                } else {
                    logger.warn("Failed to put model [{}]", model.getModelId());
                }
            }).addListener(finalListener);
    }

    private Map<String, Object> requestToMap(PutInferenceModelAction.Request request) throws IOException {
        try (
            XContentParser parser = XContentHelper.createParser(
                XContentParserConfiguration.EMPTY,
                request.getContent(),
                request.getContentType()
            )
        ) {
            return parser.map();
        }
    }

    @Override
    protected ClusterBlockException checkBlock(PutInferenceModelAction.Request request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }

    static boolean clusterIsInElasticCloud(ClusterSettings settings) {
        // use a heuristic to determine if in Elastic cloud.
        // One such heuristic is where USE_AUTO_MACHINE_MEMORY_PERCENT == true
        return settings.get(MachineLearningField.USE_AUTO_MACHINE_MEMORY_PERCENT);
    }
}
