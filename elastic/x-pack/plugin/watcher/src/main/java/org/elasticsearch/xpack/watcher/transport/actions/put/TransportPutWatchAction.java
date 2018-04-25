/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.watcher.transport.actions.put;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.DocWriteResponse;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.action.update.UpdateRequest;
import org.elasticsearch.action.update.UpdateResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.watcher.support.xcontent.WatcherParams;
import org.elasticsearch.xpack.core.watcher.transport.actions.put.PutWatchAction;
import org.elasticsearch.xpack.core.watcher.transport.actions.put.PutWatchRequest;
import org.elasticsearch.xpack.core.watcher.transport.actions.put.PutWatchResponse;
import org.elasticsearch.xpack.core.watcher.watch.Watch;
import org.elasticsearch.xpack.watcher.Watcher;
import org.elasticsearch.xpack.watcher.transport.actions.WatcherTransportAction;
import org.elasticsearch.xpack.watcher.watch.WatchParser;
import org.joda.time.DateTime;

import java.time.Clock;
import java.util.Map;
import java.util.stream.Collectors;

import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.elasticsearch.xpack.core.ClientHelper.WATCHER_ORIGIN;
import static org.elasticsearch.xpack.core.ClientHelper.executeAsyncWithOrigin;
import static org.joda.time.DateTimeZone.UTC;

/**
 * This action internally has two modes of operation - an insert and an update mode
 *
 * The insert mode will simply put a watch and that is it.
 * The update mode is a bit more complex and uses versioning. First this prevents the
 * last-write-wins issue, when two users store the same watch. This could happen due
 * to UI users. To prevent this a version is required to trigger the update mode.
 * This mode has been mainly introduced to deal with updates, where the user does not
 * need to provide secrets like passwords for basic auth or sending emails. If this
 * is an update, the watch will not parse the secrets coming in, and the resulting JSON
 * to store the new watch will not contain a password allowing for updates.
 *
 * Internally both requests result in an update call, albeit with different parameters and
 * use of versioning as well as setting the docAsUpsert boolean.
 */
public class TransportPutWatchAction extends WatcherTransportAction<PutWatchRequest, PutWatchResponse> {

    private final Clock clock;
    private final WatchParser parser;
    private final Client client;
    private static final ToXContent.Params DEFAULT_PARAMS =
            WatcherParams.builder().hideSecrets(false).hideHeaders(false).includeStatus(true).build();

    @Inject
    public TransportPutWatchAction(Settings settings, TransportService transportService, ThreadPool threadPool, ActionFilters actionFilters,
                                   IndexNameExpressionResolver indexNameExpressionResolver, Clock clock, XPackLicenseState licenseState,
                                   WatchParser parser, Client client) {
        super(settings, PutWatchAction.NAME, transportService, threadPool, actionFilters, indexNameExpressionResolver,
                licenseState, PutWatchRequest::new);
        this.clock = clock;
        this.parser = parser;
        this.client = client;
    }

    @Override
    protected void doExecute(PutWatchRequest request, ActionListener<PutWatchResponse> listener) {
        try {
            DateTime now = new DateTime(clock.millis(), UTC);
            boolean isUpdate = request.getVersion() > 0;
            Watch watch = parser.parseWithSecrets(request.getId(), false, request.getSource(), now, request.xContentType(), isUpdate);
            watch.setState(request.isActive(), now);

            // ensure we only filter for the allowed headers
            Map<String, String> filteredHeaders = threadPool.getThreadContext().getHeaders().entrySet().stream()
                    .filter(e -> Watcher.HEADER_FILTERS.contains(e.getKey()))
                    .collect(Collectors.toMap(Map.Entry::getKey, Map.Entry::getValue));
            watch.status().setHeaders(filteredHeaders);

            try (XContentBuilder builder = jsonBuilder()) {
                watch.toXContent(builder, DEFAULT_PARAMS);

                UpdateRequest updateRequest = new UpdateRequest(Watch.INDEX, Watch.DOC_TYPE, request.getId());
                updateRequest.docAsUpsert(isUpdate == false);
                updateRequest.version(request.getVersion());
                updateRequest.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);
                updateRequest.doc(builder);

                executeAsyncWithOrigin(client.threadPool().getThreadContext(), WATCHER_ORIGIN, updateRequest,
                        ActionListener.<UpdateResponse>wrap(response -> {
                            boolean created = response.getResult() == DocWriteResponse.Result.CREATED;
                            listener.onResponse(new PutWatchResponse(response.getId(), response.getVersion(), created));
                        }, listener::onFailure),
                        client::update);
            }
        } catch (Exception e) {
            listener.onFailure(e);
        }
    }
}
