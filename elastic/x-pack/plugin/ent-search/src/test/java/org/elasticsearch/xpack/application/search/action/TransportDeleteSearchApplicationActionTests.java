/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.application.search.action;

import org.elasticsearch.ElasticsearchSecurityException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.license.MockLicenseState;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.application.utils.LicenseUtils;

import java.util.concurrent.atomic.AtomicReference;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.nullValue;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class TransportDeleteSearchApplicationActionTests extends ESTestCase {
    public void testWithUnsupportedLicense() {
        MockLicenseState licenseState = mock(MockLicenseState.class);

        when(licenseState.isAllowed(LicenseUtils.LICENSED_ENT_SEARCH_FEATURE)).thenReturn(false);
        when(licenseState.isActive()).thenReturn(false);
        when(licenseState.statusDescription()).thenReturn("invalid license");

        TransportDeleteSearchApplicationAction transportAction = new TransportDeleteSearchApplicationAction(
            mock(TransportService.class),
            mock(ActionFilters.class),
            mock(Client.class),
            mock(ClusterService.class),
            mock(NamedWriteableRegistry.class),
            mock(BigArrays.class),
            licenseState
        );

        DeleteSearchApplicationAction.Request request = new DeleteSearchApplicationAction.Request("my-search-app");

        final AtomicReference<Throwable> throwableRef = new AtomicReference<>();
        final AtomicReference<AcknowledgedResponse> responseRef = new AtomicReference<>();

        transportAction.doExecute(mock(Task.class), request, new ActionListener<>() {
            @Override
            public void onResponse(AcknowledgedResponse response) {
                responseRef.set(response);
            }

            @Override
            public void onFailure(Exception e) {
                throwableRef.set(e);
            }
        });

        assertThat(responseRef.get(), is(nullValue()));
        assertThat(throwableRef.get(), instanceOf(ElasticsearchSecurityException.class));
        assertThat(
            throwableRef.get().getMessage(),
            containsString("Search Applications and behavioral analytics require an active trial, platinum or enterprise license.")
        );
    }
}
