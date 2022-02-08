/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.client;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.client.security.AuthenticateRequest;
import org.elasticsearch.client.security.AuthenticateResponse;
import org.elasticsearch.client.security.ChangePasswordRequest;
import org.elasticsearch.client.security.ClearRealmCacheRequest;
import org.elasticsearch.client.security.ClearRealmCacheResponse;
import org.elasticsearch.client.security.CreateTokenRequest;
import org.elasticsearch.client.security.CreateTokenResponse;
import org.elasticsearch.client.security.DelegatePkiAuthenticationRequest;
import org.elasticsearch.client.security.DelegatePkiAuthenticationResponse;
import org.elasticsearch.client.security.DeleteRoleMappingRequest;
import org.elasticsearch.client.security.DeleteRoleMappingResponse;
import org.elasticsearch.client.security.DeleteRoleRequest;
import org.elasticsearch.client.security.DeleteRoleResponse;
import org.elasticsearch.client.security.DeleteUserRequest;
import org.elasticsearch.client.security.DeleteUserResponse;
import org.elasticsearch.client.security.DisableUserRequest;
import org.elasticsearch.client.security.EnableUserRequest;
import org.elasticsearch.client.security.GetApiKeyRequest;
import org.elasticsearch.client.security.GetApiKeyResponse;
import org.elasticsearch.client.security.GetRolesRequest;
import org.elasticsearch.client.security.GetRolesResponse;
import org.elasticsearch.client.security.InvalidateApiKeyRequest;
import org.elasticsearch.client.security.InvalidateApiKeyResponse;
import org.elasticsearch.client.security.InvalidateTokenRequest;
import org.elasticsearch.client.security.InvalidateTokenResponse;
import org.elasticsearch.client.security.PutPrivilegesRequest;
import org.elasticsearch.client.security.PutPrivilegesResponse;
import org.elasticsearch.client.security.PutRoleMappingRequest;
import org.elasticsearch.client.security.PutRoleMappingResponse;
import org.elasticsearch.client.security.PutRoleRequest;
import org.elasticsearch.client.security.PutRoleResponse;
import org.elasticsearch.client.security.PutUserRequest;
import org.elasticsearch.client.security.PutUserResponse;

import java.io.IOException;

import static java.util.Collections.emptySet;
import static java.util.Collections.singleton;

/**
 * A wrapper for the {@link RestHighLevelClient} that provides methods for accessing the Security APIs.
 * <p>
 * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api.html">Security APIs on elastic.co</a>
 *
 * @deprecated The High Level Rest Client is deprecated in favor of the
 * <a href="https://www.elastic.co/guide/en/elasticsearch/client/java-api-client/current/introduction.html">
 * Elasticsearch Java API Client</a>
 */
@Deprecated(since = "7.16.0", forRemoval = true)
@SuppressWarnings("removal")
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
        return restHighLevelClient.performRequestAndParseEntity(
            request,
            SecurityRequestConverters::putUser,
            options,
            PutUserResponse::fromXContent,
            emptySet()
        );
    }

    /**
     * Removes user from the native realm synchronously.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-delete-user.html">
     * the docs</a> for more.
     * @param request the request with the user to delete
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the delete user call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public DeleteUserResponse deleteUser(DeleteUserRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(
            request,
            SecurityRequestConverters::deleteUser,
            options,
            DeleteUserResponse::fromXContent,
            singleton(404)
        );
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
        return restHighLevelClient.performRequestAndParseEntity(
            request,
            SecurityRequestConverters::putRoleMapping,
            options,
            PutRoleMappingResponse::fromXContent,
            emptySet()
        );
    }

    /**
     * Enable a native realm or built-in user synchronously.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-enable-user.html">
     * the docs</a> for more.
     *
     * @param request the request with the user to enable
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return {@code true} if the request succeeded (the user is enabled)
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public boolean enableUser(EnableUserRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequest(
            request,
            SecurityRequestConverters::enableUser,
            options,
            RestHighLevelClient::convertExistsResponse,
            emptySet()
        );
    }

    /**
     * Disable a native realm or built-in user synchronously.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-disable-user.html">
     * the docs</a> for more.
     *
     * @param request the request with the user to disable
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return {@code true} if the request succeeded (the user is disabled)
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public boolean disableUser(DisableUserRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequest(
            request,
            SecurityRequestConverters::disableUser,
            options,
            RestHighLevelClient::convertExistsResponse,
            emptySet()
        );
    }

    /**
     * Disable a native realm or built-in user asynchronously.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-disable-user.html">
     * the docs</a> for more.
     *
     * @param request  the request with the user to disable
     * @param options  the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @param listener the listener to be notified upon request completion
     * @return cancellable that may be used to cancel the request
     */
    public Cancellable disableUserAsync(DisableUserRequest request, RequestOptions options, ActionListener<Boolean> listener) {
        return restHighLevelClient.performRequestAsync(
            request,
            SecurityRequestConverters::disableUser,
            options,
            RestHighLevelClient::convertExistsResponse,
            listener,
            emptySet()
        );
    }

    /**
     * Authenticate the current user and return all the information about the authenticated user.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-authenticate.html">
     * the docs</a> for more.
     *
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the responsee from the authenticate user call
     */
    public AuthenticateResponse authenticate(RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(
            AuthenticateRequest.INSTANCE,
            AuthenticateRequest::getRequest,
            options,
            AuthenticateResponse::fromXContent,
            emptySet()
        );
    }

    /**
     * Clears the cache in one or more realms.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-clear-cache.html">
     * the docs</a> for more.
     *
     * @param request the request with the realm names and usernames to clear the cache for
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the clear realm cache call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public ClearRealmCacheResponse clearRealmCache(ClearRealmCacheRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(
            request,
            SecurityRequestConverters::clearRealmCache,
            options,
            ClearRealmCacheResponse::fromXContent,
            emptySet()
        );
    }

    /**
     * Change the password of a user of a native realm or built-in user synchronously.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-change-password.html">
     * the docs</a> for more.
     *
     * @param request the request with the user's new password
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return {@code true} if the request succeeded (the new password was set)
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public boolean changePassword(ChangePasswordRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequest(
            request,
            SecurityRequestConverters::changePassword,
            options,
            RestHighLevelClient::convertExistsResponse,
            emptySet()
        );
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
        return restHighLevelClient.performRequestAndParseEntity(
            request,
            SecurityRequestConverters::deleteRoleMapping,
            options,
            DeleteRoleMappingResponse::fromXContent,
            emptySet()
        );
    }

    /**
     * Retrieves roles from the native roles store.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-get-role.html">
     * the docs</a> for more.
     *
     * @param request the request with the roles to get
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the get roles call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public GetRolesResponse getRoles(final GetRolesRequest request, final RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(
            request,
            SecurityRequestConverters::getRoles,
            options,
            GetRolesResponse::fromXContent,
            emptySet()
        );
    }

    /**
     * Create or update a role in the native roles store.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-put-role.html">
     * the docs</a> for more.
     *
     * @param request the request containing the role to create or update
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the put role call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public PutRoleResponse putRole(final PutRoleRequest request, final RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(
            request,
            SecurityRequestConverters::putRole,
            options,
            PutRoleResponse::fromXContent,
            emptySet()
        );
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
        return restHighLevelClient.performRequestAndParseEntity(
            request,
            SecurityRequestConverters::deleteRole,
            options,
            DeleteRoleResponse::fromXContent,
            singleton(404)
        );
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
        return restHighLevelClient.performRequestAndParseEntity(
            request,
            SecurityRequestConverters::createToken,
            options,
            CreateTokenResponse::fromXContent,
            emptySet()
        );
    }

    /**
     * Invalidates an OAuth2 token.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-invalidate-token.html">
     * the docs</a> for more.
     *
     * @param request the request to invalidate the token
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the create token call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public InvalidateTokenResponse invalidateToken(InvalidateTokenRequest request, RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(
            request,
            SecurityRequestConverters::invalidateToken,
            options,
            InvalidateTokenResponse::fromXContent,
            singleton(404)
        );
    }

    /**
     * Create or update application privileges.
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-put-privileges.html">
     * the docs</a> for more.
     *
     * @param request the request to create or update application privileges
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the create or update application privileges call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public PutPrivilegesResponse putPrivileges(final PutPrivilegesRequest request, final RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(
            request,
            SecurityRequestConverters::putPrivileges,
            options,
            PutPrivilegesResponse::fromXContent,
            emptySet()
        );
    }

    /**
     * Retrieve API Key(s) information.<br>
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-get-api-key.html">
     * the docs</a> for more.
     *
     * @param request the request to retrieve API key(s)
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the get API key call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public GetApiKeyResponse getApiKey(final GetApiKeyRequest request, final RequestOptions options) throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(
            request,
            SecurityRequestConverters::getApiKey,
            options,
            GetApiKeyResponse::fromXContent,
            emptySet()
        );
    }

    /**
     * Invalidate API Key(s).<br>
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-invalidate-api-key.html">
     * the docs</a> for more.
     *
     * @param request the request to invalidate API key(s)
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the invalidate API key call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public InvalidateApiKeyResponse invalidateApiKey(final InvalidateApiKeyRequest request, final RequestOptions options)
        throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(
            request,
            SecurityRequestConverters::invalidateApiKey,
            options,
            InvalidateApiKeyResponse::fromXContent,
            emptySet()
        );
    }

    /**
     * Get an Elasticsearch access token from an {@code X509Certificate} chain. The certificate chain is that of the client from a mutually
     * authenticated TLS session, and it is validated by the PKI realms with {@code delegation.enabled} toggled to {@code true}.<br>
     * See <a href="https://www.elastic.co/guide/en/elasticsearch/reference/current/security-api-delegate-pki-authentication.html"> the
     * docs</a> for more details.
     *
     * @param request the request containing the certificate chain
     * @param options the request options (e.g. headers), use {@link RequestOptions#DEFAULT} if nothing needs to be customized
     * @return the response from the delegate-pki-authentication API key call
     * @throws IOException in case there is a problem sending the request or parsing back the response
     */
    public DelegatePkiAuthenticationResponse delegatePkiAuthentication(DelegatePkiAuthenticationRequest request, RequestOptions options)
        throws IOException {
        return restHighLevelClient.performRequestAndParseEntity(
            request,
            SecurityRequestConverters::delegatePkiAuthentication,
            options,
            DelegatePkiAuthenticationResponse::fromXContent,
            emptySet()
        );
    }

}
