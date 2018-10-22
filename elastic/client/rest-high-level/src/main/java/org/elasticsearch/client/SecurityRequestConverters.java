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

package org.elasticsearch.client;

import org.apache.http.client.methods.HttpDelete;
import org.apache.http.client.methods.HttpPost;
import org.apache.http.client.methods.HttpPut;
import org.elasticsearch.client.security.DeleteRoleMappingRequest;
import org.elasticsearch.client.security.DeleteRoleRequest;
import org.elasticsearch.client.security.PutRoleMappingRequest;
import org.elasticsearch.client.security.DisableUserRequest;
import org.elasticsearch.client.security.EnableUserRequest;
import org.elasticsearch.client.security.ChangePasswordRequest;
import org.elasticsearch.client.security.PutUserRequest;
import org.elasticsearch.client.security.SetUserEnabledRequest;

import java.io.IOException;

import static org.elasticsearch.client.RequestConverters.REQUEST_BODY_CONTENT_TYPE;
import static org.elasticsearch.client.RequestConverters.createEntity;

final class SecurityRequestConverters {

    private SecurityRequestConverters() {}

    static Request changePassword(ChangePasswordRequest changePasswordRequest) throws IOException {
        String endpoint = new RequestConverters.EndpointBuilder()
            .addPathPartAsIs("_xpack/security/user")
            .addPathPart(changePasswordRequest.getUsername())
            .addPathPartAsIs("_password")
            .build();
        Request request = new Request(HttpPost.METHOD_NAME, endpoint);
        request.setEntity(createEntity(changePasswordRequest, REQUEST_BODY_CONTENT_TYPE));
        RequestConverters.Params params = new RequestConverters.Params(request);
        params.withRefreshPolicy(changePasswordRequest.getRefreshPolicy());
        return request;
    }

    static Request putUser(PutUserRequest putUserRequest) throws IOException {
        String endpoint = new RequestConverters.EndpointBuilder()
            .addPathPartAsIs("_xpack/security/user")
            .addPathPart(putUserRequest.getUsername())
            .build();
        Request request = new Request(HttpPut.METHOD_NAME, endpoint);
        request.setEntity(createEntity(putUserRequest, REQUEST_BODY_CONTENT_TYPE));
        RequestConverters.Params params = new RequestConverters.Params(request);
        params.withRefreshPolicy(putUserRequest.getRefreshPolicy());
        return request;
    }

    static Request putRoleMapping(final PutRoleMappingRequest putRoleMappingRequest) throws IOException {
        final String endpoint = new RequestConverters.EndpointBuilder()
            .addPathPartAsIs("_xpack/security/role_mapping")
            .addPathPart(putRoleMappingRequest.getName())
            .build();
        final Request request = new Request(HttpPut.METHOD_NAME, endpoint);
        request.setEntity(createEntity(putRoleMappingRequest, REQUEST_BODY_CONTENT_TYPE));
        final RequestConverters.Params params = new RequestConverters.Params(request);
        params.withRefreshPolicy(putRoleMappingRequest.getRefreshPolicy());
        return request;
    }

    static Request enableUser(EnableUserRequest enableUserRequest) {
        return setUserEnabled(enableUserRequest);
    }

    static Request disableUser(DisableUserRequest disableUserRequest) {
        return setUserEnabled(disableUserRequest);
    }

    private static Request setUserEnabled(SetUserEnabledRequest setUserEnabledRequest) {
        String endpoint = new RequestConverters.EndpointBuilder()
            .addPathPartAsIs("_xpack/security/user")
            .addPathPart(setUserEnabledRequest.getUsername())
            .addPathPart(setUserEnabledRequest.isEnabled() ? "_enable" : "_disable")
            .build();
        Request request = new Request(HttpPut.METHOD_NAME, endpoint);
        RequestConverters.Params params = new RequestConverters.Params(request);
        params.withRefreshPolicy(setUserEnabledRequest.getRefreshPolicy());
        return request;
    }

    static Request deleteRoleMapping(DeleteRoleMappingRequest deleteRoleMappingRequest) {
        final String endpoint = new RequestConverters.EndpointBuilder()
            .addPathPartAsIs("_xpack/security/role_mapping")
            .addPathPart(deleteRoleMappingRequest.getName())
            .build();
        final Request request = new Request(HttpDelete.METHOD_NAME, endpoint);
        final RequestConverters.Params params = new RequestConverters.Params(request);
        params.withRefreshPolicy(deleteRoleMappingRequest.getRefreshPolicy());
        return request;
    }

    static Request deleteRole(DeleteRoleRequest deleteRoleRequest) {
        String endpoint = new RequestConverters.EndpointBuilder()
            .addPathPartAsIs("_xpack/security/role")
            .addPathPart(deleteRoleRequest.getName())
            .build();
        Request request = new Request(HttpDelete.METHOD_NAME, endpoint);
        RequestConverters.Params params = new RequestConverters.Params(request);
        params.withRefreshPolicy(deleteRoleRequest.getRefreshPolicy());
        return request;
    }
}
