/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.action.bulk;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.apache.lucene.util.SparseFixedBitSet;
import org.elasticsearch.Assertions;
import org.elasticsearch.ElasticsearchParseException;
import org.elasticsearch.ExceptionsHelper;
import org.elasticsearch.ResourceAlreadyExistsException;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionRunnable;
import org.elasticsearch.action.DocWriteRequest;
import org.elasticsearch.action.DocWriteResponse;
import org.elasticsearch.action.RoutingMissingException;
import org.elasticsearch.action.admin.indices.create.AutoCreateAction;
import org.elasticsearch.action.admin.indices.create.CreateIndexRequest;
import org.elasticsearch.action.admin.indices.create.CreateIndexResponse;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.ingest.IngestActionForwarder;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.AutoCreateIndex;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.action.update.TransportUpdateAction;
import org.elasticsearch.action.update.UpdateRequest;
import org.elasticsearch.action.update.UpdateResponse;
import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateObserver;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.DataStream;
import org.elasticsearch.cluster.metadata.IndexAbstraction;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.IndexTemplateMetadata;
import org.elasticsearch.cluster.metadata.MappingMetadata;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.metadata.MetadataIndexTemplateService;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.lease.Releasable;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.concurrent.AbstractRunnable;
import org.elasticsearch.common.util.concurrent.AtomicArray;
import org.elasticsearch.index.Index;
import org.elasticsearch.index.IndexNotFoundException;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.VersionType;
import org.elasticsearch.index.IndexingPressure;
import org.elasticsearch.index.seqno.SequenceNumbers;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.indices.IndexClosedException;
import org.elasticsearch.ingest.IngestService;
import org.elasticsearch.node.NodeClosedException;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicIntegerArray;
import java.util.function.LongSupplier;
import java.util.stream.Collectors;

import static java.util.Collections.emptyMap;
import static org.elasticsearch.index.seqno.SequenceNumbers.UNASSIGNED_PRIMARY_TERM;
import static org.elasticsearch.index.seqno.SequenceNumbers.UNASSIGNED_SEQ_NO;

/**
 * Groups bulk request items by shard, optionally creating non-existent indices and
 * delegates to {@link TransportShardBulkAction} for shard-level bulk execution
 */
public class TransportBulkAction extends HandledTransportAction<BulkRequest, BulkResponse> {

    private static final Logger logger = LogManager.getLogger(TransportBulkAction.class);

    private final ThreadPool threadPool;
    private final AutoCreateIndex autoCreateIndex;
    private final ClusterService clusterService;
    private final IngestService ingestService;
    private final LongSupplier relativeTimeProvider;
    private final IngestActionForwarder ingestForwarder;
    private final NodeClient client;
    private final IndexNameExpressionResolver indexNameExpressionResolver;
    private static final String DROPPED_ITEM_WITH_AUTO_GENERATED_ID = "auto-generated";
    private final IndexingPressure indexingPressure;

    @Inject
    public TransportBulkAction(ThreadPool threadPool, TransportService transportService,
                               ClusterService clusterService, IngestService ingestService,
                               NodeClient client, ActionFilters actionFilters, IndexNameExpressionResolver indexNameExpressionResolver,
                               AutoCreateIndex autoCreateIndex, IndexingPressure indexingPressure) {
        this(threadPool, transportService, clusterService, ingestService, client, actionFilters,
            indexNameExpressionResolver, autoCreateIndex, indexingPressure, System::nanoTime);
    }

    public TransportBulkAction(ThreadPool threadPool, TransportService transportService,
                               ClusterService clusterService, IngestService ingestService,
                               NodeClient client, ActionFilters actionFilters, IndexNameExpressionResolver indexNameExpressionResolver,
                               AutoCreateIndex autoCreateIndex, IndexingPressure indexingPressure, LongSupplier relativeTimeProvider) {
        super(BulkAction.NAME, transportService, actionFilters, BulkRequest::new, ThreadPool.Names.SAME);
        Objects.requireNonNull(relativeTimeProvider);
        this.threadPool = threadPool;
        this.clusterService = clusterService;
        this.ingestService = ingestService;
        this.autoCreateIndex = autoCreateIndex;
        this.relativeTimeProvider = relativeTimeProvider;
        this.ingestForwarder = new IngestActionForwarder(transportService);
        this.client = client;
        this.indexNameExpressionResolver = indexNameExpressionResolver;
        this.indexingPressure = indexingPressure;
        clusterService.addStateApplier(this.ingestForwarder);
    }

    /**
     * Retrieves the {@link IndexRequest} from the provided {@link DocWriteRequest} for index or upsert actions.  Upserts are
     * modeled as {@link IndexRequest} inside the {@link UpdateRequest}. Ignores {@link org.elasticsearch.action.delete.DeleteRequest}'s
     *
     * @param docWriteRequest The request to find the {@link IndexRequest}
     * @return the found {@link IndexRequest} or {@code null} if one can not be found.
     */
    public static IndexRequest getIndexWriteRequest(DocWriteRequest<?> docWriteRequest) {
        IndexRequest indexRequest = null;
        if (docWriteRequest instanceof IndexRequest) {
            indexRequest = (IndexRequest) docWriteRequest;
        } else if (docWriteRequest instanceof UpdateRequest) {
            UpdateRequest updateRequest = (UpdateRequest) docWriteRequest;
            indexRequest = updateRequest.docAsUpsert() ? updateRequest.doc() : updateRequest.upsertRequest();
        }
        return indexRequest;
    }

    @Override
    protected void doExecute(Task task, BulkRequest bulkRequest, ActionListener<BulkResponse> listener) {
        long indexingBytes = bulkRequest.ramBytesUsed();
        final Releasable releasable = indexingPressure.markIndexingOperationStarted(indexingBytes);
        final ActionListener<BulkResponse> releasingListener = ActionListener.runBefore(listener, releasable::close);
        try {
            doInternalExecute(task, bulkRequest, releasingListener);
        } catch (Exception e) {
            releasingListener.onFailure(e);
        }
    }

    protected void doInternalExecute(Task task, BulkRequest bulkRequest, ActionListener<BulkResponse> listener) {
        final long startTime = relativeTime();
        final AtomicArray<BulkItemResponse> responses = new AtomicArray<>(bulkRequest.requests.size());

        boolean hasIndexRequestsWithPipelines = false;
        final Metadata metadata = clusterService.state().getMetadata();
        final Version minNodeVersion = clusterService.state().getNodes().getMinNodeVersion();
        for (DocWriteRequest<?> actionRequest : bulkRequest.requests) {
            IndexRequest indexRequest = getIndexWriteRequest(actionRequest);
            if (indexRequest != null) {
                // Each index request needs to be evaluated, because this method also modifies the IndexRequest
                boolean indexRequestHasPipeline = resolvePipelines(actionRequest, indexRequest, metadata);
                hasIndexRequestsWithPipelines |= indexRequestHasPipeline;
            }

            if (actionRequest instanceof IndexRequest) {
                IndexRequest ir = (IndexRequest) actionRequest;
                ir.checkAutoIdWithOpTypeCreateSupportedByVersion(minNodeVersion);
                if (ir.getAutoGeneratedTimestamp() != IndexRequest.UNSET_AUTO_GENERATED_TIMESTAMP) {
                    throw new IllegalArgumentException("autoGeneratedTimestamp should not be set externally");
                }
            }
        }

        if (hasIndexRequestsWithPipelines) {
            // this method (doExecute) will be called again, but with the bulk requests updated from the ingest node processing but
            // also with IngestService.NOOP_PIPELINE_NAME on each request. This ensures that this on the second time through this method,
            // this path is never taken.
            try {
                if (Assertions.ENABLED) {
                    final boolean arePipelinesResolved = bulkRequest.requests()
                        .stream()
                        .map(TransportBulkAction::getIndexWriteRequest)
                        .filter(Objects::nonNull)
                        .allMatch(IndexRequest::isPipelineResolved);
                    assert arePipelinesResolved : bulkRequest;
                }
                if (clusterService.localNode().isIngestNode()) {
                    processBulkIndexIngestRequest(task, bulkRequest, listener);
                } else {
                    ingestForwarder.forwardIngestRequest(BulkAction.INSTANCE, bulkRequest, listener);
                }
            } catch (Exception e) {
                listener.onFailure(e);
            }
            return;
        }

        if (needToCheck()) {
            // Attempt to create all the indices that we're going to need during the bulk before we start.
            // Step 1: collect all the indices in the request
            final Set<String> indices = bulkRequest.requests.stream()
                    // delete requests should not attempt to create the index (if the index does not
                    // exists), unless an external versioning is used
                .filter(request -> request.opType() != DocWriteRequest.OpType.DELETE
                        || request.versionType() == VersionType.EXTERNAL
                        || request.versionType() == VersionType.EXTERNAL_GTE)
                .map(DocWriteRequest::index)
                .collect(Collectors.toSet());
            /* Step 2: filter that to indices that don't exist and we can create. At the same time build a map of indices we can't create
             * that we'll use when we try to run the requests. */
            final Map<String, IndexNotFoundException> indicesThatCannotBeCreated = new HashMap<>();
            Set<String> autoCreateIndices = new HashSet<>();
            ClusterState state = clusterService.state();
            for (String index : indices) {
                boolean shouldAutoCreate;
                try {
                    shouldAutoCreate = shouldAutoCreate(index, state);
                } catch (IndexNotFoundException e) {
                    shouldAutoCreate = false;
                    indicesThatCannotBeCreated.put(index, e);
                }
                if (shouldAutoCreate) {
                    autoCreateIndices.add(index);
                }
            }
            // Step 3: create all the indices that are missing, if there are any missing. start the bulk after all the creates come back.
            if (autoCreateIndices.isEmpty()) {
                executeBulk(task, bulkRequest, startTime, listener, responses, indicesThatCannotBeCreated);
            } else {
                final AtomicInteger counter = new AtomicInteger(autoCreateIndices.size());
                for (String index : autoCreateIndices) {
                    createIndex(index, bulkRequest.timeout(), minNodeVersion, new ActionListener<>() {
                        @Override
                        public void onResponse(CreateIndexResponse result) {
                            if (counter.decrementAndGet() == 0) {
                                threadPool.executor(ThreadPool.Names.WRITE).execute(
                                    () -> executeBulk(task, bulkRequest, startTime, listener, responses, indicesThatCannotBeCreated));
                            }
                        }

                        @Override
                        public void onFailure(Exception e) {
                            if (!(ExceptionsHelper.unwrapCause(e) instanceof ResourceAlreadyExistsException)) {
                                // fail all requests involving this index, if create didn't work
                                for (int i = 0; i < bulkRequest.requests.size(); i++) {
                                    DocWriteRequest<?> request = bulkRequest.requests.get(i);
                                    if (request != null && setResponseFailureIfIndexMatches(responses, i, request, index, e)) {
                                        bulkRequest.requests.set(i, null);
                                    }
                                }
                            }
                            if (counter.decrementAndGet() == 0) {
                                executeBulk(task, bulkRequest, startTime, ActionListener.wrap(listener::onResponse, inner -> {
                                    inner.addSuppressed(e);
                                    listener.onFailure(inner);
                                }), responses, indicesThatCannotBeCreated);
                            }
                        }
                    });
                }
            }
        } else {
            executeBulk(task, bulkRequest, startTime, listener, responses, emptyMap());
        }
    }

    static void prohibitAppendWritesInBackingIndices(DocWriteRequest<?> writeRequest, Metadata metadata) {
        IndexAbstraction indexAbstraction = metadata.getIndicesLookup().get(writeRequest.index());
        if (indexAbstraction == null) {
            return;
        }
        if (indexAbstraction.getType() != IndexAbstraction.Type.CONCRETE_INDEX) {
            return;
        }
        if (indexAbstraction.getParentDataStream() == null) {
            return;
        }

        DataStream dataStream = indexAbstraction.getParentDataStream().getDataStream();

        // At this point with write op is targeting a backing index of a data stream directly,
        // so checking if write op is append-only and if so fail.
        // (Updates and deletes are allowed to target a backing index)

        DocWriteRequest.OpType opType = writeRequest.opType();
        // CREATE op_type is considered append-only and
        // INDEX op_type is considered append-only when no if_primary_term and if_seq_no is specified.
        // (the latter maybe an update, but at this stage we can't determine that. In order to determine
        // that an engine level change is needed and for now this check is sufficient.)
        if (opType == DocWriteRequest.OpType.CREATE) {
            throw new IllegalArgumentException("index request with op_type=create targeting backing indices is disallowed, " +
                "target corresponding data stream [" + dataStream.getName() + "] instead");
        }
        if (opType == DocWriteRequest.OpType.INDEX && writeRequest.ifPrimaryTerm() == UNASSIGNED_PRIMARY_TERM &&
            writeRequest.ifSeqNo() == UNASSIGNED_SEQ_NO) {
            throw new IllegalArgumentException("index request with op_type=index and no if_primary_term and if_seq_no set " +
                "targeting backing indices is disallowed, target corresponding data stream [" + dataStream.getName() + "] instead");
        }
    }

    static void prohibitCustomRoutingOnDataStream(DocWriteRequest<?> writeRequest, Metadata metadata) {
        IndexAbstraction indexAbstraction = metadata.getIndicesLookup().get(writeRequest.index());
        if (indexAbstraction == null) {
            return;
        }
        if (indexAbstraction.getType() != IndexAbstraction.Type.DATA_STREAM) {
            return;
        }

        if (writeRequest.routing() != null) {
            IndexAbstraction.DataStream dataStream = (IndexAbstraction.DataStream) indexAbstraction;
            throw new IllegalArgumentException("index request targeting data stream [" + dataStream.getName() + "] specifies a custom " +
                "routing. target the backing indices directly or remove the custom routing.");
        }
    }

    static boolean resolvePipelines(final DocWriteRequest<?> originalRequest, final IndexRequest indexRequest, final Metadata metadata) {
        if (indexRequest.isPipelineResolved() == false) {
            final String requestPipeline = indexRequest.getPipeline();
            indexRequest.setPipeline(IngestService.NOOP_PIPELINE_NAME);
            indexRequest.setFinalPipeline(IngestService.NOOP_PIPELINE_NAME);
            String defaultPipeline = null;
            String finalPipeline = null;
            // start to look for default or final pipelines via settings found in the index meta data
            IndexMetadata indexMetadata = metadata.indices().get(originalRequest.index());
            // check the alias for the index request (this is how normal index requests are modeled)
            if (indexMetadata == null && indexRequest.index() != null) {
                IndexAbstraction indexAbstraction = metadata.getIndicesLookup().get(indexRequest.index());
                if (indexAbstraction != null) {
                    indexMetadata = indexAbstraction.getWriteIndex();
                }
            }
            // check the alias for the action request (this is how upserts are modeled)
            if (indexMetadata == null && originalRequest.index() != null) {
                IndexAbstraction indexAbstraction = metadata.getIndicesLookup().get(originalRequest.index());
                if (indexAbstraction != null) {
                    indexMetadata = indexAbstraction.getWriteIndex();
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
                    if (defaultPipeline == null && IndexSettings.DEFAULT_PIPELINE.exists(settings)) {
                        defaultPipeline = IndexSettings.DEFAULT_PIPELINE.get(settings);
                        // we can not break in case a lower-order template has a final pipeline that we need to collect
                    }
                    if (finalPipeline == null && IndexSettings.FINAL_PIPELINE.exists(settings)) {
                        finalPipeline = IndexSettings.FINAL_PIPELINE.get(settings);
                        // we can not break in case a lower-order template has a default pipeline that we need to collect
                    }
                    indexRequest.setPipeline(Objects.requireNonNullElse(defaultPipeline, IngestService.NOOP_PIPELINE_NAME));
                    indexRequest.setFinalPipeline(Objects.requireNonNullElse(finalPipeline, IngestService.NOOP_PIPELINE_NAME));
                } else {
                    List<IndexTemplateMetadata> templates =
                            MetadataIndexTemplateService.findV1Templates(metadata, indexRequest.index(), null);
                    assert (templates != null);
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
                    indexRequest.setPipeline(Objects.requireNonNullElse(defaultPipeline, IngestService.NOOP_PIPELINE_NAME));
                    indexRequest.setFinalPipeline(Objects.requireNonNullElse(finalPipeline, IngestService.NOOP_PIPELINE_NAME));
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
        return IngestService.NOOP_PIPELINE_NAME.equals(indexRequest.getPipeline()) == false
            || IngestService.NOOP_PIPELINE_NAME.equals(indexRequest.getFinalPipeline()) == false;
    }

    boolean needToCheck() {
        return autoCreateIndex.needToCheck();
    }

    boolean shouldAutoCreate(String index, ClusterState state) {
        return autoCreateIndex.shouldAutoCreate(index, state);
    }

    void createIndex(String index,
                     TimeValue timeout,
                     Version minNodeVersion,
                     ActionListener<CreateIndexResponse> listener) {
        CreateIndexRequest createIndexRequest = new CreateIndexRequest();
        createIndexRequest.index(index);
        createIndexRequest.cause("auto(bulk api)");
        createIndexRequest.masterNodeTimeout(timeout);
        if (minNodeVersion.onOrAfter(Version.V_7_8_0)) {
            client.execute(AutoCreateAction.INSTANCE, createIndexRequest, listener);
        } else {
            client.admin().indices().create(createIndexRequest, listener);
        }
    }

    private boolean setResponseFailureIfIndexMatches(AtomicArray<BulkItemResponse> responses, int idx, DocWriteRequest<?> request,
                                                     String index, Exception e) {
        if (index.equals(request.index())) {
            responses.set(idx, new BulkItemResponse(idx, request.opType(), new BulkItemResponse.Failure(request.index(),
                request.id(), e)));
            return true;
        }
        return false;
    }

    private long buildTookInMillis(long startTimeNanos) {
        return TimeUnit.NANOSECONDS.toMillis(relativeTime() - startTimeNanos);
    }

    /**
     * retries on retryable cluster blocks, resolves item requests,
     * constructs shard bulk requests and delegates execution to shard bulk action
     * */
    private final class BulkOperation extends ActionRunnable<BulkResponse> {
        private final Task task;
        private BulkRequest bulkRequest; // set to null once all requests are sent out
        private final AtomicArray<BulkItemResponse> responses;
        private final long startTimeNanos;
        private final ClusterStateObserver observer;
        private final Map<String, IndexNotFoundException> indicesThatCannotBeCreated;

        BulkOperation(Task task, BulkRequest bulkRequest, ActionListener<BulkResponse> listener, AtomicArray<BulkItemResponse> responses,
                long startTimeNanos, Map<String, IndexNotFoundException> indicesThatCannotBeCreated) {
            super(listener);
            this.task = task;
            this.bulkRequest = bulkRequest;
            this.responses = responses;
            this.startTimeNanos = startTimeNanos;
            this.indicesThatCannotBeCreated = indicesThatCannotBeCreated;
            this.observer = new ClusterStateObserver(clusterService, bulkRequest.timeout(), logger, threadPool.getThreadContext());
        }

        @Override
        protected void doRun() {
            assert bulkRequest != null;
            final ClusterState clusterState = observer.setAndGetObservedState();
            if (handleBlockExceptions(clusterState)) {
                return;
            }
            final ConcreteIndices concreteIndices = new ConcreteIndices(clusterState, indexNameExpressionResolver);
            Metadata metadata = clusterState.metadata();
            for (int i = 0; i < bulkRequest.requests.size(); i++) {
                DocWriteRequest<?> docWriteRequest = bulkRequest.requests.get(i);
                //the request can only be null because we set it to null in the previous step, so it gets ignored
                if (docWriteRequest == null) {
                    continue;
                }
                if (addFailureIfIndexIsUnavailable(docWriteRequest, i, concreteIndices, metadata)) {
                    continue;
                }
                Index concreteIndex = concreteIndices.resolveIfAbsent(docWriteRequest);
                try {
                    switch (docWriteRequest.opType()) {
                        case CREATE:
                        case INDEX:
                            prohibitAppendWritesInBackingIndices(docWriteRequest, metadata);
                            prohibitCustomRoutingOnDataStream(docWriteRequest, metadata);
                            IndexRequest indexRequest = (IndexRequest) docWriteRequest;
                            final IndexMetadata indexMetadata = metadata.index(concreteIndex);
                            MappingMetadata mappingMd = indexMetadata.mapping();
                            Version indexCreated = indexMetadata.getCreationVersion();
                            indexRequest.resolveRouting(metadata);
                            indexRequest.process(indexCreated, mappingMd, concreteIndex.getName());
                            break;
                        case UPDATE:
                            TransportUpdateAction.resolveAndValidateRouting(metadata, concreteIndex.getName(),
                                (UpdateRequest) docWriteRequest);
                            break;
                        case DELETE:
                            docWriteRequest.routing(metadata.resolveWriteIndexRouting(docWriteRequest.routing(), docWriteRequest.index()));
                            // check if routing is required, if so, throw error if routing wasn't specified
                            if (docWriteRequest.routing() == null && metadata.routingRequired(concreteIndex.getName())) {
                                throw new RoutingMissingException(concreteIndex.getName(), docWriteRequest.id());
                            }
                            break;
                        default: throw new AssertionError("request type not supported: [" + docWriteRequest.opType() + "]");
                    }
                } catch (ElasticsearchParseException | IllegalArgumentException | RoutingMissingException e) {
                    BulkItemResponse.Failure failure = new BulkItemResponse.Failure(concreteIndex.getName(),
                        docWriteRequest.id(), e);
                    BulkItemResponse bulkItemResponse = new BulkItemResponse(i, docWriteRequest.opType(), failure);
                    responses.set(i, bulkItemResponse);
                    // make sure the request gets never processed again
                    bulkRequest.requests.set(i, null);
                }
            }

            // first, go over all the requests and create a ShardId -> Operations mapping
            Map<ShardId, List<BulkItemRequest>> requestsByShard = new HashMap<>();
            for (int i = 0; i < bulkRequest.requests.size(); i++) {
                DocWriteRequest<?> request = bulkRequest.requests.get(i);
                if (request == null) {
                    continue;
                }
                String concreteIndex = concreteIndices.getConcreteIndex(request.index()).getName();
                ShardId shardId = clusterService.operationRouting().indexShards(clusterState, concreteIndex, request.id(),
                    request.routing()).shardId();
                List<BulkItemRequest> shardRequests = requestsByShard.computeIfAbsent(shardId, shard -> new ArrayList<>());
                shardRequests.add(new BulkItemRequest(i, request));
            }

            if (requestsByShard.isEmpty()) {
                listener.onResponse(new BulkResponse(responses.toArray(new BulkItemResponse[responses.length()]),
                    buildTookInMillis(startTimeNanos)));
                return;
            }

            final AtomicInteger counter = new AtomicInteger(requestsByShard.size());
            String nodeId = clusterService.localNode().getId();
            for (Map.Entry<ShardId, List<BulkItemRequest>> entry : requestsByShard.entrySet()) {
                final ShardId shardId = entry.getKey();
                final List<BulkItemRequest> requests = entry.getValue();
                BulkShardRequest bulkShardRequest = new BulkShardRequest(shardId, bulkRequest.getRefreshPolicy(),
                        requests.toArray(new BulkItemRequest[requests.size()]));
                bulkShardRequest.waitForActiveShards(bulkRequest.waitForActiveShards());
                bulkShardRequest.timeout(bulkRequest.timeout());
                bulkShardRequest.routedBasedOnClusterVersion(clusterState.version());
                if (task != null) {
                    bulkShardRequest.setParentTask(nodeId, task.getId());
                }
                client.executeLocally(TransportShardBulkAction.TYPE, bulkShardRequest, new ActionListener<>() {
                    @Override
                    public void onResponse(BulkShardResponse bulkShardResponse) {
                        for (BulkItemResponse bulkItemResponse : bulkShardResponse.getResponses()) {
                            // we may have no response if item failed
                            if (bulkItemResponse.getResponse() != null) {
                                bulkItemResponse.getResponse().setShardInfo(bulkShardResponse.getShardInfo());
                            }
                            responses.set(bulkItemResponse.getItemId(), bulkItemResponse);
                        }
                        if (counter.decrementAndGet() == 0) {
                            finishHim();
                        }
                    }

                    @Override
                    public void onFailure(Exception e) {
                        // create failures for all relevant requests
                        for (BulkItemRequest request : requests) {
                            final String indexName = concreteIndices.getConcreteIndex(request.index()).getName();
                            DocWriteRequest<?> docWriteRequest = request.request();
                            responses.set(request.id(), new BulkItemResponse(request.id(), docWriteRequest.opType(),
                                    new BulkItemResponse.Failure(indexName, docWriteRequest.id(), e)));
                        }
                        if (counter.decrementAndGet() == 0) {
                            finishHim();
                        }
                    }

                    private void finishHim() {
                        listener.onResponse(new BulkResponse(responses.toArray(new BulkItemResponse[responses.length()]),
                            buildTookInMillis(startTimeNanos)));
                    }
                });
            }
            bulkRequest = null; // allow memory for bulk request items to be reclaimed before all items have been completed
        }

        private boolean handleBlockExceptions(ClusterState state) {
            ClusterBlockException blockException = state.blocks().globalBlockedException(ClusterBlockLevel.WRITE);
            if (blockException != null) {
                if (blockException.retryable()) {
                    logger.trace("cluster is blocked, scheduling a retry", blockException);
                    retry(blockException);
                } else {
                    onFailure(blockException);
                }
                return true;
            }
            return false;
        }

        void retry(Exception failure) {
            assert failure != null;
            if (observer.isTimedOut()) {
                // we running as a last attempt after a timeout has happened. don't retry
                onFailure(failure);
                return;
            }
            observer.waitForNextChange(new ClusterStateObserver.Listener() {
                @Override
                public void onNewClusterState(ClusterState state) {
                    run();
                }

                @Override
                public void onClusterServiceClose() {
                    onFailure(new NodeClosedException(clusterService.localNode()));
                }

                @Override
                public void onTimeout(TimeValue timeout) {
                    // Try one more time...
                    run();
                }
            });
        }

        private boolean addFailureIfIndexIsUnavailable(DocWriteRequest<?> request, int idx, final ConcreteIndices concreteIndices,
                final Metadata metadata) {
            IndexNotFoundException cannotCreate = indicesThatCannotBeCreated.get(request.index());
            if (cannotCreate != null) {
                addFailure(request, idx, cannotCreate);
                return true;
            }
            Index concreteIndex = concreteIndices.getConcreteIndex(request.index());
            if (concreteIndex == null) {
                try {
                    concreteIndex = concreteIndices.resolveIfAbsent(request);
                } catch (IndexClosedException | IndexNotFoundException ex) {
                    addFailure(request, idx, ex);
                    return true;
                }
            }
            IndexMetadata indexMetadata = metadata.getIndexSafe(concreteIndex);
            if (indexMetadata.getState() == IndexMetadata.State.CLOSE) {
                addFailure(request, idx, new IndexClosedException(concreteIndex));
                return true;
            }
            return false;
        }

        private void addFailure(DocWriteRequest<?> request, int idx, Exception unavailableException) {
            BulkItemResponse.Failure failure = new BulkItemResponse.Failure(request.index(), request.id(),
                    unavailableException);
            BulkItemResponse bulkItemResponse = new BulkItemResponse(idx, request.opType(), failure);
            responses.set(idx, bulkItemResponse);
            // make sure the request gets never processed again
            bulkRequest.requests.set(idx, null);
        }
    }

    void executeBulk(Task task, final BulkRequest bulkRequest, final long startTimeNanos, final ActionListener<BulkResponse> listener,
            final AtomicArray<BulkItemResponse> responses, Map<String, IndexNotFoundException> indicesThatCannotBeCreated) {
        new BulkOperation(task, bulkRequest, listener, responses, startTimeNanos, indicesThatCannotBeCreated).run();
    }

    private static class ConcreteIndices  {
        private final ClusterState state;
        private final IndexNameExpressionResolver indexNameExpressionResolver;
        private final Map<String, Index> indices = new HashMap<>();

        ConcreteIndices(ClusterState state, IndexNameExpressionResolver indexNameExpressionResolver) {
            this.state = state;
            this.indexNameExpressionResolver = indexNameExpressionResolver;
        }

        Index getConcreteIndex(String indexOrAlias) {
            return indices.get(indexOrAlias);
        }

        Index resolveIfAbsent(DocWriteRequest<?> request) {
            Index concreteIndex = indices.get(request.index());
            if (concreteIndex == null) {
                boolean includeDataStreams = request.opType() == DocWriteRequest.OpType.CREATE;
                concreteIndex = indexNameExpressionResolver.concreteWriteIndex(state, request.indicesOptions(), request.indices()[0],
                    false, includeDataStreams);
                indices.put(request.index(), concreteIndex);
            }
            return concreteIndex;
        }
    }

    private long relativeTime() {
        return relativeTimeProvider.getAsLong();
    }

    private void processBulkIndexIngestRequest(Task task, BulkRequest original, ActionListener<BulkResponse> listener) {
        final long ingestStartTimeInNanos = System.nanoTime();
        final BulkRequestModifier bulkRequestModifier = new BulkRequestModifier(original);
        ingestService.executeBulkRequest(
            original.numberOfActions(),
            () -> bulkRequestModifier,
            bulkRequestModifier::markItemAsFailed,
            (originalThread, exception) -> {
                if (exception != null) {
                    logger.debug("failed to execute pipeline for a bulk request", exception);
                    listener.onFailure(exception);
                } else {
                    long ingestTookInMillis = TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - ingestStartTimeInNanos);
                    BulkRequest bulkRequest = bulkRequestModifier.getBulkRequest();
                    ActionListener<BulkResponse> actionListener = bulkRequestModifier.wrapActionListenerIfNeeded(ingestTookInMillis,
                        listener);
                    if (bulkRequest.requests().isEmpty()) {
                        // at this stage, the transport bulk action can't deal with a bulk request with no requests,
                        // so we stop and send an empty response back to the client.
                        // (this will happen if pre-processing all items in the bulk failed)
                        actionListener.onResponse(new BulkResponse(new BulkItemResponse[0], 0));
                    } else {
                        // If a processor went async and returned a response on a different thread then
                        // before we continue the bulk request we should fork back on a write thread:
                        if (originalThread == Thread.currentThread()) {
                            assert Thread.currentThread().getName().contains(ThreadPool.Names.WRITE);
                            doInternalExecute(task, bulkRequest, actionListener);
                        } else {
                            threadPool.executor(ThreadPool.Names.WRITE).execute(new AbstractRunnable() {
                                @Override
                                public void onFailure(Exception e) {
                                    listener.onFailure(e);
                                }

                                @Override
                                protected void doRun() throws Exception {
                                    doInternalExecute(task, bulkRequest, actionListener);
                                }

                                @Override
                                public boolean isForceExecution() {
                                    // If we fork back to a write thread we **not** should fail, because tp queue is full.
                                    // (Otherwise the work done during ingest will be lost)
                                    // It is okay to force execution here. Throttling of write requests happens prior to
                                    // ingest when a node receives a bulk request.
                                    return true;
                                }
                            });
                        }
                    }
                }
            },
            bulkRequestModifier::markItemAsDropped
        );
    }

    static final class BulkRequestModifier implements Iterator<DocWriteRequest<?>> {

        private static final Logger logger = LogManager.getLogger(BulkRequestModifier.class);

        final BulkRequest bulkRequest;
        final SparseFixedBitSet failedSlots;
        final List<BulkItemResponse> itemResponses;
        final AtomicIntegerArray originalSlots;

        volatile int currentSlot = -1;

        BulkRequestModifier(BulkRequest bulkRequest) {
            this.bulkRequest = bulkRequest;
            this.failedSlots = new SparseFixedBitSet(bulkRequest.requests().size());
            this.itemResponses = new ArrayList<>(bulkRequest.requests().size());
            this.originalSlots = new AtomicIntegerArray(bulkRequest.requests().size()); // oversize, but that's ok
        }

        @Override
        public DocWriteRequest<?> next() {
            return bulkRequest.requests().get(++currentSlot);
        }

        @Override
        public boolean hasNext() {
            return (currentSlot + 1) < bulkRequest.requests().size();
        }

        BulkRequest getBulkRequest() {
            if (itemResponses.isEmpty()) {
                return bulkRequest;
            } else {
                BulkRequest modifiedBulkRequest = new BulkRequest();
                modifiedBulkRequest.setRefreshPolicy(bulkRequest.getRefreshPolicy());
                modifiedBulkRequest.waitForActiveShards(bulkRequest.waitForActiveShards());
                modifiedBulkRequest.timeout(bulkRequest.timeout());

                int slot = 0;
                List<DocWriteRequest<?>> requests = bulkRequest.requests();
                for (int i = 0; i < requests.size(); i++) {
                    DocWriteRequest<?> request = requests.get(i);
                    if (failedSlots.get(i) == false) {
                        modifiedBulkRequest.add(request);
                        originalSlots.set(slot++, i);
                    }
                }
                return modifiedBulkRequest;
            }
        }

        ActionListener<BulkResponse> wrapActionListenerIfNeeded(long ingestTookInMillis, ActionListener<BulkResponse> actionListener) {
            if (itemResponses.isEmpty()) {
                return ActionListener.map(actionListener,
                    response -> new BulkResponse(response.getItems(), response.getTook().getMillis(), ingestTookInMillis));
            } else {
                return ActionListener.delegateFailure(actionListener, (delegatedListener, response) -> {
                    BulkItemResponse[] items = response.getItems();
                    for (int i = 0; i < items.length; i++) {
                        itemResponses.add(originalSlots.get(i), response.getItems()[i]);
                    }
                    delegatedListener.onResponse(
                        new BulkResponse(
                            itemResponses.toArray(new BulkItemResponse[0]), response.getTook().getMillis(), ingestTookInMillis));
                });
            }
        }

        synchronized void markItemAsDropped(int slot) {
            IndexRequest indexRequest = getIndexWriteRequest(bulkRequest.requests().get(slot));
            failedSlots.set(slot);
            final String id = indexRequest.id() == null ? DROPPED_ITEM_WITH_AUTO_GENERATED_ID : indexRequest.id();
            itemResponses.add(
                new BulkItemResponse(slot, indexRequest.opType(),
                    new UpdateResponse(
                        new ShardId(indexRequest.index(), IndexMetadata.INDEX_UUID_NA_VALUE, 0),
                        id, SequenceNumbers.UNASSIGNED_SEQ_NO, SequenceNumbers.UNASSIGNED_PRIMARY_TERM,
                        indexRequest.version(), DocWriteResponse.Result.NOOP
                    )
                )
            );
        }

        synchronized void markItemAsFailed(int slot, Exception e) {
            IndexRequest indexRequest = getIndexWriteRequest(bulkRequest.requests().get(slot));
            logger.debug(() -> new ParameterizedMessage("failed to execute pipeline [{}] for document [{}/{}]",
                indexRequest.getPipeline(), indexRequest.index(), indexRequest.id()), e);

            // We hit a error during preprocessing a request, so we:
            // 1) Remember the request item slot from the bulk, so that we're done processing all requests we know what failed
            // 2) Add a bulk item failure for this request
            // 3) Continue with the next request in the bulk.
            failedSlots.set(slot);
            BulkItemResponse.Failure failure = new BulkItemResponse.Failure(indexRequest.index(), indexRequest.id(), e);
            itemResponses.add(new BulkItemResponse(slot, indexRequest.opType(), failure));
        }

    }
}
