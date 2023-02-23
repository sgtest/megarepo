/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.ingest;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.util.Strings;
import org.elasticsearch.ElasticsearchParseException;
import org.elasticsearch.ExceptionsHelper;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.DocWriteRequest;
import org.elasticsearch.action.admin.cluster.node.info.NodeInfo;
import org.elasticsearch.action.admin.cluster.node.info.NodesInfoResponse;
import org.elasticsearch.action.bulk.TransportBulkAction;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.ingest.DeletePipelineRequest;
import org.elasticsearch.action.ingest.PutPipelineRequest;
import org.elasticsearch.action.support.RefCountingRunnable;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.cluster.ClusterChangedEvent;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateApplier;
import org.elasticsearch.cluster.ClusterStateTaskExecutor;
import org.elasticsearch.cluster.ClusterStateTaskListener;
import org.elasticsearch.cluster.metadata.DataStream.TimestampField;
import org.elasticsearch.cluster.metadata.IndexAbstraction;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.IndexTemplateMetadata;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.metadata.MetadataIndexTemplateService;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.cluster.service.MasterServiceTaskQueue;
import org.elasticsearch.common.Priority;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.regex.Regex;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.CollectionUtils;
import org.elasticsearch.common.util.concurrent.AbstractRunnable;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.core.Releasable;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.core.Tuple;
import org.elasticsearch.env.Environment;
import org.elasticsearch.gateway.GatewayService;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.VersionType;
import org.elasticsearch.index.analysis.AnalysisRegistry;
import org.elasticsearch.node.ReportingService;
import org.elasticsearch.plugins.IngestPlugin;
import org.elasticsearch.script.ScriptService;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xcontent.XContentBuilder;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.LinkedList;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.function.BiConsumer;
import java.util.function.Consumer;
import java.util.function.IntConsumer;
import java.util.function.Predicate;
import java.util.stream.Collectors;

import static org.elasticsearch.core.Strings.format;

/**
 * Holder class for several ingest related services.
 */
public class IngestService implements ClusterStateApplier, ReportingService<IngestInfo> {

    public static final String NOOP_PIPELINE_NAME = "_none";

    public static final String INGEST_ORIGIN = "ingest";

    private static final Logger logger = LogManager.getLogger(IngestService.class);

    private final MasterServiceTaskQueue<PipelineClusterStateUpdateTask> taskQueue;
    private final ClusterService clusterService;
    private final ScriptService scriptService;
    private final Map<String, Processor.Factory> processorFactories;
    // Ideally this should be in IngestMetadata class, but we don't have the processor factories around there.
    // We know of all the processor factories when a node with all its plugin have been initialized. Also some
    // processor factories rely on other node services. Custom metadata is statically registered when classes
    // are loaded, so in the cluster state we just save the pipeline config and here we keep the actual pipelines around.
    private volatile Map<String, PipelineHolder> pipelines = Map.of();
    private final ThreadPool threadPool;
    private final IngestMetric totalMetrics = new IngestMetric();
    private final List<Consumer<ClusterState>> ingestClusterStateListeners = new CopyOnWriteArrayList<>();
    private volatile ClusterState state;

    /**
     * Cluster state task executor for ingest pipeline operations
     */
    static final ClusterStateTaskExecutor<PipelineClusterStateUpdateTask> PIPELINE_TASK_EXECUTOR = batchExecutionContext -> {
        final var allIndexMetadata = batchExecutionContext.initialState().metadata().indices().values();
        final IngestMetadata initialIngestMetadata = batchExecutionContext.initialState().metadata().custom(IngestMetadata.TYPE);
        var currentIngestMetadata = initialIngestMetadata;
        for (final var taskContext : batchExecutionContext.taskContexts()) {
            try {
                final var task = taskContext.getTask();
                try (var ignored = taskContext.captureResponseHeaders()) {
                    currentIngestMetadata = task.execute(currentIngestMetadata, allIndexMetadata);
                }
                taskContext.success(() -> task.listener.onResponse(AcknowledgedResponse.TRUE));
            } catch (Exception e) {
                taskContext.onFailure(e);
            }
        }
        final var finalIngestMetadata = currentIngestMetadata;
        return finalIngestMetadata == initialIngestMetadata
            ? batchExecutionContext.initialState()
            : batchExecutionContext.initialState().copyAndUpdateMetadata(b -> b.putCustom(IngestMetadata.TYPE, finalIngestMetadata));
    };

    /**
     * Specialized cluster state update task specifically for ingest pipeline operations.
     * These operations all receive an AcknowledgedResponse.
     */
    public abstract static class PipelineClusterStateUpdateTask implements ClusterStateTaskListener {
        final ActionListener<AcknowledgedResponse> listener;

        PipelineClusterStateUpdateTask(ActionListener<AcknowledgedResponse> listener) {
            this.listener = listener;
        }

        public abstract IngestMetadata execute(IngestMetadata currentIngestMetadata, Collection<IndexMetadata> allIndexMetadata);

        @Override
        public void onFailure(Exception e) {
            listener.onFailure(e);
        }
    }

    public IngestService(
        ClusterService clusterService,
        ThreadPool threadPool,
        Environment env,
        ScriptService scriptService,
        AnalysisRegistry analysisRegistry,
        List<IngestPlugin> ingestPlugins,
        Client client
    ) {
        this.clusterService = clusterService;
        this.scriptService = scriptService;
        this.processorFactories = processorFactories(
            ingestPlugins,
            new Processor.Parameters(
                env,
                scriptService,
                analysisRegistry,
                threadPool.getThreadContext(),
                threadPool::relativeTimeInMillis,
                (delay, command) -> threadPool.schedule(command, TimeValue.timeValueMillis(delay), ThreadPool.Names.GENERIC),
                this,
                client,
                threadPool.generic()::execute
            )
        );

        this.threadPool = threadPool;
        this.taskQueue = clusterService.createTaskQueue("ingest-pipelines", Priority.NORMAL, PIPELINE_TASK_EXECUTOR);
    }

    private static Map<String, Processor.Factory> processorFactories(List<IngestPlugin> ingestPlugins, Processor.Parameters parameters) {
        Map<String, Processor.Factory> processorFactories = new HashMap<>();
        for (IngestPlugin ingestPlugin : ingestPlugins) {
            Map<String, Processor.Factory> newProcessors = ingestPlugin.getProcessors(parameters);
            for (Map.Entry<String, Processor.Factory> entry : newProcessors.entrySet()) {
                if (processorFactories.put(entry.getKey(), entry.getValue()) != null) {
                    throw new IllegalArgumentException("Ingest processor [" + entry.getKey() + "] is already registered");
                }
            }
        }
        return Collections.unmodifiableMap(processorFactories);
    }

    public static boolean resolvePipelines(
        final DocWriteRequest<?> originalRequest,
        final IndexRequest indexRequest,
        final Metadata metadata
    ) {
        return resolvePipelines(originalRequest, indexRequest, metadata, System.currentTimeMillis());
    }

    public static boolean resolvePipelines(
        final DocWriteRequest<?> originalRequest,
        final IndexRequest indexRequest,
        final Metadata metadata,
        final long epochMillis
    ) {
        if (indexRequest.isPipelineResolved() == false) {
            final String requestPipeline = indexRequest.getPipeline();
            indexRequest.setPipeline(NOOP_PIPELINE_NAME);
            indexRequest.setFinalPipeline(NOOP_PIPELINE_NAME);
            String defaultPipeline = null;
            String finalPipeline = null;
            IndexMetadata indexMetadata = null;
            // start to look for default or final pipelines via settings found in the index metadata
            if (originalRequest != null) {
                indexMetadata = metadata.indices()
                    .get(IndexNameExpressionResolver.resolveDateMathExpression(originalRequest.index(), epochMillis));
            }
            // check the alias for the index request (this is how normal index requests are modeled)
            if (indexMetadata == null && indexRequest.index() != null) {
                IndexAbstraction indexAbstraction = metadata.getIndicesLookup().get(indexRequest.index());
                if (indexAbstraction != null && indexAbstraction.getWriteIndex() != null) {
                    indexMetadata = metadata.index(indexAbstraction.getWriteIndex());
                }
            }
            // check the alias for the action request (this is how upserts are modeled)
            if (indexMetadata == null && originalRequest != null && originalRequest.index() != null) {
                IndexAbstraction indexAbstraction = metadata.getIndicesLookup().get(originalRequest.index());
                if (indexAbstraction != null && indexAbstraction.getWriteIndex() != null) {
                    indexMetadata = metadata.index(indexAbstraction.getWriteIndex());
                }
            }
            if (indexMetadata != null) {
                final Settings indexSettings = indexMetadata.getSettings();
                if (IndexSettings.DEFAULT_PIPELINE.exists(indexSettings)) {
                    // find the default pipeline if one is defined from an existing index setting
                    defaultPipeline = IndexSettings.DEFAULT_PIPELINE.get(indexSettings);
                    indexRequest.setPipeline(defaultPipeline);
                }
                if (IndexSettings.FINAL_PIPELINE.exists(indexSettings)) {
                    // find the final pipeline if one is defined from an existing index setting
                    finalPipeline = IndexSettings.FINAL_PIPELINE.get(indexSettings);
                    indexRequest.setFinalPipeline(finalPipeline);
                }
            } else if (indexRequest.index() != null) {
                // the index does not exist yet (and this is a valid request), so match index
                // templates to look for pipelines in either a matching V2 template (which takes
                // precedence), or if a V2 template does not match, any V1 templates
                String v2Template = MetadataIndexTemplateService.findV2Template(metadata, indexRequest.index(), false);
                if (v2Template != null) {
                    Settings settings = MetadataIndexTemplateService.resolveSettings(metadata, v2Template);
                    if (IndexSettings.DEFAULT_PIPELINE.exists(settings)) {
                        defaultPipeline = IndexSettings.DEFAULT_PIPELINE.get(settings);
                    }
                    if (IndexSettings.FINAL_PIPELINE.exists(settings)) {
                        finalPipeline = IndexSettings.FINAL_PIPELINE.get(settings);
                    }
                    indexRequest.setPipeline(Objects.requireNonNullElse(defaultPipeline, NOOP_PIPELINE_NAME));
                    indexRequest.setFinalPipeline(Objects.requireNonNullElse(finalPipeline, NOOP_PIPELINE_NAME));
                } else {
                    List<IndexTemplateMetadata> templates = MetadataIndexTemplateService.findV1Templates(
                        metadata,
                        indexRequest.index(),
                        null
                    );
                    // order of templates are highest order first
                    for (final IndexTemplateMetadata template : templates) {
                        final Settings settings = template.settings();
                        if (defaultPipeline == null && IndexSettings.DEFAULT_PIPELINE.exists(settings)) {
                            defaultPipeline = IndexSettings.DEFAULT_PIPELINE.get(settings);
                            // we can not break in case a lower-order template has a final pipeline that we need to collect
                        }
                        if (finalPipeline == null && IndexSettings.FINAL_PIPELINE.exists(settings)) {
                            finalPipeline = IndexSettings.FINAL_PIPELINE.get(settings);
                            // we can not break in case a lower-order template has a default pipeline that we need to collect
                        }
                        if (defaultPipeline != null && finalPipeline != null) {
                            // we can break if we have already collected a default and final pipeline
                            break;
                        }
                    }
                    indexRequest.setPipeline(Objects.requireNonNullElse(defaultPipeline, NOOP_PIPELINE_NAME));
                    indexRequest.setFinalPipeline(Objects.requireNonNullElse(finalPipeline, NOOP_PIPELINE_NAME));
                }
            }

            if (requestPipeline != null) {
                indexRequest.setPipeline(requestPipeline);
            }

            /*
             * We have to track whether or not the pipeline for this request has already been resolved. It can happen that the
             * pipeline for this request has already been derived yet we execute this loop again. That occurs if the bulk request
             * has been forwarded by a non-ingest coordinating node to an ingest node. In this case, the coordinating node will have
             * already resolved the pipeline for this request. It is important that we are able to distinguish this situation as we
             * can not double-resolve the pipeline because we will not be able to distinguish the case of the pipeline having been
             * set from a request pipeline parameter versus having been set by the resolution. We need to be able to distinguish
             * these cases as we need to reject the request if the pipeline was set by a required pipeline and there is a request
             * pipeline parameter too.
             */
            indexRequest.isPipelineResolved(true);
        }

        // return whether this index request has a pipeline
        return NOOP_PIPELINE_NAME.equals(indexRequest.getPipeline()) == false
            || NOOP_PIPELINE_NAME.equals(indexRequest.getFinalPipeline()) == false;
    }

    public ClusterService getClusterService() {
        return clusterService;
    }

    public ScriptService getScriptService() {
        return scriptService;
    }

    /**
     * Deletes the pipeline specified by id in the request.
     */
    public void delete(DeletePipelineRequest request, ActionListener<AcknowledgedResponse> listener) {
        taskQueue.submitTask(
            "delete-pipeline-" + request.getId(),
            new DeletePipelineClusterStateUpdateTask(listener, request),
            request.masterNodeTimeout()
        );
    }

    /**
     * Used by this class and {@link org.elasticsearch.action.ingest.ReservedPipelineAction}
     */
    public static class DeletePipelineClusterStateUpdateTask extends PipelineClusterStateUpdateTask {
        private final DeletePipelineRequest request;

        DeletePipelineClusterStateUpdateTask(ActionListener<AcknowledgedResponse> listener, DeletePipelineRequest request) {
            super(listener);
            this.request = request;
        }

        /**
         * Used by the {@link org.elasticsearch.action.ingest.ReservedPipelineAction}
         */
        public DeletePipelineClusterStateUpdateTask(String id) {
            this(null, new DeletePipelineRequest(id));
        }

        @Override
        public IngestMetadata execute(IngestMetadata currentIngestMetadata, Collection<IndexMetadata> allIndexMetadata) {
            if (currentIngestMetadata == null) {
                return null;
            }
            Map<String, PipelineConfiguration> pipelines = currentIngestMetadata.getPipelines();
            Set<String> toRemove = new HashSet<>();
            for (String pipelineKey : pipelines.keySet()) {
                if (Regex.simpleMatch(request.getId(), pipelineKey)) {
                    toRemove.add(pipelineKey);
                }
            }
            if (toRemove.isEmpty() && Regex.isMatchAllPattern(request.getId()) == false) {
                throw new ResourceNotFoundException("pipeline [{}] is missing", request.getId());
            } else if (toRemove.isEmpty()) {
                return currentIngestMetadata;
            }
            final Map<String, PipelineConfiguration> pipelinesCopy = new HashMap<>(pipelines);
            for (String key : toRemove) {
                validateNotInUse(key, allIndexMetadata);
                pipelinesCopy.remove(key);
            }
            return new IngestMetadata(pipelinesCopy);
        }
    }

    static void validateNotInUse(String pipeline, Collection<IndexMetadata> allIndexMetadata) {
        List<String> defaultPipelineIndices = new ArrayList<>();
        List<String> finalPipelineIndices = new ArrayList<>();
        for (IndexMetadata indexMetadata : allIndexMetadata) {
            String defaultPipeline = IndexSettings.DEFAULT_PIPELINE.get(indexMetadata.getSettings());
            String finalPipeline = IndexSettings.FINAL_PIPELINE.get(indexMetadata.getSettings());
            if (pipeline.equals(defaultPipeline)) {
                defaultPipelineIndices.add(indexMetadata.getIndex().getName());
            }

            if (pipeline.equals(finalPipeline)) {
                finalPipelineIndices.add(indexMetadata.getIndex().getName());
            }
        }

        if (defaultPipelineIndices.size() > 0 || finalPipelineIndices.size() > 0) {
            throw new IllegalArgumentException(
                String.format(
                    Locale.ROOT,
                    "pipeline [%s] cannot be deleted because it is %s%s%s",
                    pipeline,
                    defaultPipelineIndices.size() > 0
                        ? String.format(
                            Locale.ROOT,
                            "the default pipeline for %s index(es) including [%s]",
                            defaultPipelineIndices.size(),
                            defaultPipelineIndices.stream().sorted().limit(3).collect(Collectors.joining(","))
                        )
                        : Strings.EMPTY,
                    defaultPipelineIndices.size() > 0 && finalPipelineIndices.size() > 0 ? " and " : Strings.EMPTY,
                    finalPipelineIndices.size() > 0
                        ? String.format(
                            Locale.ROOT,
                            "the final pipeline for %s index(es) including [%s]",
                            finalPipelineIndices.size(),
                            finalPipelineIndices.stream().sorted().limit(3).collect(Collectors.joining(","))
                        )
                        : Strings.EMPTY
                )
            );
        }
    }

    /**
     * @return pipeline configuration specified by id. If multiple ids or wildcards are specified multiple pipelines
     * may be returned
     */
    // Returning PipelineConfiguration instead of Pipeline, because Pipeline and Processor interface don't
    // know how to serialize themselves.
    public static List<PipelineConfiguration> getPipelines(ClusterState clusterState, String... ids) {
        IngestMetadata ingestMetadata = clusterState.getMetadata().custom(IngestMetadata.TYPE);
        return innerGetPipelines(ingestMetadata, ids);
    }

    static List<PipelineConfiguration> innerGetPipelines(IngestMetadata ingestMetadata, String... ids) {
        if (ingestMetadata == null) {
            return Collections.emptyList();
        }

        // if we didn't ask for _any_ ID, then we get them all (this is the same as if they ask for '*')
        if (ids.length == 0) {
            return new ArrayList<>(ingestMetadata.getPipelines().values());
        }

        List<PipelineConfiguration> result = new ArrayList<>(ids.length);
        for (String id : ids) {
            if (Regex.isSimpleMatchPattern(id)) {
                for (Map.Entry<String, PipelineConfiguration> entry : ingestMetadata.getPipelines().entrySet()) {
                    if (Regex.simpleMatch(id, entry.getKey())) {
                        result.add(entry.getValue());
                    }
                }
            } else {
                PipelineConfiguration pipeline = ingestMetadata.getPipelines().get(id);
                if (pipeline != null) {
                    result.add(pipeline);
                }
            }
        }
        return result;
    }

    /**
     * Stores the specified pipeline definition in the request.
     */
    public void putPipeline(
        PutPipelineRequest request,
        ActionListener<AcknowledgedResponse> listener,
        Consumer<ActionListener<NodesInfoResponse>> nodeInfoListener
    ) throws Exception {
        if (isNoOpPipelineUpdate(state, request)) {
            // existing pipeline matches request pipeline -- no need to update
            listener.onResponse(AcknowledgedResponse.TRUE);
            return;
        }

        nodeInfoListener.accept(ActionListener.wrap(nodeInfos -> {
            validatePipelineRequest(request, nodeInfos);

            taskQueue.submitTask(
                "put-pipeline-" + request.getId(),
                new PutPipelineClusterStateUpdateTask(listener, request),
                request.masterNodeTimeout()
            );
        }, listener::onFailure));
    }

    public void validatePipelineRequest(PutPipelineRequest request, NodesInfoResponse nodeInfos) throws Exception {
        final Map<String, Object> config = XContentHelper.convertToMap(request.getSource(), false, request.getXContentType()).v2();
        Map<DiscoveryNode, IngestInfo> ingestInfos = new HashMap<>();
        for (NodeInfo nodeInfo : nodeInfos.getNodes()) {
            ingestInfos.put(nodeInfo.getNode(), nodeInfo.getInfo(IngestInfo.class));
        }

        validatePipeline(ingestInfos, request.getId(), config);
    }

    public static boolean isNoOpPipelineUpdate(ClusterState state, PutPipelineRequest request) {
        IngestMetadata currentIngestMetadata = state.metadata().custom(IngestMetadata.TYPE);
        if (request.getVersion() == null
            && currentIngestMetadata != null
            && currentIngestMetadata.getPipelines().containsKey(request.getId())) {
            var pipelineConfig = XContentHelper.convertToMap(request.getSource(), false, request.getXContentType()).v2();
            var currentPipeline = currentIngestMetadata.getPipelines().get(request.getId());
            if (currentPipeline.getConfigAsMap().equals(pipelineConfig)) {
                return true;
            }
        }

        return false;
    }

    /**
     * Returns the pipeline by the specified id
     */
    public Pipeline getPipeline(String id) {
        PipelineHolder holder = pipelines.get(id);
        if (holder != null) {
            return holder.pipeline;
        } else {
            return null;
        }
    }

    public Map<String, Processor.Factory> getProcessorFactories() {
        return processorFactories;
    }

    @Override
    public IngestInfo info() {
        Map<String, Processor.Factory> processorFactories = getProcessorFactories();
        List<ProcessorInfo> processorInfoList = new ArrayList<>(processorFactories.size());
        for (Map.Entry<String, Processor.Factory> entry : processorFactories.entrySet()) {
            processorInfoList.add(new ProcessorInfo(entry.getKey()));
        }
        return new IngestInfo(processorInfoList);
    }

    Map<String, PipelineHolder> pipelines() {
        return pipelines;
    }

    /**
     * Recursive method to obtain all the non-failure processors for given compoundProcessor.
     * <p>
     * 'if' and 'ignore_failure'/'on_failure' are implemented as wrappers around the actual processor (via {@link ConditionalProcessor}
     * and {@link OnFailureProcessor}, respectively), so we unwrap these processors internally in order to expose the underlying
     * 'actual' processor via the metrics. This corresponds best to the customer intent -- e.g. they used a 'set' processor that has an
     * 'on_failure', so we report metrics for the set processor, not an on_failure processor.
     *
     * @param compoundProcessor The compound processor to start walking the non-failure processors
     * @param processorMetrics The list of {@link Processor} {@link IngestMetric} tuples.
     * @return the processorMetrics for all non-failure processor that belong to the original compoundProcessor
     */
    private static List<Tuple<Processor, IngestMetric>> getProcessorMetrics(
        CompoundProcessor compoundProcessor,
        List<Tuple<Processor, IngestMetric>> processorMetrics
    ) {
        // only surface the top level non-failure processors, on-failure processor times will be included in the top level non-failure
        for (Tuple<Processor, IngestMetric> processorWithMetric : compoundProcessor.getProcessorsWithMetrics()) {
            Processor processor = processorWithMetric.v1();
            IngestMetric metric = processorWithMetric.v2();

            // unwrap 'if' and 'ignore_failure/on_failure' wrapping, so that we expose the underlying actual processor
            boolean unwrapped;
            do {
                unwrapped = false;
                if (processor instanceof ConditionalProcessor conditional) {
                    processor = conditional.getInnerProcessor();
                    metric = conditional.getMetric(); // prefer the conditional's metric, it only covers when the conditional was true
                    unwrapped = true;
                }
                if (processor instanceof OnFailureProcessor onFailure) {
                    processor = onFailure.getInnerProcessor();
                    metric = onFailure.getInnerMetric(); // the wrapped processor records the failure count
                    unwrapped = true;
                }
            } while (unwrapped);

            if (processor instanceof CompoundProcessor cp) {
                getProcessorMetrics(cp, processorMetrics);
            } else {
                processorMetrics.add(new Tuple<>(processor, metric));
            }
        }
        return processorMetrics;
    }

    /**
     * Used in this class and externally by the {@link org.elasticsearch.action.ingest.ReservedPipelineAction}
     */
    public static class PutPipelineClusterStateUpdateTask extends PipelineClusterStateUpdateTask {
        private final PutPipelineRequest request;

        PutPipelineClusterStateUpdateTask(ActionListener<AcknowledgedResponse> listener, PutPipelineRequest request) {
            super(listener);
            this.request = request;
        }

        /**
         * Used by {@link org.elasticsearch.action.ingest.ReservedPipelineAction}
         */
        public PutPipelineClusterStateUpdateTask(PutPipelineRequest request) {
            this(null, request);
        }

        @Override
        public IngestMetadata execute(IngestMetadata currentIngestMetadata, Collection<IndexMetadata> allIndexMetadata) {
            BytesReference pipelineSource = request.getSource();
            if (request.getVersion() != null) {
                var currentPipeline = currentIngestMetadata != null ? currentIngestMetadata.getPipelines().get(request.getId()) : null;
                if (currentPipeline == null) {
                    throw new IllegalArgumentException(
                        String.format(
                            Locale.ROOT,
                            "version conflict, required version [%s] for pipeline [%s] but no pipeline was found",
                            request.getVersion(),
                            request.getId()
                        )
                    );
                }

                final Integer currentVersion = currentPipeline.getVersion();
                if (Objects.equals(request.getVersion(), currentVersion) == false) {
                    throw new IllegalArgumentException(
                        String.format(
                            Locale.ROOT,
                            "version conflict, required version [%s] for pipeline [%s] but current version is [%s]",
                            request.getVersion(),
                            request.getId(),
                            currentVersion
                        )
                    );
                }

                var pipelineConfig = XContentHelper.convertToMap(request.getSource(), false, request.getXContentType()).v2();
                final Integer specifiedVersion = (Integer) pipelineConfig.get("version");
                if (pipelineConfig.containsKey("version") && Objects.equals(specifiedVersion, currentVersion)) {
                    throw new IllegalArgumentException(
                        String.format(
                            Locale.ROOT,
                            "cannot update pipeline [%s] with the same version [%s]",
                            request.getId(),
                            request.getVersion()
                        )
                    );
                }

                // if no version specified in the pipeline definition, inject a version of [request.getVersion() + 1]
                if (specifiedVersion == null) {
                    pipelineConfig.put("version", request.getVersion() == null ? 1 : request.getVersion() + 1);
                    try {
                        var builder = XContentBuilder.builder(request.getXContentType().xContent()).map(pipelineConfig);
                        pipelineSource = BytesReference.bytes(builder);
                    } catch (IOException e) {
                        throw new IllegalStateException(e);
                    }
                }
            }

            Map<String, PipelineConfiguration> pipelines;
            if (currentIngestMetadata != null) {
                pipelines = new HashMap<>(currentIngestMetadata.getPipelines());
            } else {
                pipelines = new HashMap<>();
            }

            pipelines.put(request.getId(), new PipelineConfiguration(request.getId(), pipelineSource, request.getXContentType()));
            return new IngestMetadata(pipelines);
        }
    }

    void validatePipeline(Map<DiscoveryNode, IngestInfo> ingestInfos, String pipelineId, Map<String, Object> pipelineConfig)
        throws Exception {
        if (ingestInfos.isEmpty()) {
            throw new IllegalStateException("Ingest info is empty");
        }

        Pipeline pipeline = Pipeline.create(pipelineId, pipelineConfig, processorFactories, scriptService);
        List<Exception> exceptions = new ArrayList<>();
        for (Processor processor : pipeline.flattenAllProcessors()) {
            for (Map.Entry<DiscoveryNode, IngestInfo> entry : ingestInfos.entrySet()) {
                String type = processor.getType();
                if (entry.getValue().containsProcessor(type) == false && ConditionalProcessor.TYPE.equals(type) == false) {
                    String message = "Processor type [" + processor.getType() + "] is not installed on node [" + entry.getKey() + "]";
                    exceptions.add(ConfigurationUtils.newConfigurationException(processor.getType(), processor.getTag(), null, message));
                }
            }
        }
        ExceptionsHelper.rethrowAndSuppress(exceptions);
    }

    public void executeBulkRequest(
        final int numberOfActionRequests,
        final Iterable<DocWriteRequest<?>> actionRequests,
        final IntConsumer onDropped,
        final BiConsumer<Integer, Exception> onFailure,
        final BiConsumer<Thread, Exception> onCompletion,
        final String executorName
    ) {
        assert numberOfActionRequests > 0 : "numberOfActionRequests must be greater than 0 but was [" + numberOfActionRequests + "]";

        threadPool.executor(executorName).execute(new AbstractRunnable() {

            @Override
            public void onFailure(Exception e) {
                onCompletion.accept(null, e);
            }

            @Override
            protected void doRun() {
                final Thread originalThread = Thread.currentThread();
                try (var refs = new RefCountingRunnable(() -> onCompletion.accept(originalThread, null))) {
                    int i = 0;
                    for (DocWriteRequest<?> actionRequest : actionRequests) {
                        IndexRequest indexRequest = TransportBulkAction.getIndexWriteRequest(actionRequest);
                        if (indexRequest == null) {
                            i++;
                            continue;
                        }

                        final String pipelineId = indexRequest.getPipeline();
                        indexRequest.setPipeline(NOOP_PIPELINE_NAME);
                        final String finalPipelineId = indexRequest.getFinalPipeline();
                        indexRequest.setFinalPipeline(NOOP_PIPELINE_NAME);
                        boolean hasFinalPipeline = true;
                        final List<String> pipelines;
                        if (IngestService.NOOP_PIPELINE_NAME.equals(pipelineId) == false
                            && IngestService.NOOP_PIPELINE_NAME.equals(finalPipelineId) == false) {
                            pipelines = List.of(pipelineId, finalPipelineId);
                        } else if (IngestService.NOOP_PIPELINE_NAME.equals(pipelineId) == false) {
                            pipelines = List.of(pipelineId);
                            hasFinalPipeline = false;
                        } else if (IngestService.NOOP_PIPELINE_NAME.equals(finalPipelineId) == false) {
                            pipelines = List.of(finalPipelineId);
                        } else {
                            i++;
                            continue;
                        }

                        // start the stopwatch and acquire a ref to indicate that we're working on this document
                        final long startTimeInNanos = System.nanoTime();
                        totalMetrics.preIngest();
                        final int slot = i;
                        final Releasable ref = refs.acquire();
                        // the document listener gives us three-way logic: a document can fail processing (1), or it can
                        // be successfully processed. a successfully processed document can be kept (2) or dropped (3).
                        final ActionListener<Boolean> documentListener = ActionListener.runAfter(new ActionListener<>() {
                            @Override
                            public void onResponse(Boolean kept) {
                                assert kept != null;
                                if (kept == false) {
                                    onDropped.accept(slot);
                                }
                            }

                            @Override
                            public void onFailure(Exception e) {
                                totalMetrics.ingestFailed();
                                onFailure.accept(slot, e);
                            }
                        }, () -> {
                            // regardless of success or failure, we always stop the ingest "stopwatch" and release the ref to indicate
                            // that we're finished with this document
                            final long ingestTimeInNanos = System.nanoTime() - startTimeInNanos;
                            totalMetrics.postIngest(ingestTimeInNanos);
                            ref.close();
                        });

                        IngestDocument ingestDocument = newIngestDocument(indexRequest);
                        executePipelines(pipelines.iterator(), hasFinalPipeline, indexRequest, ingestDocument, documentListener);

                        i++;
                    }
                }
            }
        });
    }

    private void executePipelines(
        final Iterator<String> pipelineIds,
        final boolean hasFinalPipeline,
        final IndexRequest indexRequest,
        final IngestDocument ingestDocument,
        final ActionListener<Boolean> listener
    ) {
        assert pipelineIds.hasNext();
        final String pipelineId = pipelineIds.next();
        try {
            final PipelineHolder holder = pipelines.get(pipelineId);
            if (holder == null) {
                throw new IllegalArgumentException("pipeline with id [" + pipelineId + "] does not exist");
            }
            final Pipeline pipeline = holder.pipeline;
            final String originalIndex = indexRequest.indices()[0];
            executePipeline(ingestDocument, pipeline, (keep, e) -> {
                assert keep != null;

                if (e != null) {
                    logger.debug(
                        () -> format(
                            "failed to execute pipeline [%s] for document [%s/%s]",
                            pipelineId,
                            indexRequest.index(),
                            indexRequest.id()
                        ),
                        e
                    );
                    listener.onFailure(e);
                    return; // document failed!
                }

                if (keep == false) {
                    listener.onResponse(false);
                    return; // document dropped!
                }

                // update the index request so that we can execute additional pipelines (if any), etc
                updateIndexRequestMetadata(indexRequest, ingestDocument.getMetadata());
                try {
                    // check for self-references if necessary, (i.e. if a script processor has run), and clear the bit
                    if (ingestDocument.doNoSelfReferencesCheck()) {
                        CollectionUtils.ensureNoSelfReferences(ingestDocument.getSource(), null);
                        ingestDocument.doNoSelfReferencesCheck(false);
                    }
                } catch (IllegalArgumentException ex) {
                    // An IllegalArgumentException can be thrown when an ingest processor creates a source map that is self-referencing.
                    // In that case, we catch and wrap the exception, so we can include more details
                    listener.onFailure(
                        new IllegalArgumentException(
                            format(
                                "Failed to generate the source document for ingest pipeline [%s] for document [%s/%s]",
                                pipelineId,
                                indexRequest.index(),
                                indexRequest.id()
                            ),
                            ex
                        )
                    );
                    return; // document failed!
                }

                Iterator<String> newPipelineIds = pipelineIds;
                boolean newHasFinalPipeline = hasFinalPipeline;
                final String newIndex = indexRequest.indices()[0];

                if (Objects.equals(originalIndex, newIndex) == false) {
                    if (hasFinalPipeline && pipelineIds.hasNext() == false) {
                        listener.onFailure(
                            new IllegalStateException(
                                format(
                                    "final pipeline [%s] can't change the target index (from [%s] to [%s]) for document [%s]",
                                    pipelineId,
                                    originalIndex,
                                    newIndex,
                                    indexRequest.id()
                                )
                            )
                        );
                        return; // document failed!
                    } else {
                        indexRequest.isPipelineResolved(false);
                        resolvePipelines(null, indexRequest, state.metadata());
                        if (IngestService.NOOP_PIPELINE_NAME.equals(indexRequest.getFinalPipeline()) == false) {
                            newPipelineIds = Collections.singleton(indexRequest.getFinalPipeline()).iterator();
                            newHasFinalPipeline = true;
                        } else {
                            newPipelineIds = Collections.emptyIterator();
                        }
                    }
                }

                if (newPipelineIds.hasNext()) {
                    executePipelines(newPipelineIds, newHasFinalPipeline, indexRequest, ingestDocument, listener);
                } else {
                    // update the index request's source and (potentially) cache the timestamp for TSDB
                    updateIndexRequestSource(indexRequest, ingestDocument);
                    cacheRawTimestamp(indexRequest, ingestDocument);
                    listener.onResponse(true); // document succeeded!
                }
            });
        } catch (Exception e) {
            logger.debug(
                () -> format("failed to execute pipeline [%s] for document [%s/%s]", pipelineId, indexRequest.index(), indexRequest.id()),
                e
            );
            listener.onFailure(e); // document failed!
        }
    }

    private void executePipeline(
        final IngestDocument ingestDocument,
        final Pipeline pipeline,
        final BiConsumer<Boolean, Exception> handler
    ) {
        // adapt our {@code BiConsumer<Boolean, Exception>} handler shape to the
        // {@code BiConsumer<IngestDocument, Exception>} handler shape used internally
        // by ingest pipelines and processors
        ingestDocument.executePipeline(pipeline, (result, e) -> {
            if (e != null) {
                handler.accept(true, e);
            } else {
                handler.accept(result != null, null);
            }
        });
    }

    public IngestStats stats() {
        IngestStats.Builder statsBuilder = new IngestStats.Builder();
        statsBuilder.addTotalMetrics(totalMetrics);
        pipelines.forEach((id, holder) -> {
            Pipeline pipeline = holder.pipeline;
            CompoundProcessor rootProcessor = pipeline.getCompoundProcessor();
            statsBuilder.addPipelineMetrics(id, pipeline.getMetrics());
            List<Tuple<Processor, IngestMetric>> processorMetrics = new ArrayList<>();
            getProcessorMetrics(rootProcessor, processorMetrics);
            processorMetrics.forEach(t -> {
                Processor processor = t.v1();
                IngestMetric processorMetric = t.v2();
                statsBuilder.addProcessorMetrics(id, getProcessorName(processor), processor.getType(), processorMetric);
            });
        });
        return statsBuilder.build();
    }

    /**
     * Adds a listener that gets invoked with the current cluster state before processor factories
     * get invoked.
     * <p>
     * This is useful for components that are used by ingest processors, so that they have the opportunity to update
     * before these components get used by the ingest processor factory.
     */
    public void addIngestClusterStateListener(Consumer<ClusterState> listener) {
        ingestClusterStateListeners.add(listener);
    }

    // package private for testing
    static String getProcessorName(Processor processor) {
        // conditionals are implemented as wrappers around the real processor, so get the real processor for the correct type for the name
        if (processor instanceof ConditionalProcessor conditionalProcessor) {
            processor = conditionalProcessor.getInnerProcessor();
        }
        StringBuilder sb = new StringBuilder(5);
        sb.append(processor.getType());

        if (processor instanceof PipelineProcessor pipelineProcessor) {
            String pipelineName = pipelineProcessor.getPipelineTemplate().newInstance(Map.of()).execute();
            sb.append(":");
            sb.append(pipelineName);
        }
        String tag = processor.getTag();
        if (tag != null && tag.isEmpty() == false) {
            sb.append(":");
            sb.append(tag);
        }
        return sb.toString();
    }

    /**
     * Builds a new ingest document from the passed-in index request.
     */
    private static IngestDocument newIngestDocument(final IndexRequest request) {
        return new IngestDocument(
            request.index(),
            request.id(),
            request.version(),
            request.routing(),
            request.versionType(),
            request.sourceAsMap()
        );
    }

    /**
     * Updates an index request based on the metadata of an ingest document.
     */
    private static void updateIndexRequestMetadata(final IndexRequest request, final org.elasticsearch.script.Metadata metadata) {
        // it's fine to set all metadata fields all the time, as ingest document holds their starting values
        // before ingestion, which might also get modified during ingestion.
        request.index(metadata.getIndex());
        request.id(metadata.getId());
        request.routing(metadata.getRouting());
        request.version(metadata.getVersion());
        if (metadata.getVersionType() != null) {
            request.versionType(VersionType.fromString(metadata.getVersionType()));
        }
        Number number;
        if ((number = metadata.getIfSeqNo()) != null) {
            request.setIfSeqNo(number.longValue());
        }
        if ((number = metadata.getIfPrimaryTerm()) != null) {
            request.setIfPrimaryTerm(number.longValue());
        }
        Map<String, String> map;
        if ((map = metadata.getDynamicTemplates()) != null) {
            Map<String, String> mergedDynamicTemplates = new HashMap<>(request.getDynamicTemplates());
            mergedDynamicTemplates.putAll(map);
            request.setDynamicTemplates(mergedDynamicTemplates);
        }
    }

    /**
     * Updates an index request based on the source of an ingest document, guarding against self-references if necessary.
     */
    private static void updateIndexRequestSource(final IndexRequest request, final IngestDocument document) {
        boolean ensureNoSelfReferences = document.doNoSelfReferencesCheck();
        // we already check for self references elsewhere (and clear the bit), so this should always be false,
        // keeping the check and assert as a guard against extraordinarily surprising circumstances
        assert ensureNoSelfReferences == false;
        request.source(document.getSource(), request.getContentType(), ensureNoSelfReferences);
    }

    /**
     * Grab the @timestamp and store it on the index request so that TSDB can use it without needing to parse
     * the source for this document.
     */
    private static void cacheRawTimestamp(final IndexRequest request, final IngestDocument document) {
        if (request.getRawTimestamp() == null) {
            // cache the @timestamp from the ingest document's source map if there is one
            Object rawTimestamp = document.getSource().get(TimestampField.FIXED_TIMESTAMP_FIELD);
            if (rawTimestamp != null) {
                request.setRawTimestamp(rawTimestamp);
            }
        }
    }

    @Override
    public void applyClusterState(final ClusterChangedEvent event) {
        state = event.state();
        if (state.blocks().hasGlobalBlock(GatewayService.STATE_NOT_RECOVERED_BLOCK)) {
            return;
        }

        // Publish cluster state to components that are used by processor factories before letting
        // processor factories create new processor instances.
        // (Note that this needs to be done also in the case when there is no change to ingest metadata, because in the case
        // when only the part of the cluster state that a component is interested in, is updated.)
        ingestClusterStateListeners.forEach(consumer -> consumer.accept(state));

        IngestMetadata newIngestMetadata = state.getMetadata().custom(IngestMetadata.TYPE);
        if (newIngestMetadata == null) {
            return;
        }

        try {
            innerUpdatePipelines(newIngestMetadata);
        } catch (ElasticsearchParseException e) {
            logger.warn("failed to update ingest pipelines", e);
        }
    }

    synchronized void innerUpdatePipelines(IngestMetadata newIngestMetadata) {
        Map<String, PipelineHolder> existingPipelines = this.pipelines;

        // Lazy initialize these variables in order to favour the most like scenario that there are no pipeline changes:
        Map<String, PipelineHolder> newPipelines = null;
        List<ElasticsearchParseException> exceptions = null;
        // Iterate over pipeline configurations in ingest metadata and constructs a new pipeline if there is no pipeline
        // or the pipeline configuration has been modified
        for (PipelineConfiguration newConfiguration : newIngestMetadata.getPipelines().values()) {
            PipelineHolder previous = existingPipelines.get(newConfiguration.getId());
            if (previous != null && previous.configuration.equals(newConfiguration)) {
                continue;
            }

            if (newPipelines == null) {
                newPipelines = new HashMap<>(existingPipelines);
            }
            try {
                Pipeline newPipeline = Pipeline.create(
                    newConfiguration.getId(),
                    newConfiguration.getConfigAsMap(),
                    processorFactories,
                    scriptService
                );
                newPipelines.put(newConfiguration.getId(), new PipelineHolder(newConfiguration, newPipeline));

                if (previous == null) {
                    continue;
                }
                Pipeline oldPipeline = previous.pipeline;
                newPipeline.getMetrics().add(oldPipeline.getMetrics());
                List<Tuple<Processor, IngestMetric>> oldPerProcessMetrics = new ArrayList<>();
                List<Tuple<Processor, IngestMetric>> newPerProcessMetrics = new ArrayList<>();
                getProcessorMetrics(oldPipeline.getCompoundProcessor(), oldPerProcessMetrics);
                getProcessorMetrics(newPipeline.getCompoundProcessor(), newPerProcessMetrics);
                // Best attempt to populate new processor metrics using a parallel array of the old metrics. This is not ideal since
                // the per processor metrics may get reset when the arrays don't match. However, to get to an ideal model, unique and
                // consistent id's per processor and/or semantic equals for each processor will be needed.
                if (newPerProcessMetrics.size() == oldPerProcessMetrics.size()) {
                    Iterator<Tuple<Processor, IngestMetric>> oldMetricsIterator = oldPerProcessMetrics.iterator();
                    for (Tuple<Processor, IngestMetric> compositeMetric : newPerProcessMetrics) {
                        String type = compositeMetric.v1().getType();
                        IngestMetric metric = compositeMetric.v2();
                        if (oldMetricsIterator.hasNext()) {
                            Tuple<Processor, IngestMetric> oldCompositeMetric = oldMetricsIterator.next();
                            String oldType = oldCompositeMetric.v1().getType();
                            IngestMetric oldMetric = oldCompositeMetric.v2();
                            if (type.equals(oldType)) {
                                metric.add(oldMetric);
                            }
                        }
                    }
                }
            } catch (ElasticsearchParseException e) {
                Pipeline pipeline = substitutePipeline(newConfiguration.getId(), e);
                newPipelines.put(newConfiguration.getId(), new PipelineHolder(newConfiguration, pipeline));
                if (exceptions == null) {
                    exceptions = new ArrayList<>();
                }
                exceptions.add(e);
            } catch (Exception e) {
                ElasticsearchParseException parseException = new ElasticsearchParseException(
                    "Error updating pipeline with id [" + newConfiguration.getId() + "]",
                    e
                );
                Pipeline pipeline = substitutePipeline(newConfiguration.getId(), parseException);
                newPipelines.put(newConfiguration.getId(), new PipelineHolder(newConfiguration, pipeline));
                if (exceptions == null) {
                    exceptions = new ArrayList<>();
                }
                exceptions.add(parseException);
            }
        }

        // Iterate over the current active pipelines and check whether they are missing in the pipeline configuration and
        // if so delete the pipeline from new Pipelines map:
        for (Map.Entry<String, PipelineHolder> entry : existingPipelines.entrySet()) {
            if (newIngestMetadata.getPipelines().get(entry.getKey()) == null) {
                if (newPipelines == null) {
                    newPipelines = new HashMap<>(existingPipelines);
                }
                newPipelines.remove(entry.getKey());
            }
        }

        if (newPipelines != null) {
            // Update the pipelines:
            this.pipelines = Map.copyOf(newPipelines);

            // Rethrow errors that may have occurred during creating new pipeline instances:
            if (exceptions != null) {
                ExceptionsHelper.rethrowAndSuppress(exceptions);
            }
        }
    }

    /**
     * Gets all the Processors of the given type from within a Pipeline.
     * @param pipelineId the pipeline to inspect
     * @param clazz the Processor class to look for
     * @return True if the pipeline contains an instance of the Processor class passed in
     */
    public <P extends Processor> List<P> getProcessorsInPipeline(String pipelineId, Class<P> clazz) {
        Pipeline pipeline = getPipeline(pipelineId);
        if (pipeline == null) {
            throw new IllegalArgumentException("pipeline with id [" + pipelineId + "] does not exist");
        }

        List<P> processors = new ArrayList<>();
        for (Processor processor : pipeline.flattenAllProcessors()) {
            if (clazz.isAssignableFrom(processor.getClass())) {
                processors.add(clazz.cast(processor));
            }

            while (processor instanceof WrappingProcessor wrappingProcessor) {
                if (clazz.isAssignableFrom(wrappingProcessor.getInnerProcessor().getClass())) {
                    processors.add(clazz.cast(wrappingProcessor.getInnerProcessor()));
                }
                processor = wrappingProcessor.getInnerProcessor();
                // break in the case of self referencing processors in the event a processor author creates a
                // wrapping processor that has its inner processor refer to itself.
                if (wrappingProcessor == processor) {
                    break;
                }
            }
        }

        return processors;
    }

    public <P extends Processor> Collection<String> getPipelineWithProcessorType(Class<P> clazz, Predicate<P> predicate) {
        List<String> matchedPipelines = new LinkedList<>();
        for (PipelineHolder holder : pipelines.values()) {
            String pipelineId = holder.pipeline.getId();
            List<P> processors = getProcessorsInPipeline(pipelineId, clazz);
            if (processors.isEmpty() == false && processors.stream().anyMatch(predicate)) {
                matchedPipelines.add(pipelineId);
            }
        }
        return matchedPipelines;
    }

    public synchronized void reloadPipeline(String id) throws Exception {
        PipelineHolder holder = pipelines.get(id);
        Pipeline updatedPipeline = Pipeline.create(id, holder.configuration.getConfigAsMap(), processorFactories, scriptService);
        Map<String, PipelineHolder> updatedPipelines = new HashMap<>(this.pipelines);
        updatedPipelines.put(id, new PipelineHolder(holder.configuration, updatedPipeline));
        this.pipelines = Map.copyOf(updatedPipelines);
    }

    private static Pipeline substitutePipeline(String id, ElasticsearchParseException e) {
        String tag = e.getHeaderKeys().contains("processor_tag") ? e.getHeader("processor_tag").get(0) : null;
        String type = e.getHeaderKeys().contains("processor_type") ? e.getHeader("processor_type").get(0) : "unknown";
        String errorMessage = "pipeline with id [" + id + "] could not be loaded, caused by [" + e.getDetailedMessage() + "]";
        Processor failureProcessor = new AbstractProcessor(tag, "this is a placeholder processor") {
            @Override
            public IngestDocument execute(IngestDocument ingestDocument) {
                throw new IllegalStateException(errorMessage);
            }

            @Override
            public String getType() {
                return type;
            }
        };
        String description = "this is a place holder pipeline, because pipeline with id [" + id + "] could not be loaded";
        return new Pipeline(id, description, null, null, new CompoundProcessor(failureProcessor));
    }

    static class PipelineHolder {

        final PipelineConfiguration configuration;
        final Pipeline pipeline;

        PipelineHolder(PipelineConfiguration configuration, Pipeline pipeline) {
            this.configuration = Objects.requireNonNull(configuration);
            this.pipeline = Objects.requireNonNull(pipeline);
        }
    }

}
