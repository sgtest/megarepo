/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.search;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.ExceptionsHelper;
import org.elasticsearch.ResourceAlreadyExistsException;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.delete.DeleteRequest;
import org.elasticsearch.action.get.GetRequest;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.index.IndexResponse;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.action.update.UpdateRequest;
import org.elasticsearch.action.update.UpdateResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.client.OriginSettingClient;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.io.stream.ByteBufferStreamInput;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.io.stream.NamedWriteableAwareStreamInput;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.tasks.TaskManager;
import org.elasticsearch.xpack.core.search.action.AsyncSearchResponse;
import org.elasticsearch.xpack.core.security.SecurityContext;
import org.elasticsearch.xpack.core.security.authc.Authentication;
import org.elasticsearch.xpack.core.security.authc.support.AuthenticationContextSerializer;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.util.Base64;
import java.util.Collections;
import java.util.HashMap;
import java.util.Map;

import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.elasticsearch.index.mapper.MapperService.SINGLE_MAPPING_NAME;
import static org.elasticsearch.xpack.core.ClientHelper.ASYNC_SEARCH_ORIGIN;
import static org.elasticsearch.xpack.core.security.authc.AuthenticationField.AUTHENTICATION_KEY;

/**
 * A service that exposes the CRUD operations for the async-search index.
 */
class AsyncSearchIndexService {
    private static final Logger logger = LogManager.getLogger(AsyncSearchIndexService.class);

    public static final String INDEX = ".async-search";

    public static final String HEADERS_FIELD = "headers";
    public static final String EXPIRATION_TIME_FIELD = "expiration_time";
    public static final String RESULT_FIELD = "result";

    private static Settings settings() {
        return Settings.builder()
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .put(IndexMetaData.SETTING_AUTO_EXPAND_REPLICAS, "0-1")
            .build();
    }

    private static XContentBuilder mappings() throws IOException {
        XContentBuilder builder = jsonBuilder()
            .startObject()
                .startObject(SINGLE_MAPPING_NAME)
                    .startObject("_meta")
                        .field("version", Version.CURRENT)
                    .endObject()
                    .field("dynamic", "strict")
                    .startObject("properties")
                        .startObject(HEADERS_FIELD)
                            .field("type", "object")
                            .field("enabled", "false")
                        .endObject()
                        .startObject(RESULT_FIELD)
                            .field("type", "object")
                            .field("enabled", "false")
                        .endObject()
                        .startObject(EXPIRATION_TIME_FIELD)
                            .field("type", "long")
                        .endObject()
                    .endObject()
                .endObject()
            .endObject();
        return builder;
    }

    private final ClusterService clusterService;
    private final Client client;
    private final SecurityContext securityContext;
    private final NamedWriteableRegistry registry;

    AsyncSearchIndexService(ClusterService clusterService,
                            ThreadContext threadContext,
                            Client client,
                            NamedWriteableRegistry registry) {
        this.clusterService = clusterService;
        this.securityContext = new SecurityContext(clusterService.getSettings(), threadContext);
        this.client = new OriginSettingClient(client, ASYNC_SEARCH_ORIGIN);
        this.registry = registry;
    }

    /**
     * Returns the internal client with origin.
     */
    Client getClient() {
        return client;
    }

    /**
     * Creates the index with the expected settings and mappings if it doesn't exist.
     */
    void createIndexIfNecessary(ActionListener<Void> listener) {
        if (clusterService.state().routingTable().hasIndex(AsyncSearchIndexService.INDEX) == false) {
            try {
                client.admin().indices().prepareCreate(INDEX)
                    .setSettings(settings())
                    .setMapping(mappings())
                    .execute(ActionListener.wrap(
                        resp -> listener.onResponse(null),
                        exc -> {
                            if (ExceptionsHelper.unwrapCause(exc) instanceof ResourceAlreadyExistsException) {
                                listener.onResponse(null);
                            } else {
                                logger.error("failed to create async-search index", exc);
                                listener.onFailure(exc);
                            }
                        }));
            } catch (Exception exc) {
                logger.error("failed to create async-search index", exc);
                listener.onFailure(exc);
            }
        } else {
            listener.onResponse(null);
        }
    }

    /**
     * Stores the initial response with the original headers of the authenticated user
     * and the expected expiration time.
     */
    void storeInitialResponse(String docId,
                              Map<String, String> headers,
                              AsyncSearchResponse response,
                              ActionListener<IndexResponse> listener) throws IOException {
        Map<String, Object> source = new HashMap<>();
        source.put(HEADERS_FIELD, headers);
        source.put(EXPIRATION_TIME_FIELD, response.getExpirationTime());
        source.put(RESULT_FIELD, encodeResponse(response));
        IndexRequest indexRequest = new IndexRequest(INDEX)
            .create(true)
            .id(docId)
            .source(source, XContentType.JSON);
        createIndexIfNecessary(ActionListener.wrap(v -> client.index(indexRequest, listener), listener::onFailure));
    }

    /**
     * Stores the final response if the place-holder document is still present (update).
     */
    void storeFinalResponse(String docId,
                            AsyncSearchResponse response,
                            ActionListener<UpdateResponse> listener) throws IOException {
        Map<String, Object> source = new HashMap<>();
        source.put(RESULT_FIELD, encodeResponse(response));
        UpdateRequest request = new UpdateRequest()
            .index(INDEX)
            .id(docId)
            .doc(source, XContentType.JSON);
        createIndexIfNecessary(ActionListener.wrap(v -> client.update(request, listener), listener::onFailure));
    }

    /**
     * Updates the expiration time of the provided <code>docId</code> if the place-holder
     * document is still present (update).
     */
    void updateExpirationTime(String docId,
                              long expirationTimeMillis,
                              ActionListener<UpdateResponse> listener) {
        Map<String, Object> source = Collections.singletonMap(EXPIRATION_TIME_FIELD, expirationTimeMillis);
        UpdateRequest request = new UpdateRequest().index(INDEX)
            .id(docId)
            .doc(source, XContentType.JSON);
        createIndexIfNecessary(ActionListener.wrap(v -> client.update(request, listener), listener::onFailure));
    }

    /**
     * Deletes the provided <code>searchId</code> from the index if present.
     */
    void deleteResponse(AsyncSearchId searchId,
                        boolean failIfNotFound,
                        ActionListener<AcknowledgedResponse> listener) {
        DeleteRequest request = new DeleteRequest(INDEX).id(searchId.getDocId());
        createIndexIfNecessary(
            ActionListener.wrap(v -> client.delete(request,
                ActionListener.wrap(
                    resp -> {
                        if (resp.status() == RestStatus.NOT_FOUND && failIfNotFound) {
                            listener.onFailure(new ResourceNotFoundException(searchId.getEncoded()));
                        } else {
                            listener.onResponse(new AcknowledgedResponse(true));
                        }
                    },
                    exc -> {
                        logger.error(() -> new ParameterizedMessage("failed to clean async-search [{}]", searchId.getEncoded()), exc);
                        listener.onFailure(exc);
                    })),
                listener::onFailure));
    }

    /**
     * Returns the {@link AsyncSearchTask} if the provided <code>searchId</code>
     * is registered in the task manager, <code>null</code> otherwise.
     *
     * This method throws a {@link ResourceNotFoundException} if the authenticated user
     * is not the creator of the original task.
     */
    AsyncSearchTask getTask(TaskManager taskManager, AsyncSearchId searchId) throws IOException {
        Task task = taskManager.getTask(searchId.getTaskId().getId());
        if (task instanceof AsyncSearchTask == false) {
            return null;
        }
        AsyncSearchTask searchTask = (AsyncSearchTask) task;
        if (searchTask.getSearchId().equals(searchId) == false) {
            return null;
        }

        // Check authentication for the user
        final Authentication auth = securityContext.getAuthentication();
        if (ensureAuthenticatedUserIsSame(searchTask.getOriginHeaders(), auth) == false) {
            throw new ResourceNotFoundException(searchId.getEncoded() + " not found");
        }
        return searchTask;
    }

    /**
     * Gets the response from the index if present, or delegate a {@link ResourceNotFoundException}
     * failure to the provided listener if not.
     */
    void getResponse(AsyncSearchId searchId,
                     ActionListener<AsyncSearchResponse> listener) {
        final Authentication current = securityContext.getAuthentication();
        GetRequest internalGet = new GetRequest(INDEX)
            .preference(searchId.getEncoded())
            .id(searchId.getDocId());
        client.get(internalGet, ActionListener.wrap(
            get -> {
                if (get.isExists() == false) {
                    listener.onFailure(new ResourceNotFoundException(searchId.getEncoded()));
                    return;
                }

                // check the authentication of the current user against the user that initiated the async search
                @SuppressWarnings("unchecked")
                Map<String, String> headers = (Map<String, String>) get.getSource().get(HEADERS_FIELD);
                if (ensureAuthenticatedUserIsSame(headers, current) == false) {
                    listener.onFailure(new ResourceNotFoundException(searchId.getEncoded()));
                    return;
                }

                String encoded = (String) get.getSource().get(RESULT_FIELD);
                listener.onResponse(encoded != null ? decodeResponse(encoded) : null);
            },
            listener::onFailure
        ));
    }

    /**
     * Extracts the authentication from the original headers and checks that it matches
     * the current user. This function returns always <code>true</code> if the provided
     * <code>headers</code> do not contain any authentication.
     */
    boolean ensureAuthenticatedUserIsSame(Map<String, String> originHeaders, Authentication current) throws IOException {
        if (originHeaders == null || originHeaders.containsKey(AUTHENTICATION_KEY) == false) {
            // no authorization attached to the original request
            return true;
        }
        if (current == null) {
            // origin is an authenticated user but current is not
            return false;
        }
        Authentication origin = AuthenticationContextSerializer.decode(originHeaders.get(AUTHENTICATION_KEY));
        return ensureAuthenticatedUserIsSame(origin, current);
    }

    /**
     * Compares the {@link Authentication} that was used to create the {@link AsyncSearchId} with the
     * current authentication.
     */
    boolean ensureAuthenticatedUserIsSame(Authentication original, Authentication current) {
        final boolean samePrincipal = original.getUser().principal().equals(current.getUser().principal());
        final boolean sameRealmType;
        if (original.getUser().isRunAs()) {
            if (current.getUser().isRunAs()) {
                sameRealmType = original.getLookedUpBy().getType().equals(current.getLookedUpBy().getType());
            }  else {
                sameRealmType = original.getLookedUpBy().getType().equals(current.getAuthenticatedBy().getType());
            }
        } else if (current.getUser().isRunAs()) {
            sameRealmType = original.getAuthenticatedBy().getType().equals(current.getLookedUpBy().getType());
        } else {
            sameRealmType = original.getAuthenticatedBy().getType().equals(current.getAuthenticatedBy().getType());
        }
        return samePrincipal && sameRealmType;
    }

    /**
     * Encode the provided response in a binary form using base64 encoding.
     */
    String encodeResponse(AsyncSearchResponse response) throws IOException {
        try (BytesStreamOutput out = new BytesStreamOutput()) {
            Version.writeVersion(Version.CURRENT, out);
            response.writeTo(out);
            return Base64.getEncoder().encodeToString(BytesReference.toBytes(out.bytes()));
        }
    }

    /**
     * Decode the provided base-64 bytes into a {@link AsyncSearchResponse}.
     */
    AsyncSearchResponse decodeResponse(String value) throws IOException {
        try (ByteBufferStreamInput buf = new ByteBufferStreamInput(ByteBuffer.wrap(Base64.getDecoder().decode(value)))) {
            try (StreamInput in = new NamedWriteableAwareStreamInput(buf, registry)) {
                in.setVersion(Version.readVersion(in));
                return new AsyncSearchResponse(in);
            }
        }
    }
}
