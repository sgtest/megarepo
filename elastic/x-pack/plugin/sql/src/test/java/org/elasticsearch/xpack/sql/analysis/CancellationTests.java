/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.sql.analysis;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.fieldcaps.FieldCapabilities;
import org.elasticsearch.action.fieldcaps.FieldCapabilitiesResponse;
import org.elasticsearch.action.search.SearchAction;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchRequestBuilder;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.tasks.TaskCancelledException;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.ql.index.IndexResolver;
import org.elasticsearch.xpack.ql.type.DefaultDataTypeRegistry;
import org.elasticsearch.xpack.sql.action.SqlQueryAction;
import org.elasticsearch.xpack.sql.action.SqlQueryRequest;
import org.elasticsearch.xpack.sql.action.SqlQueryRequestBuilder;
import org.elasticsearch.xpack.sql.action.SqlQueryResponse;
import org.elasticsearch.xpack.sql.action.SqlQueryTask;
import org.elasticsearch.xpack.sql.execution.PlanExecutor;
import org.elasticsearch.xpack.sql.plugin.TransportSqlQueryAction;
import org.mockito.ArgumentCaptor;
import org.mockito.stubbing.Answer;

import java.util.Collections;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicBoolean;

import static java.util.Collections.emptyMap;
import static java.util.Collections.singletonMap;
import static org.hamcrest.Matchers.instanceOf;
import static org.mockito.Matchers.any;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.times;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.verifyNoMoreInteractions;
import static org.mockito.Mockito.when;

public class CancellationTests extends ESTestCase {

    public void testCancellationBeforeFieldCaps() throws InterruptedException {
        Client client = mock(Client.class);
        SqlQueryTask task = mock(SqlQueryTask.class);
        when(task.isCancelled()).thenReturn(true);
        ClusterService mockClusterService = mockClusterService();

        IndexResolver indexResolver = indexResolver(client);
        PlanExecutor planExecutor = new PlanExecutor(client, indexResolver, new NamedWriteableRegistry(Collections.emptyList()));
        CountDownLatch countDownLatch = new CountDownLatch(1);
        SqlQueryRequest request = new SqlQueryRequestBuilder(client, SqlQueryAction.INSTANCE).query("SELECT foo FROM bar").request();
        TransportSqlQueryAction.operation(planExecutor, task, request, new ActionListener<>() {
                @Override
                public void onResponse(SqlQueryResponse sqlSearchResponse) {
                    fail("Shouldn't be here");
                    countDownLatch.countDown();
                }

                @Override
                public void onFailure(Exception e) {
                    assertThat(e, instanceOf(TaskCancelledException.class));
                    countDownLatch.countDown();
                }
            }, "", mock(TransportService.class), mockClusterService);
        countDownLatch.await();
        verify(task, times(1)).isCancelled();
        verify(task, times(1)).getId();
        verify(client, times(1)).settings();
        verify(client, times(1)).threadPool();
        verifyNoMoreInteractions(client, task);
    }

    private Map<String, Map<String, FieldCapabilities>> fields(String[] indices) {
        FieldCapabilities fooField =
            new FieldCapabilities("foo", "integer", false, true, true, indices, null, null, emptyMap());
        FieldCapabilities categoryField =
            new FieldCapabilities("event.category", "keyword", false, true, true, indices, null, null, emptyMap());
        FieldCapabilities timestampField =
            new FieldCapabilities("@timestamp", "date", false, true, true, indices, null, null, emptyMap());
        Map<String, Map<String, FieldCapabilities>> fields = new HashMap<>();
        fields.put(fooField.getName(), singletonMap(fooField.getName(), fooField));
        fields.put(categoryField.getName(), singletonMap(categoryField.getName(), categoryField));
        fields.put(timestampField.getName(), singletonMap(timestampField.getName(), timestampField));
        return fields;
    }

    public void testCancellationBeforeSearch() throws InterruptedException {
        Client client = mock(Client.class);

        AtomicBoolean cancelled = new AtomicBoolean(false);
        SqlQueryTask task = mock(SqlQueryTask.class);
        long taskId = randomNonNegativeLong();
        when(task.isCancelled()).then(invocationOnMock -> cancelled.get());
        when(task.getId()).thenReturn(taskId);
        ClusterService mockClusterService = mockClusterService();

        String[] indices = new String[]{"endgame"};

        FieldCapabilitiesResponse fieldCapabilitiesResponse = mock(FieldCapabilitiesResponse.class);
        when(fieldCapabilitiesResponse.getIndices()).thenReturn(indices);
        when(fieldCapabilitiesResponse.get()).thenReturn(fields(indices));
        doAnswer((Answer<Void>) invocation -> {
            @SuppressWarnings("unchecked")
            ActionListener<FieldCapabilitiesResponse> listener = (ActionListener<FieldCapabilitiesResponse>) invocation.getArguments()[1];
            assertFalse(cancelled.getAndSet(true));
            listener.onResponse(fieldCapabilitiesResponse);
            return null;
        }).when(client).fieldCaps(any(), any());


        IndexResolver indexResolver = indexResolver(client);
        PlanExecutor planExecutor = new PlanExecutor(client, indexResolver, new NamedWriteableRegistry(Collections.emptyList()));
        CountDownLatch countDownLatch = new CountDownLatch(1);
        SqlQueryRequest request = new SqlQueryRequestBuilder(client, SqlQueryAction.INSTANCE)
            .query("SELECT foo FROM " + indices[0]).request();
        TransportSqlQueryAction.operation(planExecutor, task, request, new ActionListener<>() {
            @Override
            public void onResponse(SqlQueryResponse sqlSearchResponse) {
                fail("Shouldn't be here");
                countDownLatch.countDown();
            }

            @Override
            public void onFailure(Exception e) {
                assertThat(e, instanceOf(TaskCancelledException.class));
                countDownLatch.countDown();
            }
        }, "", mock(TransportService.class), mockClusterService);
        countDownLatch.await();
        verify(client, times(1)).fieldCaps(any(), any());
        verify(task, times(2)).isCancelled();
        verify(task, times(1)).getId();
        verify(client, times(1)).settings();
        verify(client, times(1)).threadPool();
        verifyNoMoreInteractions(client, task);
    }

    public void testCancellationDuringSearch() throws InterruptedException {
        Client client = mock(Client.class);

        SqlQueryTask task = mock(SqlQueryTask.class);
        String nodeId = randomAlphaOfLength(10);
        long taskId = randomNonNegativeLong();
        when(task.isCancelled()).thenReturn(false);
        when(task.getId()).thenReturn(taskId);
        ClusterService mockClusterService = mockClusterService(nodeId);

        String[] indices = new String[]{"endgame"};

        // Emulation of field capabilities
        FieldCapabilitiesResponse fieldCapabilitiesResponse = mock(FieldCapabilitiesResponse.class);
        when(fieldCapabilitiesResponse.getIndices()).thenReturn(indices);
        when(fieldCapabilitiesResponse.get()).thenReturn(fields(indices));
        doAnswer((Answer<Void>) invocation -> {
            @SuppressWarnings("unchecked")
            ActionListener<FieldCapabilitiesResponse> listener = (ActionListener<FieldCapabilitiesResponse>) invocation.getArguments()[1];
            listener.onResponse(fieldCapabilitiesResponse);
            return null;
        }).when(client).fieldCaps(any(), any());

        // Emulation of search cancellation
        ArgumentCaptor<SearchRequest> searchRequestCaptor = ArgumentCaptor.forClass(SearchRequest.class);
        when(client.prepareSearch(any())).thenReturn(new SearchRequestBuilder(client, SearchAction.INSTANCE).setIndices(indices));
        doAnswer((Answer<Void>) invocation -> {
            @SuppressWarnings("unchecked")
            SearchRequest request = (SearchRequest) invocation.getArguments()[1];
            TaskId parentTask = request.getParentTask();
            assertNotNull(parentTask);
            assertEquals(taskId, parentTask.getId());
            assertEquals(nodeId, parentTask.getNodeId());
            @SuppressWarnings("unchecked")
            ActionListener<SearchResponse> listener = (ActionListener<SearchResponse>) invocation.getArguments()[2];
            listener.onFailure(new TaskCancelledException("cancelled"));
            return null;
        }).when(client).execute(any(), searchRequestCaptor.capture(), any());

        IndexResolver indexResolver = indexResolver(client);
        PlanExecutor planExecutor = new PlanExecutor(client, indexResolver, new NamedWriteableRegistry(Collections.emptyList()));
        SqlQueryRequest request = new SqlQueryRequestBuilder(client, SqlQueryAction.INSTANCE)
            .query("SELECT foo FROM " + indices[0]).request();
        CountDownLatch countDownLatch = new CountDownLatch(1);
        TransportSqlQueryAction.operation(planExecutor, task, request, new ActionListener<>() {
            @Override
            public void onResponse(SqlQueryResponse sqlSearchResponse) {
                fail("Shouldn't be here");
                countDownLatch.countDown();
            }

            @Override
            public void onFailure(Exception e) {
                assertThat(e, instanceOf(TaskCancelledException.class));
                countDownLatch.countDown();
            }
        }, "", mock(TransportService.class), mockClusterService);
        countDownLatch.await();
        // Final verification to ensure no more interaction
        verify(client).fieldCaps(any(), any());
        verify(client).execute(any(), any(), any());
        verify(task, times(2)).isCancelled();
        verify(task, times(1)).getId();
        verify(client, times(1)).settings();
        verify(client, times(1)).threadPool();
        verifyNoMoreInteractions(client, task);
    }

    private ClusterService mockClusterService() {
        return mockClusterService(null);
    }

    private ClusterService mockClusterService(String nodeId) {
        final ClusterService mockClusterService = mock(ClusterService.class);
        final DiscoveryNode mockNode = mock(DiscoveryNode.class);
        final ClusterName mockClusterName = mock(ClusterName.class);
        when(mockNode.getId()).thenReturn(nodeId == null ? randomAlphaOfLength(10) : nodeId);
        when(mockClusterService.localNode()).thenReturn(mockNode);
        when(mockClusterName.value()).thenReturn(randomAlphaOfLength(10));
        when(mockClusterService.getClusterName()).thenReturn(mockClusterName);
        return mockClusterService;
    }

    private static IndexResolver indexResolver(Client client) {
        return new IndexResolver(client, randomAlphaOfLength(10), DefaultDataTypeRegistry.INSTANCE);
    }
}
