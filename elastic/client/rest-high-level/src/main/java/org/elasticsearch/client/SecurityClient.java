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

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.client.security.ChangePasswordRequest;
import org.elasticsearch.client.security.ClearRolesCacheRequest;
import org.elasticsearch.client.security.ClearRolesCacheResponse;
import org.elasticsearch.client.security.CreateTokenRequest;
import org.elasticsearch.client.security.CreateTokenResponse;
import org.elasticsearch.client.security.DeleteRoleMappingRequest;
import org.elasticsearch.client.security.DeleteRoleMappingResponse;
import org.elasticsearch.client.security.DeleteRoleRequest;
import org.elasticsearch.client.security.DeleteRoleResponse;
import org.elasticsearch.client.security.DisableUserRequest;
import org.elasticsearch.client.security.EmptyResponse;
import org.elasticsearch.client.security.EnableUserRequest;
import org.elasticsearch.client.security.GetRoleMappingsRequest;
import org.elasticsearch.client.security.GetRoleMappingsResponse;
import org.elasticsearch.client.security.GetSslCertificatesRequest;
import org.elasticsearch.client.security.GetSslCertificatesResponse;
import org.elasticsearch.client.security.PutRoleMappingRequest;
import org.elasticsearch.client.security.PutRoleMappingResponse;
import org.elasticsearch.client.security.PutUserRequest;
import org.elasticsearch.client.security.PutUserResponse;

import java.io.IOException;

import static java.util.Collections.emptySet;
import static java.util.Collections.singleton;

/**
 * A wrapper for the {@link RestHighLevelClient} that provides methods for accessing the Security APIs.
 * <p>
 * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api.html">Security APIs on elastic.co</a>
 */
public final class SecurityClient {

    private final RestHighLevelClient restHighLevelClient;

    SecurityClient(RestHighLevelClient restHighLevelClient) {
        this.restHighLevelClient = restHighLevelClient;
    }

    /**
     * Create/update a user in the native realm synchronously.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-users.html">
     * the docs</a> for more.
     *
     * @param request the request with the user's information
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the put user call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public PutUserResponse putUser(PutUserRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(request, SecurityRequestConverters::putUser, options,
            PutUserResponse::fromXContent, emptySet());
    }

    /**
     * Asynchronously create/update a user in the native realm.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-users.html">
     * the docs</a> for more.
     *
     * @param request  the request with the user's information
     * @param options  the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @param listener the listener to be notified upon request completion
     */
    public void putUserAsync(PutUserRequest request, RequestOptions options, ActionListener<PutUserResponse> listener) {
        restHighLevelClient.performRequestAsyncAndParseEntity(request, SecurityRequestConverters::putUser, options,
            PutUserResponse::fromXContent, listener, emptySet());
    }

    /**
     * Create/Update a role mapping.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-put-role-mapping.html">
     * the docs</a> for more.
     * @param request the request with the role mapping information
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the put role mapping call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public PutRoleMappingResponse putRoleMapping(final PutRoleMappingRequest request, final RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(request, SecurityRequestConverters::putRoleMapping, options,
                PutRoleMappingResponse::fromXContent, emptySet());
    }

    /**
     * Asynchronously create/update a role mapping.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-put-role-mapping.html">
     * the docs</a> for more.
     * @param request the request with the role mapping information
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @param listener the listener to be notified upon request completion
     */
    public void putRoleMappingAsync(final PutRoleMappingRequest request, final RequestOptions options,
            final ActionListener<PutRoleMappingResponse> listener) {
        restHighLevelClient.performRequestAsyncAndParseEntity(request, SecurityRequestConverters::putRoleMapping, options,
                PutRoleMappingResponse::fromXContent, listener, emptySet());
    }

    /**
     * Synchronously get role mapping(s).
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-get-role-mapping.html">
     * the docs</a> for more.
     *
     * @param request {@link GetRoleMappingsRequest} with role mapping name(s).
     * If no role mapping name is provided then retrieves all role mappings.
     * @param options the request options (e.g. headers), use
     * {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the get role mapping call
     * @throws IOException in case there is a problem sending the request or
     * parsing back the response
     */
    public GetRoleMappingsResponse getRoleMappings(final GetRoleMappingsRequest request, final RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(request, SecurityRequestConverters::getRoleMappings,
            options, GetRoleMappingsResponse::fromXContent, emptySet());
    }

    /**
     * Asynchronously get role mapping(s).
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-get-role-mapping.html">
     * the docs</a> for more.
     *
     * @param request {@link GetRoleMappingsRequest} with role mapping name(s).
     * If no role mapping name is provided then retrieves all role mappings.
     * @param options  the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @param listener the listener to be notified upon request completion
     */
    public void getRoleMappingsAsync(final GetRoleMappingsRequest request, final RequestOptions options,
            final ActionListener<GetRoleMappingsResponse> listener) {
        restHighLevelClient.performRequestAsyncAndParseEntity(request, SecurityRequestConverters::getRoleMappings,
                options, GetRoleMappingsResponse::fromXContent, listener, emptySet());
    }

    /**
     * Enable a native realm or built-in user synchronously.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-enable-user.html">
     * the docs</a> for more.
     *
     * @param request the request with the user to enable
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the enable user call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public EmptyResponse enableUser(EnableUserRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(request, SecurityRequestConverters::enableUser, options,
            EmptyResponse::fromXContent, emptySet());
    }

    /**
     * Enable a native realm or built-in user asynchronously.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-enable-user.html">
     * the docs</a> for more.
     *
     * @param request  the request with the user to enable
     * @param options  the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @param listener the listener to be notified upon request completion
     */
    public void enableUserAsync(EnableUserRequest request, RequestOptions options,
                                ActionListener<EmptyResponse> listener) {
        restHighLevelClient.performRequestAsyncAndParseEntity(request, SecurityRequestConverters::enableUser, options,
            EmptyResponse::fromXContent, listener, emptySet());
    }

    /**
     * Disable a native realm or built-in user synchronously.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-disable-user.html">
     * the docs</a> for more.
     *
     * @param request the request with the user to disable
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the enable user call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public EmptyResponse disableUser(DisableUserRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(request, SecurityRequestConverters::disableUser, options,
            EmptyResponse::fromXContent, emptySet());
    }

    /**
     * Disable a native realm or built-in user asynchronously.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-disable-user.html">
     * the docs</a> for more.
     *
     * @param request  the request with the user to disable
     * @param options  the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @param listener the listener to be notified upon request completion
     */
    public void disableUserAsync(DisableUserRequest request, RequestOptions options,
                                 ActionListener<EmptyResponse> listener) {
        restHighLevelClient.performRequestAsyncAndParseEntity(request, SecurityRequestConverters::disableUser, options,
            EmptyResponse::fromXContent, listener, emptySet());
    }

    /**
     * Clears the native roles cache for a set of roles.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-clear-role-cache.html">
     * the docs</a> for more.
     *
     * @param request the request with the roles for which the cache should be cleared.
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the enable user call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public ClearRolesCacheResponse clearRolesCache(ClearRolesCacheRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(request, SecurityRequestConverters::clearRolesCache, options,
            ClearRolesCacheResponse::fromXContent, emptySet());
    }

    /**
     * Clears the native roles cache for a set of roles asynchronously.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-clear-role-cache.html">
     * the docs</a> for more.
     *
     * @param request  the request with the roles for which the cache should be cleared.
     * @param options  the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @param listener the listener to be notified upon request completion
     */
    public void clearRolesCacheAsync(ClearRolesCacheRequest request, RequestOptions options,
                                     ActionListener<ClearRolesCacheResponse> listener) {
        restHighLevelClient.performRequestAsyncAndParseEntity(request, SecurityRequestConverters::clearRolesCache, options,
            ClearRolesCacheResponse::fromXContent, listener, emptySet());
    }

    /**
     * Synchronously retrieve the X.509 certificates that are used to encrypt communications in an Elasticsearch cluster.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-ssl.html">
     * the docs</a> for more.
     *
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the get certificates call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public GetSslCertificatesResponse getSslCertificates(RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(GetSslCertificatesRequest.INSTANCE, GetSslCertificatesRequest::getRequest,
            options, GetSslCertificatesResponse::fromXContent, emptySet());
    }

    /**
     * Asynchronously retrieve the X.509 certificates that are used to encrypt communications in an Elasticsearch cluster.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-ssl.html">
     * the docs</a> for more.
     *
     * @param options  the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @param listener the listener to be notified upon request completion
     */
    public void getSslCertificatesAsync(RequestOptions options, ActionListener<GetSslCertificatesResponse> listener) {
        restHighLevelClient.performRequestAsyncAndParseEntity(GetSslCertificatesRequest.INSTANCE, GetSslCertificatesRequest::getRequest,
            options, GetSslCertificatesResponse::fromXContent, listener, emptySet());
    }

    /**
     * Change the password of a user of a native realm or built-in user synchronously.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-change-password.html">
     * the docs</a> for more.
     *
     * @param request the request with the user's new password
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the change user password call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public EmptyResponse changePassword(ChangePasswordRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(request, SecurityRequestConverters::changePassword, options,
            EmptyResponse::fromXContent, emptySet());
    }

    /**
     * Change the password of a user of a native realm or built-in user asynchronously.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-change-password.html">
     * the docs</a> for more.
     *
     * @param request  the request with the user's new password
     * @param options  the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @param listener the listener to be notified upon request completion
     */
    public void changePasswordAsync(ChangePasswordRequest request, RequestOptions options,
                                    ActionListener<EmptyResponse> listener) {
        restHighLevelClient.performRequestAsyncAndParseEntity(request, SecurityRequestConverters::changePassword, options,
            EmptyResponse::fromXContent, listener, emptySet());
    }

    /**
     * Delete a role mapping.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-delete-role-mapping.html">
     * the docs</a> for more.
     * @param request the request with the role mapping name to be deleted.
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the delete role mapping call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public DeleteRoleMappingResponse deleteRoleMapping(DeleteRoleMappingRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(request, SecurityRequestConverters::deleteRoleMapping, options,
                DeleteRoleMappingResponse::fromXContent, emptySet());
    }

    /**
     * Asynchronously delete a role mapping.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-delete-role-mapping.html">
     * the docs</a> for more.
     * @param request the request with the role mapping name to be deleted.
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @param listener the listener to be notified upon request completion
     */
    public void deleteRoleMappingAsync(DeleteRoleMappingRequest request, RequestOptions options,
            ActionListener<DeleteRoleMappingResponse> listener) {
        restHighLevelClient.performRequestAsyncAndParseEntity(request, SecurityRequestConverters::deleteRoleMapping, options,
                DeleteRoleMappingResponse::fromXContent, listener, emptySet());
    }

    /**
     * Removes role from the native realm.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-delete-role.html">
     * the docs</a> for more.
     * @param request the request with the role to delete
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the delete role call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public DeleteRoleResponse deleteRole(DeleteRoleRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(request, SecurityRequestConverters::deleteRole, options,
            DeleteRoleResponse::fromXContent, singleton(404));
    }

    /**
     * Removes role from the native realm.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-delete-role.html">
     * the docs</a> for more.
     * @param request the request with the role to delete
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @param listener the listener to be notified upon request completion
     */
    public void deleteRoleAsync(DeleteRoleRequest request, RequestOptions options, ActionListener<DeleteRoleResponse> listener) {
        restHighLevelClient.performRequestAsyncAndParseEntity(request, SecurityRequestConverters::deleteRole, options,
            DeleteRoleResponse::fromXContent, listener, singleton(404));
    }

    /**
     * Creates an OAuth2 token.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-get-token.html">
     * the docs</a> for more.
     *
     * @param request the request for the token
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the create token call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public CreateTokenResponse createToken(CreateTokenRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(request, SecurityRequestConverters::createToken, options,
            CreateTokenResponse::fromXContent, emptySet());
    }

    /**
     * Asynchronously creates an OAuth2 token.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-get-token.html">
     * the docs</a> for more.
     *
     * @param request the request for the token
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @param listener the listener to be notified upon request completion
     */
    public void createTokenAsync(CreateTokenRequest request, RequestOptions options, ActionListener<CreateTokenResponse> listener) {
        restHighLevelClient.performRequestAsyncAndParseEntity(request, SecurityRequestConverters::createToken, options,
            CreateTokenResponse::fromXContent, listener, emptySet());
    }
}
