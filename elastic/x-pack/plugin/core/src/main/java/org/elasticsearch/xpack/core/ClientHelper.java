/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core;

import org.elasticsearch.action.Action;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionRequestBuilder;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.support.ContextPreservingActionListener;
import org.elasticsearch.client.Client;
import org.elasticsearch.client.FilterClient;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.util.set.Sets;
import org.elasticsearch.xpack.core.security.authc.AuthenticationField;
import org.elasticsearch.xpack.core.security.authc.AuthenticationServiceField;

import java.util.Map;
import java.util.Set;
import java.util.function.BiConsumer;
import java.util.function.Supplier;
import java.util.stream.Collectors;

/**
 * Utility class to help with the execution of requests made using a {@link Client} such that they
 * have the origin as a transient and listeners have the appropriate context upon invocation
 */
public final class ClientHelper {

    /**
     * List of headers that are related to security
     */
    public static final Set<String> SECURITY_HEADER_FILTERS = Sets.newHashSet(AuthenticationServiceField.RUN_AS_USER_HEADER,
            AuthenticationField.AUTHENTICATION_KEY);

    public static final String ACTION_ORIGIN_TRANSIENT_NAME = "action.origin";
    public static final String SECURITY_ORIGIN = "security";
    public static final String WATCHER_ORIGIN = "watcher";
    public static final String ML_ORIGIN = "ml";
    public static final String MONITORING_ORIGIN = "monitoring";
    public static final String DEPRECATION_ORIGIN = "deprecation";
    public static final String PERSISTENT_TASK_ORIGIN = "persistent_tasks";
    public static final String ROLLUP_ORIGIN = "rollup";

    private ClientHelper() {}

    /**
     * Stashes the current context and sets the origin in the current context. The original context is returned as a stored context
     */
    public static ThreadContext.StoredContext stashWithOrigin(ThreadContext threadContext, String origin) {
        final ThreadContext.StoredContext storedContext = threadContext.stashContext();
        threadContext.putTransient(ACTION_ORIGIN_TRANSIENT_NAME, origin);
        return storedContext;
    }

    /**
     * Returns a client that will always set the appropriate origin and ensure the proper context is restored by listeners
     */
    public static Client clientWithOrigin(Client client, String origin) {
        return new ClientWithOrigin(client, origin);
    }

    /**
     * Executes a consumer after setting the origin and wrapping the listener so that the proper context is restored
     */
    public static <Request extends ActionRequest, Response extends ActionResponse> void executeAsyncWithOrigin(
            ThreadContext threadContext, String origin, Request request, ActionListener<Response> listener,
            BiConsumer<Request, ActionListener<Response>> consumer) {
        final Supplier<ThreadContext.StoredContext> supplier = threadContext.newRestorableContext(false);
        try (ThreadContext.StoredContext ignore = stashWithOrigin(threadContext, origin)) {
            consumer.accept(request, new ContextPreservingActionListener<>(supplier, listener));
        }
    }

    /**
     * Executes an asynchronous action using the provided client. The origin is set in the context and the listener
     * is wrapped to ensure the proper context is restored
     */
    public static <Request extends ActionRequest, Response extends ActionResponse,
            RequestBuilder extends ActionRequestBuilder<Request, Response, RequestBuilder>> void executeAsyncWithOrigin(
            Client client, String origin, Action<Request, Response, RequestBuilder> action, Request request,
            ActionListener<Response> listener) {
        final ThreadContext threadContext = client.threadPool().getThreadContext();
        final Supplier<ThreadContext.StoredContext> supplier = threadContext.newRestorableContext(false);
        try (ThreadContext.StoredContext ignore = stashWithOrigin(threadContext, origin)) {
            client.execute(action, request, new ContextPreservingActionListener<>(supplier, listener));
        }
    }

    /**
     * Execute a client operation and return the response, try to run an action
     * with least privileges, when headers exist
     *
     * @param headers
     *            Request headers, ideally including security headers
     * @param origin
     *            The origin to fall back to if there are no security headers
     * @param client
     *            The client used to query
     * @param supplier
     *            The action to run
     * @return An instance of the response class
     */
    public static <T extends ActionResponse> T executeWithHeaders(Map<String, String> headers, String origin, Client client,
            Supplier<T> supplier) {
        Map<String, String> filteredHeaders = headers.entrySet().stream().filter(e -> SECURITY_HEADER_FILTERS.contains(e.getKey()))
                .collect(Collectors.toMap(Map.Entry::getKey, Map.Entry::getValue));

        // no security headers, we will have to use the xpack internal user for
        // our execution by specifying the origin
        if (filteredHeaders.isEmpty()) {
            try (ThreadContext.StoredContext ignore = stashWithOrigin(client.threadPool().getThreadContext(), origin)) {
                return supplier.get();
            }
        } else {
            try (ThreadContext.StoredContext ignore = client.threadPool().getThreadContext().stashContext()) {
                client.threadPool().getThreadContext().copyHeaders(filteredHeaders.entrySet());
                return supplier.get();
            }
        }
    }

    /**
     * Execute a client operation asynchronously, try to run an action with
     * least privileges, when headers exist
     *
     * @param headers
     *            Request headers, ideally including security headers
     * @param origin
     *            The origin to fall back to if there are no security headers
     * @param action
     *            The action to execute
     * @param request
     *            The request object for the action
     * @param listener
     *            The listener to call when the action is complete
     */
    public static <Request extends ActionRequest, Response extends ActionResponse, 
            RequestBuilder extends ActionRequestBuilder<Request, Response, RequestBuilder>> void executeWithHeadersAsync(
            Map<String, String> headers, String origin, Client client, Action<Request, Response, RequestBuilder> action, Request request,
            ActionListener<Response> listener) {

        Map<String, String> filteredHeaders = headers.entrySet().stream().filter(e -> SECURITY_HEADER_FILTERS.contains(e.getKey()))
                .collect(Collectors.toMap(Map.Entry::getKey, Map.Entry::getValue));

        final ThreadContext threadContext = client.threadPool().getThreadContext();

        // No headers (e.g. security not installed/in use) so execute as origin
        if (filteredHeaders.isEmpty()) {
            ClientHelper.executeAsyncWithOrigin(client, origin, action, request, listener);
        } else {
            // Otherwise stash the context and copy in the saved headers before executing
            final Supplier<ThreadContext.StoredContext> supplier = threadContext.newRestorableContext(false);
            try (ThreadContext.StoredContext ignore = stashWithHeaders(threadContext, filteredHeaders)) {
                client.execute(action, request, new ContextPreservingActionListener<>(supplier, listener));
            }
        }
    }

    private static ThreadContext.StoredContext stashWithHeaders(ThreadContext threadContext, Map<String, String> headers) {
        final ThreadContext.StoredContext storedContext = threadContext.stashContext();
        threadContext.copyHeaders(headers.entrySet());
        return storedContext;
    }

    private static final class ClientWithOrigin extends FilterClient {

        private final String origin;

        private ClientWithOrigin(Client in, String origin) {
            super(in);
            this.origin = origin;
        }

        @Override
        protected <Request extends ActionRequest, Response extends ActionResponse,
                RequestBuilder extends ActionRequestBuilder<Request, Response, RequestBuilder>> void doExecute(
                Action<Request, Response, RequestBuilder> action, Request request, ActionListener<Response> listener) {
            final Supplier<ThreadContext.StoredContext> supplier = in().threadPool().getThreadContext().newRestorableContext(false);
            try (ThreadContext.StoredContext ignore = in().threadPool().getThreadContext().stashContext()) {
                in().threadPool().getThreadContext().putTransient(ACTION_ORIGIN_TRANSIENT_NAME, origin);
                super.doExecute(action, request, new ContextPreservingActionListener<>(supplier, listener));
            }
        }
    }
}
