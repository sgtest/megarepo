/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.security.rest.action.profile;

import org.elasticsearch.client.internal.node.NodeClient;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestToXContentListener;
import org.elasticsearch.xcontent.ConstructingObjectParser;
import org.elasticsearch.xcontent.ParseField;
import org.elasticsearch.xpack.core.security.action.profile.SearchProfilesAction;
import org.elasticsearch.xpack.core.security.action.profile.SearchProfilesRequest;
import org.elasticsearch.xpack.security.rest.action.SecurityBaseRestHandler;

import java.io.IOException;
import java.util.List;
import java.util.Set;

import static org.elasticsearch.rest.RestRequest.Method.GET;
import static org.elasticsearch.rest.RestRequest.Method.POST;
import static org.elasticsearch.xcontent.ConstructingObjectParser.optionalConstructorArg;

public class RestSearchProfilesAction extends SecurityBaseRestHandler {

    static final ConstructingObjectParser<Payload, Void> PARSER = new ConstructingObjectParser<>(
        "search_profile_request_payload",
        a -> new Payload((String) a[0], (Integer) a[1])
    );

    static {
        PARSER.declareString(optionalConstructorArg(), new ParseField("name"));
        PARSER.declareInt(optionalConstructorArg(), new ParseField("size"));
    }

    public RestSearchProfilesAction(Settings settings, XPackLicenseState licenseState) {
        super(settings, licenseState);
    }

    @Override
    public List<Route> routes() {
        return List.of(new Route(GET, "/_security/profile/_search"), new Route(POST, "/_security/profile/_search"));
    }

    @Override
    public String getName() {
        return "xpack_security_search_profile";
    }

    @Override
    protected RestChannelConsumer innerPrepareRequest(RestRequest request, NodeClient client) throws IOException {
        final Set<String> dataKeys = Strings.tokenizeByCommaToSet(request.param("data", null));
        final Payload payload = request.hasContent() ? PARSER.parse(request.contentParser(), null) : new Payload(null, null);

        final SearchProfilesRequest searchProfilesRequest = new SearchProfilesRequest(dataKeys, payload.name(), payload.size());
        return channel -> client.execute(SearchProfilesAction.INSTANCE, searchProfilesRequest, new RestToXContentListener<>(channel));
    }

    record Payload(String name, Integer size) {

        public String name() {
            return name != null ? name : "";
        }

        public Integer size() {
            return size != null ? size : 10;
        }
    }
}
