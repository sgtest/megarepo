/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.watcher.rest.action;

import org.elasticsearch.common.lucene.uid.Versions;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.set.Sets;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.rest.BytesRestResponse;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.RestResponse;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.rest.action.RestBuilderListener;
import org.elasticsearch.xpack.core.security.rest.RestRequestFilter;
import org.elasticsearch.xpack.core.watcher.client.WatcherClient;
import org.elasticsearch.xpack.core.watcher.transport.actions.put.PutWatchRequest;
import org.elasticsearch.xpack.core.watcher.transport.actions.put.PutWatchResponse;
import org.elasticsearch.xpack.watcher.rest.WatcherRestHandler;

import java.io.IOException;
import java.util.Collections;
import java.util.Set;

import static org.elasticsearch.rest.RestRequest.Method.POST;
import static org.elasticsearch.rest.RestRequest.Method.PUT;
import static org.elasticsearch.rest.RestStatus.CREATED;
import static org.elasticsearch.rest.RestStatus.OK;

public class RestPutWatchAction extends WatcherRestHandler implements RestRequestFilter {

    public RestPutWatchAction(Settings settings, RestController controller) {
        super(settings);
        controller.registerHandler(POST, URI_BASE + "/watch/{id}", this);
        controller.registerHandler(PUT, URI_BASE + "/watch/{id}", this);
    }

    @Override
    public String getName() {
        return "xpack_watcher_put_watch_action";
    }

    @Override
    protected RestChannelConsumer doPrepareRequest(final RestRequest request, WatcherClient client) throws IOException {
        PutWatchRequest putWatchRequest =
                new PutWatchRequest(request.param("id"), request.content(), request.getXContentType());
        putWatchRequest.setVersion(request.paramAsLong("version", Versions.MATCH_ANY));
        putWatchRequest.setActive(request.paramAsBoolean("active", putWatchRequest.isActive()));
        return channel -> client.putWatch(putWatchRequest, new RestBuilderListener<PutWatchResponse>(channel) {
            @Override
            public RestResponse buildResponse(PutWatchResponse response, XContentBuilder builder) throws Exception {
                builder.startObject()
                        .field("_id", response.getId())
                        .field("_version", response.getVersion())
                        .field("created", response.isCreated())
                        .endObject();
                RestStatus status = response.isCreated() ? CREATED : OK;
                return new BytesRestResponse(status, builder);
            }
        });
    }

    private static final Set<String> FILTERED_FIELDS = Collections.unmodifiableSet(
            Sets.newHashSet("input.http.request.auth.basic.password", "input.chain.inputs.*.http.request.auth.basic.password",
                    "actions.*.email.attachments.*.reporting.auth.basic.password", "actions.*.webhook.auth.basic.password"));

    @Override
    public Set<String> getFilteredFields() {
        return FILTERED_FIELDS;
    }
}
