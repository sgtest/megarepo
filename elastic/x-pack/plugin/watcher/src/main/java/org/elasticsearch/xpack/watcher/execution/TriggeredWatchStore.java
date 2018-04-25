/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.watcher.execution;

import org.apache.logging.log4j.message.ParameterizedMessage;
import org.apache.logging.log4j.util.Supplier;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.indices.refresh.RefreshRequest;
import org.elasticsearch.action.bulk.BulkItemResponse;
import org.elasticsearch.action.bulk.BulkRequest;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.action.delete.DeleteRequest;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.search.ClearScrollRequest;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.search.SearchScrollRequest;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.routing.Preference;
import org.elasticsearch.common.component.AbstractComponent;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.index.IndexNotFoundException;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.search.sort.SortBuilders;
import org.elasticsearch.xpack.core.watcher.execution.TriggeredWatchStoreField;
import org.elasticsearch.xpack.core.watcher.execution.Wid;
import org.elasticsearch.xpack.core.watcher.watch.Watch;
import org.elasticsearch.xpack.watcher.watch.WatchStoreUtils;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Set;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.stream.Collectors;

import static org.elasticsearch.xpack.core.ClientHelper.WATCHER_ORIGIN;
import static org.elasticsearch.xpack.core.ClientHelper.executeAsyncWithOrigin;
import static org.elasticsearch.xpack.core.ClientHelper.stashWithOrigin;
import static org.elasticsearch.xpack.core.watcher.support.Exceptions.illegalState;

public class TriggeredWatchStore extends AbstractComponent {

    private final int scrollSize;
    private final Client client;
    private final TimeValue scrollTimeout;
    private final TriggeredWatch.Parser triggeredWatchParser;

    private final AtomicBoolean started = new AtomicBoolean(false);
    private final TimeValue defaultBulkTimeout;
    private final TimeValue defaultSearchTimeout;

    public TriggeredWatchStore(Settings settings, Client client, TriggeredWatch.Parser triggeredWatchParser) {
        super(settings);
        this.scrollSize = settings.getAsInt("xpack.watcher.execution.scroll.size", 1000);
        this.client = client;
        this.scrollTimeout = settings.getAsTime("xpack.watcher.execution.scroll.timeout", TimeValue.timeValueMinutes(5));
        this.defaultBulkTimeout = settings.getAsTime("xpack.watcher.internal.ops.bulk.default_timeout", TimeValue.timeValueSeconds(120));
        this.defaultSearchTimeout = settings.getAsTime("xpack.watcher.internal.ops.search.default_timeout", TimeValue.timeValueSeconds(30));
        this.triggeredWatchParser = triggeredWatchParser;
        this.started.set(true);
    }

    public void start() {
        started.set(true);
    }

    public boolean validate(ClusterState state) {
        try {
            IndexMetaData indexMetaData = WatchStoreUtils.getConcreteIndex(TriggeredWatchStoreField.INDEX_NAME, state.metaData());
            if (indexMetaData == null) {
                return true;
            } else {
                if (indexMetaData.getState() == IndexMetaData.State.CLOSE) {
                    logger.debug("triggered watch index [{}] is marked as closed, watcher cannot be started",
                            indexMetaData.getIndex().getName());
                    return false;
                } else {
                    return state.routingTable().index(indexMetaData.getIndex()).allPrimaryShardsActive();
                }
            }
        } catch (IllegalStateException e) {
            logger.trace((Supplier<?>) () -> new ParameterizedMessage("error getting index meta data [{}]: ",
                    TriggeredWatchStoreField.INDEX_NAME), e);
            return false;
        }
    }

    public void stop() {
        started.set(false);
    }

    public void putAll(final List<TriggeredWatch> triggeredWatches, final ActionListener<BulkResponse> listener) throws IOException {
        if (triggeredWatches.isEmpty()) {
            listener.onResponse(new BulkResponse(new BulkItemResponse[]{}, 0));
            return;
        }

        ensureStarted();
        executeAsyncWithOrigin(client.threadPool().getThreadContext(), WATCHER_ORIGIN, createBulkRequest(triggeredWatches,
                TriggeredWatchStoreField.DOC_TYPE), listener, client::bulk);
    }

    public BulkResponse putAll(final List<TriggeredWatch> triggeredWatches) throws IOException {
        PlainActionFuture<BulkResponse> future = PlainActionFuture.newFuture();
        putAll(triggeredWatches, future);
        return future.actionGet(defaultBulkTimeout);
    }

    /**
     * Create a bulk request from the triggered watches with a specified document type
     * @param triggeredWatches  The list of triggered watches
     * @param docType           The document type to use, either the current one or legacy
     * @return                  The bulk request for the triggered watches
     * @throws IOException      If a triggered watch could not be parsed to JSON, this exception is thrown
     */
    private BulkRequest createBulkRequest(final List<TriggeredWatch> triggeredWatches, String docType) throws IOException {
        BulkRequest request = new BulkRequest();
        for (TriggeredWatch triggeredWatch : triggeredWatches) {
            IndexRequest indexRequest = new IndexRequest(TriggeredWatchStoreField.INDEX_NAME, docType, triggeredWatch.id().value());
            try (XContentBuilder builder = XContentFactory.jsonBuilder()) {
                triggeredWatch.toXContent(builder, ToXContent.EMPTY_PARAMS);
                indexRequest.source(builder);
            }
            indexRequest.opType(IndexRequest.OpType.CREATE);
            request.add(indexRequest);
        }
        return request;
    }

    public void delete(Wid wid) {
        ensureStarted();
        DeleteRequest request = new DeleteRequest(TriggeredWatchStoreField.INDEX_NAME, TriggeredWatchStoreField.DOC_TYPE, wid.value());
        try (ThreadContext.StoredContext ignore = stashWithOrigin(client.threadPool().getThreadContext(), WATCHER_ORIGIN)) {
            client.delete(request); // FIXME shouldn't we wait before saying the delete was successful
        }
        logger.trace("successfully deleted triggered watch with id [{}]", wid);
    }

    private void ensureStarted() {
        if (!started.get()) {
            throw illegalState("unable to persist triggered watches, the store is not ready");
        }
    }

    /**
     * Checks if any of the loaded watches has been put into the triggered watches index for immediate execution
     *
     * Note: This is executing a blocking call over the network, thus a potential source of problems
     *
     * @param watches       The list of watches that will be loaded here
     * @param clusterState  The current cluster state
     * @return              A list of triggered watches that have been started to execute somewhere else but not finished
     */
    public Collection<TriggeredWatch> findTriggeredWatches(Collection<Watch> watches, ClusterState clusterState) {
        if (watches.isEmpty()) {
            return Collections.emptyList();
        }

        // non existing index, return immediately
        IndexMetaData indexMetaData = WatchStoreUtils.getConcreteIndex(TriggeredWatchStoreField.INDEX_NAME, clusterState.metaData());
        if (indexMetaData == null) {
            return Collections.emptyList();
        }

        try (ThreadContext.StoredContext ignore = stashWithOrigin(client.threadPool().getThreadContext(), WATCHER_ORIGIN)) {
            client.admin().indices().refresh(new RefreshRequest(TriggeredWatchStoreField.INDEX_NAME))
                    .actionGet(TimeValue.timeValueSeconds(5));
        } catch (IndexNotFoundException e) {
            return Collections.emptyList();
        }

        Set<String> ids = watches.stream().map(Watch::id).collect(Collectors.toSet());
        Collection<TriggeredWatch> triggeredWatches = new ArrayList<>(ids.size());

        SearchRequest searchRequest = new SearchRequest(TriggeredWatchStoreField.INDEX_NAME)
                .scroll(scrollTimeout)
                .preference(Preference.LOCAL.toString())
                .source(new SearchSourceBuilder()
                        .size(scrollSize)
                        .sort(SortBuilders.fieldSort("_doc"))
                        .version(true));

        SearchResponse response = null;
        try (ThreadContext.StoredContext ignore = stashWithOrigin(client.threadPool().getThreadContext(), WATCHER_ORIGIN)) {
            response = client.search(searchRequest).actionGet(defaultSearchTimeout);
            logger.debug("trying to find triggered watches for ids {}: found [{}] docs", ids, response.getHits().getTotalHits());
            while (response.getHits().getHits().length != 0) {
                for (SearchHit hit : response.getHits()) {
                    Wid wid = new Wid(hit.getId());
                    if (ids.contains(wid.watchId())) {
                        TriggeredWatch triggeredWatch = triggeredWatchParser.parse(hit.getId(), hit.getVersion(), hit.getSourceRef());
                        triggeredWatches.add(triggeredWatch);
                    }
                }
                SearchScrollRequest request = new SearchScrollRequest(response.getScrollId());
                request.scroll(scrollTimeout);
                response = client.searchScroll(request).actionGet(defaultSearchTimeout);
            }
        } finally {
            if (response != null) {
                try (ThreadContext.StoredContext ignore = stashWithOrigin(client.threadPool().getThreadContext(), WATCHER_ORIGIN)) {
                    ClearScrollRequest clearScrollRequest = new ClearScrollRequest();
                    clearScrollRequest.addScrollId(response.getScrollId());
                    client.clearScroll(clearScrollRequest).actionGet(scrollTimeout);
                }
            }
        }

        return triggeredWatches;
    }
}
