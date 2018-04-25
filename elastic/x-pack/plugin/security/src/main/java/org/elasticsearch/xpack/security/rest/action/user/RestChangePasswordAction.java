/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.rest.action.user;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.rest.BytesRestResponse;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.RestResponse;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.rest.action.RestBuilderListener;
import org.elasticsearch.xpack.core.security.SecurityContext;
import org.elasticsearch.xpack.core.security.action.user.ChangePasswordResponse;
import org.elasticsearch.xpack.core.security.client.SecurityClient;
import org.elasticsearch.xpack.core.security.rest.RestRequestFilter;
import org.elasticsearch.xpack.core.security.user.User;
import org.elasticsearch.xpack.security.rest.action.SecurityBaseRestHandler;

import java.io.IOException;
import java.util.Collections;
import java.util.Set;

import static org.elasticsearch.rest.RestRequest.Method.POST;
import static org.elasticsearch.rest.RestRequest.Method.PUT;

public class RestChangePasswordAction extends SecurityBaseRestHandler implements RestRequestFilter {

    private final SecurityContext securityContext;

    public RestChangePasswordAction(Settings settings, RestController controller, SecurityContext securityContext,
                                    XPackLicenseState licenseState) {
        super(settings, licenseState);
        this.securityContext = securityContext;
        controller.registerHandler(POST, "/_xpack/security/user/{username}/_password", this);
        controller.registerHandler(PUT, "/_xpack/security/user/{username}/_password", this);
        controller.registerHandler(POST, "/_xpack/security/user/_password", this);
        controller.registerHandler(PUT, "/_xpack/security/user/_password", this);
    }

    @Override
    public String getName() {
        return "xpack_security_change_password_action";
    }

    @Override
    public RestChannelConsumer innerPrepareRequest(RestRequest request, NodeClient client) throws IOException {
        final User user = securityContext.getUser();
        final String username;
        if (request.param("username") == null) {
            username = user.principal();
        } else {
            username = request.param("username");
        }

        final String refresh = request.param("refresh");
        return channel ->
                new SecurityClient(client)
                        .prepareChangePassword(username, request.requiredContent(), request.getXContentType())
                        .setRefreshPolicy(refresh)
                        .execute(new RestBuilderListener<ChangePasswordResponse>(channel) {
                            @Override
                            public RestResponse buildResponse(ChangePasswordResponse changePasswordResponse,
                                                              XContentBuilder builder) throws Exception {
                                return new BytesRestResponse(RestStatus.OK, builder.startObject().endObject());
                            }
                        });
    }

    private static final Set<String> FILTERED_FIELDS = Collections.singleton("password");

    @Override
    public Set<String> getFilteredFields() {
        return FILTERED_FIELDS;
    }
}
