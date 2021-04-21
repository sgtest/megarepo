/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.rest.action.admin.indices;

import org.elasticsearch.action.admin.indices.template.get.GetIndexTemplatesRequest;
import org.elasticsearch.action.admin.indices.template.get.GetIndexTemplatesResponse;
import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.RestApiVersion;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.logging.DeprecationLogger;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.set.Sets;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.rest.action.RestToXContentListener;

import java.io.IOException;
import java.util.Collections;
import java.util.List;
import java.util.Set;

import static org.elasticsearch.rest.RestRequest.Method.GET;
import static org.elasticsearch.rest.RestRequest.Method.HEAD;
import static org.elasticsearch.rest.RestStatus.NOT_FOUND;
import static org.elasticsearch.rest.RestStatus.OK;

/**
 * The REST handler for get template and head template APIs.
 */
public class RestGetIndexTemplateAction extends BaseRestHandler {
    private static final Set<String> COMPATIBLE_RESPONSE_PARAMS = Collections.unmodifiableSet(Sets.union(
        Collections.singleton(INCLUDE_TYPE_NAME_PARAMETER), Settings.FORMAT_PARAMS));
    private static final DeprecationLogger deprecationLogger =  DeprecationLogger.getLogger(RestGetIndexTemplateAction.class);
    public static final String TYPES_DEPRECATION_MESSAGE = "[types removal] Using include_type_name in get " +
        "index template requests is deprecated. The parameter will be removed in the next major version.";

    @Override
    public List<Route> routes() {
        return List.of(
            new Route(GET, "/_template"),
            new Route(GET, "/_template/{name}"),
            new Route(HEAD, "/_template/{name}"));
    }

    @Override
    public String getName() {
        return "get_index_template_action";
    }

    @Override
    public RestChannelConsumer prepareRequest(final RestRequest request, final NodeClient client) throws IOException {
        if (request.getRestApiVersion() == RestApiVersion.V_7 && request.hasParam(INCLUDE_TYPE_NAME_PARAMETER)) {
            deprecationLogger.compatibleApiWarning("get_index_template_include_type_name", TYPES_DEPRECATION_MESSAGE);
        }
        final String[] names = Strings.splitStringByCommaToArray(request.param("name"));

        final GetIndexTemplatesRequest getIndexTemplatesRequest = new GetIndexTemplatesRequest(names);

        getIndexTemplatesRequest.local(request.paramAsBoolean("local", getIndexTemplatesRequest.local()));
        getIndexTemplatesRequest.masterNodeTimeout(request.paramAsTime("master_timeout", getIndexTemplatesRequest.masterNodeTimeout()));

        final boolean implicitAll = getIndexTemplatesRequest.names().length == 0;

        return channel ->
                client.admin()
                        .indices()
                        .getTemplates(getIndexTemplatesRequest, new RestToXContentListener<>(channel) {
                            @Override
                            protected RestStatus getStatus(final GetIndexTemplatesResponse response) {
                                final boolean templateExists = response.getIndexTemplates().isEmpty() == false;
                                return (templateExists || implicitAll) ? OK : NOT_FOUND;
                            }
                        });
    }

    @Override
    protected Set<String> responseParams() {
        return Settings.FORMAT_PARAMS;
    }

    @Override
    protected Set<String> responseParams(RestApiVersion restApiVersion) {
        if(restApiVersion == RestApiVersion.V_7){
            return COMPATIBLE_RESPONSE_PARAMS;
        } else {
            return responseParams();
        }
    }
}
