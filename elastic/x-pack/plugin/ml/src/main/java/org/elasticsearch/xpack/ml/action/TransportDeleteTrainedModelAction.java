/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.ml.action;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.action.support.master.AcknowledgedTransportMasterNodeAction;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.ingest.IngestMetadata;
import org.elasticsearch.ingest.IngestService;
import org.elasticsearch.ingest.Pipeline;
import org.elasticsearch.ingest.PipelineConfiguration;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.ml.action.DeleteTrainedModelAction;
import org.elasticsearch.xpack.ml.inference.ingest.InferenceProcessor;
import org.elasticsearch.xpack.ml.inference.persistence.TrainedModelProvider;
import org.elasticsearch.xpack.ml.notifications.InferenceAuditor;

import java.util.HashSet;
import java.util.Map;
import java.util.Objects;
import java.util.Set;

/**
 * The action is a master node action to ensure it reads an up-to-date cluster
 * state in order to determine if there is a processor referencing the trained model
 */
public class TransportDeleteTrainedModelAction
    extends AcknowledgedTransportMasterNodeAction<DeleteTrainedModelAction.Request> {

    private static final Logger logger = LogManager.getLogger(TransportDeleteTrainedModelAction.class);

    private final TrainedModelProvider trainedModelProvider;
    private final InferenceAuditor auditor;
    private final IngestService ingestService;

    @Inject
    public TransportDeleteTrainedModelAction(TransportService transportService, ClusterService clusterService,
                                             ThreadPool threadPool, ActionFilters actionFilters,
                                             IndexNameExpressionResolver indexNameExpressionResolver,
                                             TrainedModelProvider configProvider, InferenceAuditor auditor,
                                             IngestService ingestService) {
        super(DeleteTrainedModelAction.NAME, transportService, clusterService, threadPool, actionFilters,
            DeleteTrainedModelAction.Request::new, indexNameExpressionResolver, ThreadPool.Names.SAME);
        this.trainedModelProvider = configProvider;
        this.ingestService = ingestService;
        this.auditor = Objects.requireNonNull(auditor);
    }

    @Override
    protected void masterOperation(Task task,
                                   DeleteTrainedModelAction.Request request,
                                   ClusterState state,
                                   ActionListener<AcknowledgedResponse> listener) {
        String id = request.getId();
        IngestMetadata currentIngestMetadata = state.metadata().custom(IngestMetadata.TYPE);
        Set<String> referencedModels = getReferencedModelKeys(currentIngestMetadata);

        if (referencedModels.contains(id)) {
            listener.onFailure(new ElasticsearchStatusException("Cannot delete model [{}] as it is still referenced by ingest processors",
                RestStatus.CONFLICT,
                id));
            return;
        }

        trainedModelProvider.deleteTrainedModel(request.getId(), ActionListener.wrap(
            r -> {
                auditor.info(request.getId(), "trained model deleted");
                listener.onResponse(AcknowledgedResponse.TRUE);
            },
            listener::onFailure
        ));
    }

    private Set<String> getReferencedModelKeys(IngestMetadata ingestMetadata) {
        Set<String> allReferencedModelKeys = new HashSet<>();
        if (ingestMetadata == null) {
            return allReferencedModelKeys;
        }
        for(Map.Entry<String, PipelineConfiguration> entry : ingestMetadata.getPipelines().entrySet()) {
            String pipelineId = entry.getKey();
            Map<String, Object> config = entry.getValue().getConfigAsMap();
            try {
                Pipeline pipeline = Pipeline.create(pipelineId,
                    config,
                    ingestService.getProcessorFactories(),
                    ingestService.getScriptService());
                pipeline.getProcessors().stream()
                    .filter(p -> p instanceof InferenceProcessor)
                    .map(p -> (InferenceProcessor) p)
                    .map(InferenceProcessor::getModelId)
                    .forEach(allReferencedModelKeys::add);
            } catch (Exception ex) {
                logger.warn(new ParameterizedMessage("failed to load pipeline [{}]", pipelineId), ex);
            }
        }
        return allReferencedModelKeys;
    }


    @Override
    protected ClusterBlockException checkBlock(DeleteTrainedModelAction.Request request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }
}
