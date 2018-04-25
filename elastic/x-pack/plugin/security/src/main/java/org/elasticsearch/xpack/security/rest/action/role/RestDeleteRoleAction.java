/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.rest.action.role;

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
import org.elasticsearch.xpack.core.security.action.role.DeleteRoleResponse;
import org.elasticsearch.xpack.core.security.client.SecurityClient;
import org.elasticsearch.xpack.security.rest.action.SecurityBaseRestHandler;

import java.io.IOException;

import static org.elasticsearch.rest.RestRequest.Method.DELETE;

/**
 * Rest endpoint to delete a Role from the security index
 */
public class RestDeleteRoleAction extends SecurityBaseRestHandler {

    public RestDeleteRoleAction(Settings settings, RestController controller, XPackLicenseState licenseState) {
        super(settings, licenseState);
        controller.registerHandler(DELETE, "/_xpack/security/role/{name}", this);
    }

    @Override
    public String getName() {
        return "xpack_security_delete_role_action";
    }

    @Override
    public RestChannelConsumer innerPrepareRequest(RestRequest request, NodeClient client) throws IOException {
        final String name = request.param("name");
        final String refresh = request.param("refresh");

        return channel -> new SecurityClient(client).prepareDeleteRole(name)
                .setRefreshPolicy(refresh)
                .execute(new RestBuilderListener<DeleteRoleResponse>(channel) {
                    @Override
                    public RestResponse buildResponse(DeleteRoleResponse response, XContentBuilder builder) throws Exception {
                        return new BytesRestResponse(
                                response.found() ? RestStatus.OK : RestStatus.NOT_FOUND,
                                builder.startObject().field("found", response.found()).endObject());
                    }
                });
    }
}
