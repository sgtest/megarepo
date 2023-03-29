/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.security.transport;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.cluster.remote.RemoteClusterNodesAction;
import org.elasticsearch.action.admin.cluster.shards.ClusterSearchShardsAction;
import org.elasticsearch.action.admin.cluster.state.ClusterStateAction;
import org.elasticsearch.action.admin.indices.resolve.ResolveIndexAction;
import org.elasticsearch.action.fieldcaps.FieldCapabilitiesAction;
import org.elasticsearch.action.search.SearchAction;
import org.elasticsearch.action.search.SearchTransportService;
import org.elasticsearch.action.search.TransportOpenPointInTimeAction;
import org.elasticsearch.action.support.DestructiveOperations;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.license.LicenseUtils;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.transport.TransportActionProxy;
import org.elasticsearch.transport.TransportRequest;
import org.elasticsearch.xpack.core.security.SecurityContext;
import org.elasticsearch.xpack.core.security.authc.Authentication;
import org.elasticsearch.xpack.security.Security;
import org.elasticsearch.xpack.security.audit.AuditUtil;
import org.elasticsearch.xpack.security.authc.CrossClusterAccessAuthenticationService;
import org.elasticsearch.xpack.security.authz.AuthorizationService;

import java.util.HashSet;
import java.util.Set;
import java.util.stream.Collectors;
import java.util.stream.Stream;

import static org.elasticsearch.transport.RemoteClusterService.REMOTE_CLUSTER_HANDSHAKE_ACTION_NAME;
import static org.elasticsearch.xpack.core.security.authc.CrossClusterAccessSubjectInfo.CROSS_CLUSTER_ACCESS_SUBJECT_INFO_HEADER_KEY;
import static org.elasticsearch.xpack.security.authc.CrossClusterAccessHeaders.CROSS_CLUSTER_ACCESS_CREDENTIALS_HEADER_KEY;

final class CrossClusterAccessServerTransportFilter extends ServerTransportFilter {
    // pkg-private for testing
    static final Set<String> ALLOWED_TRANSPORT_HEADERS;
    static {
        final Set<String> allowedHeaders = new HashSet<>(
            Set.of(CROSS_CLUSTER_ACCESS_CREDENTIALS_HEADER_KEY, CROSS_CLUSTER_ACCESS_SUBJECT_INFO_HEADER_KEY)
        );
        allowedHeaders.add(AuditUtil.AUDIT_REQUEST_ID);
        allowedHeaders.addAll(Task.HEADERS_TO_COPY);
        ALLOWED_TRANSPORT_HEADERS = Set.copyOf(allowedHeaders);
    }

    // package private for testing
    static final Set<String> CROSS_CLUSTER_ACCESS_ACTION_ALLOWLIST;
    static {
        CROSS_CLUSTER_ACCESS_ACTION_ALLOWLIST = Stream.concat(
            // These actions have proxy equivalents, so we need to allow-list the action name and the action name with the proxy action
            // prefix
            Stream.of(
                SearchTransportService.FREE_CONTEXT_SCROLL_ACTION_NAME,
                SearchTransportService.FREE_CONTEXT_ACTION_NAME,
                SearchTransportService.CLEAR_SCROLL_CONTEXTS_ACTION_NAME,
                SearchTransportService.DFS_ACTION_NAME,
                SearchTransportService.QUERY_ACTION_NAME,
                SearchTransportService.QUERY_ID_ACTION_NAME,
                SearchTransportService.QUERY_SCROLL_ACTION_NAME,
                SearchTransportService.QUERY_FETCH_SCROLL_ACTION_NAME,
                SearchTransportService.FETCH_ID_SCROLL_ACTION_NAME,
                SearchTransportService.FETCH_ID_ACTION_NAME,
                SearchTransportService.QUERY_CAN_MATCH_NAME,
                SearchTransportService.QUERY_CAN_MATCH_NODE_NAME,
                TransportOpenPointInTimeAction.OPEN_SHARD_READER_CONTEXT_NAME
            ).flatMap(name -> Stream.of(name, TransportActionProxy.getProxyAction(name))),
            // These actions don't have proxy equivalents
            Stream.of(
                REMOTE_CLUSTER_HANDSHAKE_ACTION_NAME,
                RemoteClusterNodesAction.NAME,
                SearchAction.NAME,
                ClusterStateAction.NAME,
                ClusterSearchShardsAction.NAME,
                ResolveIndexAction.NAME,
                FieldCapabilitiesAction.NAME,
                FieldCapabilitiesAction.NAME + "[n]",
                "indices:data/read/eql"
            )
        ).collect(Collectors.toUnmodifiableSet());
    }

    private final CrossClusterAccessAuthenticationService crossClusterAccessAuthcService;
    private final XPackLicenseState licenseState;

    CrossClusterAccessServerTransportFilter(
        CrossClusterAccessAuthenticationService crossClusterAccessAuthcService,
        AuthorizationService authzService,
        ThreadContext threadContext,
        boolean extractClientCert,
        DestructiveOperations destructiveOperations,
        SecurityContext securityContext,
        XPackLicenseState licenseState
    ) {
        super(
            crossClusterAccessAuthcService.getAuthenticationService(),
            authzService,
            threadContext,
            extractClientCert,
            destructiveOperations,
            securityContext
        );
        this.crossClusterAccessAuthcService = crossClusterAccessAuthcService;
        this.licenseState = licenseState;
    }

    @Override
    protected void authenticate(
        final String securityAction,
        final TransportRequest request,
        final ActionListener<Authentication> authenticationListener
    ) {
        if (false == Security.CONFIGURABLE_CROSS_CLUSTER_ACCESS_FEATURE.check(licenseState)) {
            authenticationListener.onFailure(
                LicenseUtils.newComplianceException(Security.CONFIGURABLE_CROSS_CLUSTER_ACCESS_FEATURE.getName())
            );

        } else if (false == CROSS_CLUSTER_ACCESS_ACTION_ALLOWLIST.contains(securityAction)) {
            authenticationListener.onFailure(
                new IllegalArgumentException(
                    "action ["
                        + securityAction
                        + "] is not allowed as a cross cluster operation on the dedicated remote cluster server port"
                )
            );
        } else {
            try {
                validateHeaders();
            } catch (Exception ex) {
                authenticationListener.onFailure(ex);
                return;
            }
            crossClusterAccessAuthcService.authenticate(securityAction, request, authenticationListener);
        }
    }

    private void validateHeaders() {
        final ThreadContext threadContext = getThreadContext();
        ensureRequiredHeaderInContext(threadContext, CROSS_CLUSTER_ACCESS_CREDENTIALS_HEADER_KEY);
        ensureRequiredHeaderInContext(threadContext, CROSS_CLUSTER_ACCESS_SUBJECT_INFO_HEADER_KEY);
        for (String header : threadContext.getHeaders().keySet()) {
            if (false == ALLOWED_TRANSPORT_HEADERS.contains(header)) {
                throw new IllegalArgumentException(
                    "Transport request header ["
                        + header
                        + "] is not allowed for cross cluster requests through the dedicated remote cluster server port"
                );
            }
        }
    }

    private void ensureRequiredHeaderInContext(ThreadContext threadContext, String requiredHeader) {
        if (threadContext.getHeader(requiredHeader) == null) {
            throw new IllegalArgumentException(
                "Cross cluster requests through the dedicated remote cluster server port require transport header ["
                    + requiredHeader
                    + "] but none found. "
                    + "Please ensure you have configured remote cluster credentials on the cluster originating the request."
            );
        }
    }

}
