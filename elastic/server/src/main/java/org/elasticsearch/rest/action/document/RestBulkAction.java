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

package org.elasticsearch.rest.action.document;

import org.elasticsearch.action.bulk.BulkRequest;
import org.elasticsearch.action.bulk.BulkShardRequest;
import org.elasticsearch.action.support.ActiveShardCount;
import org.elasticsearch.client.Requests;
import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestStatusToXContentListener;
import org.elasticsearch.search.fetch.subphase.FetchSourceContext;

import java.io.IOException;

import static org.elasticsearch.rest.RestRequest.Method.POST;
import static org.elasticsearch.rest.RestRequest.Method.PUT;

/**
 * <pre>
 * { "index" : { "_index" : "test", "_type" : "type1", "_id" : "1" }
 * { "type1" : { "field1" : "value1" } }
 * { "delete" : { "_index" : "test", "_type" : "type1", "_id" : "2" } }
 * { "create" : { "_index" : "test", "_type" : "type1", "_id" : "1" }
 * { "type1" : { "field1" : "value1" } }
 * </pre>
 */
public class RestBulkAction extends BaseRestHandler {

    private final boolean allowExplicitIndex;

    public RestBulkAction(Settings settings, RestController controller) {
        super(settings);

        controller.registerHandler(POST, "/_bulk", this);
        controller.registerHandler(PUT, "/_bulk", this);
        controller.registerHandler(POST, "/{index}/_bulk", this);
        controller.registerHandler(PUT, "/{index}/_bulk", this);
        controller.registerHandler(POST, "/{index}/{type}/_bulk", this);
        controller.registerHandler(PUT, "/{index}/{type}/_bulk", this);

        this.allowExplicitIndex = MULTI_ALLOW_EXPLICIT_INDEX.get(settings);
    }

    @Override
    public String getName() {
        return "bulk_action";
    }

    @Override
    public RestChannelConsumer prepareRequest(final RestRequest request, final NodeClient client) throws IOException {
        BulkRequest bulkRequest = Requests.bulkRequest();
        String defaultIndex = request.param("index");
        String defaultType = request.param("type");
        final boolean includeTypeName = request.paramAsBoolean("include_type_name", true);
        if (includeTypeName == false && defaultType != null) {
            throw new IllegalArgumentException("You may only use the [include_type_name=false] option with the bulx APIs with the " +
                    "[_bulk] and [{index}/_bulk] endpoints.");
        }
        if (defaultType == null) {
            defaultType = MapperService.SINGLE_MAPPING_NAME;
        }
        String defaultRouting = request.param("routing");
        FetchSourceContext defaultFetchSourceContext = FetchSourceContext.parseFromRestRequest(request);
        String defaultPipeline = request.param("pipeline");
        String waitForActiveShards = request.param("wait_for_active_shards");
        if (waitForActiveShards != null) {
            bulkRequest.waitForActiveShards(ActiveShardCount.parseString(waitForActiveShards));
        }
        bulkRequest.timeout(request.paramAsTime("timeout", BulkShardRequest.DEFAULT_TIMEOUT));
        bulkRequest.setRefreshPolicy(request.param("refresh"));
        bulkRequest.add(request.requiredContent(), defaultIndex, defaultType, defaultRouting,
            defaultFetchSourceContext, defaultPipeline, null, allowExplicitIndex, request.getXContentType());

        return channel -> client.bulk(bulkRequest, new RestStatusToXContentListener<>(channel));
    }

    @Override
    public boolean supportsContentStream() {
        return true;
    }
}
