/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.application.analytics.action;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.application.analytics.AnalyticsCollectionService;

import static org.elasticsearch.xpack.application.analytics.action.AnalyticsTransportActionTestUtils.mockLicenseState;
import static org.elasticsearch.xpack.application.analytics.action.AnalyticsTransportActionTestUtils.verifyExceptionIsThrownOnInvalidLicence;
import static org.elasticsearch.xpack.application.analytics.action.AnalyticsTransportActionTestUtils.verifyNoExceptionIsThrown;
import static org.elasticsearch.xpack.application.analytics.action.AnalyticsTransportActionTestUtils.verifyNoResponseIsSent;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.never;
import static org.mockito.Mockito.times;
import static org.mockito.Mockito.verify;

public class TransportPutAnalyticsCollectionActionTests extends ESTestCase {

    public void testWithSupportedLicense() {
        AnalyticsCollectionService analyticsCollectionService = mock(AnalyticsCollectionService.class);

        TransportPutAnalyticsCollectionAction transportAction = createTransportAction(mockLicenseState(true), analyticsCollectionService);
        PutAnalyticsCollectionAction.Request request = mock(PutAnalyticsCollectionAction.Request.class);

        ClusterState clusterState = mock(ClusterState.class);

        @SuppressWarnings("unchecked")
        ActionListener<PutAnalyticsCollectionAction.Response> listener = mock(ActionListener.class);

        transportAction.masterOperation(mock(Task.class), request, clusterState, listener);

        verify(analyticsCollectionService, times(1)).putAnalyticsCollection(clusterState, request, listener);
        verifyNoExceptionIsThrown(listener);
    }

    public void testWithUnsupportedLicense() {
        AnalyticsCollectionService analyticsCollectionService = mock(AnalyticsCollectionService.class);

        TransportPutAnalyticsCollectionAction transportAction = createTransportAction(
            AnalyticsTransportActionTestUtils.mockLicenseState(false),
            analyticsCollectionService
        );
        PutAnalyticsCollectionAction.Request request = mock(PutAnalyticsCollectionAction.Request.class);

        ClusterState clusterState = mock(ClusterState.class);

        @SuppressWarnings("unchecked")
        ActionListener<PutAnalyticsCollectionAction.Response> listener = mock(ActionListener.class);

        transportAction.masterOperation(mock(Task.class), request, clusterState, listener);

        verify(analyticsCollectionService, never()).putAnalyticsCollection(any(), any(), any());

        verifyNoResponseIsSent(listener);
        verifyExceptionIsThrownOnInvalidLicence(listener);
    }

    private TransportPutAnalyticsCollectionAction createTransportAction(
        XPackLicenseState licenseState,
        AnalyticsCollectionService analyticsCollectionService
    ) {
        return new TransportPutAnalyticsCollectionAction(
            mock(TransportService.class),
            mock(ClusterService.class),
            mock(ThreadPool.class),
            mock(ActionFilters.class),
            mock(IndexNameExpressionResolver.class),
            analyticsCollectionService,
            licenseState
        );
    }
}
