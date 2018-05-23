/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.job.persistence;

import org.elasticsearch.action.ActionFuture;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.cluster.health.ClusterHealthRequestBuilder;
import org.elasticsearch.action.admin.cluster.health.ClusterHealthResponse;
import org.elasticsearch.action.admin.indices.alias.IndicesAliasesRequest;
import org.elasticsearch.action.admin.indices.alias.IndicesAliasesRequestBuilder;
import org.elasticsearch.action.admin.indices.alias.IndicesAliasesResponse;
import org.elasticsearch.action.admin.indices.create.CreateIndexRequest;
import org.elasticsearch.action.admin.indices.create.CreateIndexRequestBuilder;
import org.elasticsearch.action.admin.indices.create.CreateIndexResponse;
import org.elasticsearch.action.admin.indices.delete.DeleteIndexAction;
import org.elasticsearch.action.admin.indices.delete.DeleteIndexRequest;
import org.elasticsearch.action.admin.indices.delete.DeleteIndexResponse;
import org.elasticsearch.action.admin.indices.exists.indices.IndicesExistsRequest;
import org.elasticsearch.action.admin.indices.exists.indices.IndicesExistsResponse;
import org.elasticsearch.action.admin.indices.mapping.get.GetMappingsRequestBuilder;
import org.elasticsearch.action.admin.indices.mapping.get.GetMappingsResponse;
import org.elasticsearch.action.admin.indices.mapping.put.PutMappingRequestBuilder;
import org.elasticsearch.action.admin.indices.mapping.put.PutMappingResponse;
import org.elasticsearch.action.admin.indices.template.put.PutIndexTemplateRequest;
import org.elasticsearch.action.admin.indices.template.put.PutIndexTemplateResponse;
import org.elasticsearch.action.bulk.BulkRequest;
import org.elasticsearch.action.bulk.BulkRequestBuilder;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.action.get.GetRequestBuilder;
import org.elasticsearch.action.get.GetResponse;
import org.elasticsearch.action.index.IndexRequestBuilder;
import org.elasticsearch.action.index.IndexResponse;
import org.elasticsearch.action.search.SearchRequestBuilder;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.search.SearchScrollRequestBuilder;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.action.support.WriteRequest.RefreshPolicy;
import org.elasticsearch.client.AdminClient;
import org.elasticsearch.client.Client;
import org.elasticsearch.client.ClusterAdminClient;
import org.elasticsearch.client.IndicesAdminClient;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.search.sort.SortBuilder;
import org.elasticsearch.search.sort.SortOrder;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.ml.action.DeleteJobAction;
import org.mockito.ArgumentCaptor;
import org.mockito.invocation.InvocationOnMock;
import org.mockito.stubbing.Answer;

import java.io.IOException;
import java.util.concurrent.ExecutionException;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.mockito.Matchers.any;
import static org.mockito.Matchers.anyBoolean;
import static org.mockito.Matchers.anyInt;
import static org.mockito.Matchers.anyString;
import static org.mockito.Matchers.eq;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.reset;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

public class MockClientBuilder {
    private Client client;

    private AdminClient adminClient;
    private ClusterAdminClient clusterAdminClient;
    private IndicesAdminClient indicesAdminClient;

    private IndicesAliasesRequestBuilder aliasesRequestBuilder;

    public MockClientBuilder(String clusterName) {
        client = mock(Client.class);
        adminClient = mock(AdminClient.class);
        clusterAdminClient = mock(ClusterAdminClient.class);
        indicesAdminClient = mock(IndicesAdminClient.class);
        aliasesRequestBuilder = mock(IndicesAliasesRequestBuilder.class);

        when(client.admin()).thenReturn(adminClient);
        when(adminClient.cluster()).thenReturn(clusterAdminClient);
        when(adminClient.indices()).thenReturn(indicesAdminClient);
        Settings settings = Settings.builder().put("cluster.name", clusterName).build();
        when(client.settings()).thenReturn(settings);
        ThreadPool threadPool = mock(ThreadPool.class);
        when(client.threadPool()).thenReturn(threadPool);
        when(threadPool.getThreadContext()).thenReturn(new ThreadContext(Settings.EMPTY));
    }

    @SuppressWarnings({ "unchecked" })
    public MockClientBuilder addClusterStatusYellowResponse() throws InterruptedException, ExecutionException {
        PlainActionFuture<ClusterHealthResponse> actionFuture = mock(PlainActionFuture.class);
        ClusterHealthRequestBuilder clusterHealthRequestBuilder = mock(ClusterHealthRequestBuilder.class);

        when(clusterAdminClient.prepareHealth()).thenReturn(clusterHealthRequestBuilder);
        when(clusterHealthRequestBuilder.setWaitForYellowStatus()).thenReturn(clusterHealthRequestBuilder);
        when(clusterHealthRequestBuilder.execute()).thenReturn(actionFuture);
        when(actionFuture.actionGet()).thenReturn(mock(ClusterHealthResponse.class));
        return this;
    }

    @SuppressWarnings({ "unchecked" })
    public MockClientBuilder addClusterStatusYellowResponse(String index) throws InterruptedException, ExecutionException {
        PlainActionFuture<ClusterHealthResponse> actionFuture = mock(PlainActionFuture.class);
        ClusterHealthRequestBuilder clusterHealthRequestBuilder = mock(ClusterHealthRequestBuilder.class);

        when(clusterAdminClient.prepareHealth(index)).thenReturn(clusterHealthRequestBuilder);
        when(clusterHealthRequestBuilder.setWaitForYellowStatus()).thenReturn(clusterHealthRequestBuilder);
        when(clusterHealthRequestBuilder.execute()).thenReturn(actionFuture);
        when(actionFuture.actionGet()).thenReturn(mock(ClusterHealthResponse.class));
        return this;
    }

    @SuppressWarnings({ "rawtypes", "unchecked" })
    public MockClientBuilder addIndicesExistsResponse(String index, boolean exists) throws InterruptedException, ExecutionException {
        ActionFuture actionFuture = mock(ActionFuture.class);
        ArgumentCaptor<IndicesExistsRequest> requestCaptor = ArgumentCaptor.forClass(IndicesExistsRequest.class);

        when(indicesAdminClient.exists(requestCaptor.capture())).thenReturn(actionFuture);
        doAnswer(invocation -> {
            IndicesExistsRequest request = (IndicesExistsRequest) invocation.getArguments()[0];
            return request.indices()[0].equals(index) ? actionFuture : null;
        }).when(indicesAdminClient).exists(any(IndicesExistsRequest.class));
        when(actionFuture.get()).thenReturn(new IndicesExistsResponse(exists));
        when(actionFuture.actionGet()).thenReturn(new IndicesExistsResponse(exists));
        return this;
    }

    @SuppressWarnings({ "unchecked" })
    public MockClientBuilder addIndicesDeleteResponse(String index, boolean exists, boolean exception,
            ActionListener<DeleteJobAction.Response> actionListener) throws InterruptedException, ExecutionException, IOException {
        DeleteIndexResponse response = DeleteIndexAction.INSTANCE.newResponse();
        StreamInput si = mock(StreamInput.class);
        // this looks complicated but Mockito can't mock the final method
        // DeleteIndexResponse.isAcknowledged() and the only way to create
        // one with a true response is reading from a stream.
        when(si.readByte()).thenReturn((byte) 0x01);
        response.readFrom(si);

        doAnswer(invocation -> {
            DeleteIndexRequest deleteIndexRequest = (DeleteIndexRequest) invocation.getArguments()[0];
            assertArrayEquals(new String[] { index }, deleteIndexRequest.indices());
            if (exception) {
                actionListener.onFailure(new InterruptedException());
            } else {
                actionListener.onResponse(new DeleteJobAction.Response(true));
            }
            return null;
        }).when(indicesAdminClient).delete(any(DeleteIndexRequest.class), any(ActionListener.class));
        return this;
    }

    public MockClientBuilder prepareGet(String index, String type, String id, GetResponse response) {
        GetRequestBuilder getRequestBuilder = mock(GetRequestBuilder.class);
        when(getRequestBuilder.get()).thenReturn(response);
        when(getRequestBuilder.setFetchSource(false)).thenReturn(getRequestBuilder);
        when(client.prepareGet(index, type, id)).thenReturn(getRequestBuilder);
        return this;
    }

    public MockClientBuilder prepareCreate(String index) {
        CreateIndexRequestBuilder createIndexRequestBuilder = mock(CreateIndexRequestBuilder.class);
        CreateIndexResponse response = mock(CreateIndexResponse.class);
        when(createIndexRequestBuilder.setSettings(any(Settings.Builder.class))).thenReturn(createIndexRequestBuilder);
        when(createIndexRequestBuilder.addMapping(any(String.class), any(XContentBuilder.class))).thenReturn(createIndexRequestBuilder);
        when(createIndexRequestBuilder.get()).thenReturn(response);
        when(indicesAdminClient.prepareCreate(eq(index))).thenReturn(createIndexRequestBuilder);
        return this;
    }

    @SuppressWarnings({ "rawtypes", "unchecked" })
    public MockClientBuilder createIndexRequest(ArgumentCaptor<CreateIndexRequest> requestCapture, final String index) {

        doAnswer(invocation -> {
            CreateIndexResponse response = new CreateIndexResponse(true, true, index) {};
            ((ActionListener) invocation.getArguments()[1]).onResponse(response);
            return null;
        }).when(indicesAdminClient).create(requestCapture.capture(), any(ActionListener.class));
        return this;
    }

    @SuppressWarnings("unchecked")
    public MockClientBuilder prepareSearchExecuteListener(String index, SearchResponse response) {
        SearchRequestBuilder builder = mock(SearchRequestBuilder.class);
        when(builder.setTypes(anyString())).thenReturn(builder);
        when(builder.addSort(any(SortBuilder.class))).thenReturn(builder);
        when(builder.setFetchSource(anyBoolean())).thenReturn(builder);
        when(builder.setScroll(anyString())).thenReturn(builder);
        when(builder.addDocValueField(any(String.class))).thenReturn(builder);
        when(builder.addDocValueField(any(String.class), any(String.class))).thenReturn(builder);
        when(builder.addSort(any(String.class), any(SortOrder.class))).thenReturn(builder);
        when(builder.setQuery(any())).thenReturn(builder);
        when(builder.setSize(anyInt())).thenReturn(builder);

        doAnswer(new Answer<Void>() {
            @Override
            public Void answer(InvocationOnMock invocationOnMock) throws Throwable {
                ActionListener<SearchResponse> listener = (ActionListener<SearchResponse>) invocationOnMock.getArguments()[0];
                listener.onResponse(response);
                return null;
            }
        }).when(builder).execute(any());

        when(client.prepareSearch(eq(index))).thenReturn(builder);

        return this;
    }

    @SuppressWarnings("unchecked")
    public MockClientBuilder prepareSearchScrollExecuteListener(SearchResponse response) {
        SearchScrollRequestBuilder builder = mock(SearchScrollRequestBuilder.class);
        when(builder.setScroll(anyString())).thenReturn(builder);
        when(builder.setScrollId(anyString())).thenReturn(builder);

        doAnswer(new Answer<Void>() {
            @Override
            public Void answer(InvocationOnMock invocationOnMock) throws Throwable {
                ActionListener<SearchResponse> listener = (ActionListener<SearchResponse>) invocationOnMock.getArguments()[0];
                listener.onResponse(response);
                return null;
            }
        }).when(builder).execute(any());

        when(client.prepareSearchScroll(anyString())).thenReturn(builder);

        return this;
    }

    public MockClientBuilder prepareSearch(String index, String type, int from, int size, SearchResponse response,
            ArgumentCaptor<QueryBuilder> filter) {
        SearchRequestBuilder builder = mock(SearchRequestBuilder.class);
        when(builder.setTypes(eq(type))).thenReturn(builder);
        when(builder.addSort(any(SortBuilder.class))).thenReturn(builder);
        when(builder.setQuery(filter.capture())).thenReturn(builder);
        when(builder.setPostFilter(filter.capture())).thenReturn(builder);
        when(builder.setFrom(eq(from))).thenReturn(builder);
        when(builder.setSize(eq(size))).thenReturn(builder);
        when(builder.setFetchSource(eq(true))).thenReturn(builder);
        when(builder.addDocValueField(any(String.class))).thenReturn(builder);
        when(builder.addDocValueField(any(String.class), any(String.class))).thenReturn(builder);
        when(builder.addSort(any(String.class), any(SortOrder.class))).thenReturn(builder);
        when(builder.get()).thenReturn(response);
        when(client.prepareSearch(eq(index))).thenReturn(builder);
        return this;
    }

    public MockClientBuilder prepareSearchAnySize(String index, String type, SearchResponse response, ArgumentCaptor<QueryBuilder> filter) {
        SearchRequestBuilder builder = mock(SearchRequestBuilder.class);
        when(builder.setTypes(eq(type))).thenReturn(builder);
        when(builder.addSort(any(SortBuilder.class))).thenReturn(builder);
        when(builder.setQuery(filter.capture())).thenReturn(builder);
        when(builder.setPostFilter(filter.capture())).thenReturn(builder);
        when(builder.setFrom(any(Integer.class))).thenReturn(builder);
        when(builder.setSize(any(Integer.class))).thenReturn(builder);
        when(builder.setFetchSource(eq(true))).thenReturn(builder);
        when(builder.addDocValueField(any(String.class))).thenReturn(builder);
        when(builder.addDocValueField(any(String.class), any(String.class))).thenReturn(builder);
        when(builder.addSort(any(String.class), any(SortOrder.class))).thenReturn(builder);
        when(builder.get()).thenReturn(response);
        when(client.prepareSearch(eq(index))).thenReturn(builder);
        return this;
    }

    @SuppressWarnings("unchecked")
    public MockClientBuilder prepareIndex(String index, String type, String responseId, ArgumentCaptor<XContentBuilder> getSource) {
        IndexRequestBuilder builder = mock(IndexRequestBuilder.class);
        PlainActionFuture<IndexResponse> actionFuture = mock(PlainActionFuture.class);
        IndexResponse response = mock(IndexResponse.class);
        when(response.getId()).thenReturn(responseId);

        when(client.prepareIndex(eq(index), eq(type))).thenReturn(builder);
        when(client.prepareIndex(eq(index), eq(type), any(String.class))).thenReturn(builder);
        when(builder.setSource(getSource.capture())).thenReturn(builder);
        when(builder.setRefreshPolicy(eq(RefreshPolicy.IMMEDIATE))).thenReturn(builder);
        when(builder.execute()).thenReturn(actionFuture);
        when(actionFuture.actionGet()).thenReturn(response);
        return this;
    }

    @SuppressWarnings("unchecked")
    public MockClientBuilder prepareAlias(String indexName, String alias, QueryBuilder filter) {
        when(aliasesRequestBuilder.addAlias(eq(indexName), eq(alias), eq(filter))).thenReturn(aliasesRequestBuilder);
        when(indicesAdminClient.prepareAliases()).thenReturn(aliasesRequestBuilder);
        doAnswer(new Answer<Void>() {
            @Override
            public Void answer(InvocationOnMock invocationOnMock) throws Throwable {
                ActionListener<IndicesAliasesResponse> listener =
                        (ActionListener<IndicesAliasesResponse>) invocationOnMock.getArguments()[0];
                listener.onResponse(mock(IndicesAliasesResponse.class));
                return null;
            }
        }).when(aliasesRequestBuilder).execute(any());
        return this;
    }

    @SuppressWarnings("unchecked")
    public MockClientBuilder prepareAlias(String indexName, String alias) {
        when(aliasesRequestBuilder.addAlias(eq(indexName), eq(alias))).thenReturn(aliasesRequestBuilder);
        when(indicesAdminClient.prepareAliases()).thenReturn(aliasesRequestBuilder);
        doAnswer(new Answer<Void>() {
            @Override
            public Void answer(InvocationOnMock invocationOnMock) throws Throwable {
                ActionListener<IndicesAliasesResponse> listener =
                        (ActionListener<IndicesAliasesResponse>) invocationOnMock.getArguments()[1];
                listener.onResponse(mock(IndicesAliasesResponse.class));
                return null;
            }
        }).when(indicesAdminClient).aliases(any(IndicesAliasesRequest.class), any(ActionListener.class));
        return this;
    }

    @SuppressWarnings("unchecked")
    public MockClientBuilder prepareBulk(BulkResponse response) {
        PlainActionFuture<BulkResponse> actionFuture = mock(PlainActionFuture.class);
        BulkRequestBuilder builder = mock(BulkRequestBuilder.class);
        when(client.prepareBulk()).thenReturn(builder);
        when(builder.execute()).thenReturn(actionFuture);
        when(actionFuture.actionGet()).thenReturn(response);
        return this;
    }

    @SuppressWarnings("unchecked")
    public MockClientBuilder bulk(BulkResponse response) {
        ActionFuture<BulkResponse> actionFuture = mock(ActionFuture.class);
        when(client.bulk(any(BulkRequest.class))).thenReturn(actionFuture);
        when(actionFuture.actionGet()).thenReturn(response);
        return this;
    }

    public MockClientBuilder preparePutMapping(PutMappingResponse response, String type) {
        PutMappingRequestBuilder requestBuilder = mock(PutMappingRequestBuilder.class);
        when(requestBuilder.setType(eq(type))).thenReturn(requestBuilder);
        when(requestBuilder.setSource(any(XContentBuilder.class))).thenReturn(requestBuilder);
        doAnswer(new Answer<Void>() {
            @Override
            public Void answer(InvocationOnMock invocationOnMock) throws Throwable {
                ActionListener<PutMappingResponse> listener =
                        (ActionListener<PutMappingResponse>) invocationOnMock.getArguments()[0];
                listener.onResponse(response);
                return null;
            }
        }).when(requestBuilder).execute(any());

        when(indicesAdminClient.preparePutMapping(any())).thenReturn(requestBuilder);
        return this;
    }

    public MockClientBuilder prepareGetMapping(GetMappingsResponse response) {
        GetMappingsRequestBuilder builder = mock(GetMappingsRequestBuilder.class);

        doAnswer(new Answer<Void>() {
            @Override
            public Void answer(InvocationOnMock invocationOnMock) throws Throwable {
                ActionListener<GetMappingsResponse> listener =
                        (ActionListener<GetMappingsResponse>) invocationOnMock.getArguments()[0];
                listener.onResponse(response);
                return null;
            }
        }).when(builder).execute(any());

        when(indicesAdminClient.prepareGetMappings(any())).thenReturn(builder);
        return this;
    }

    public MockClientBuilder putTemplate(ArgumentCaptor<PutIndexTemplateRequest> requestCaptor) {
        doAnswer(new Answer<Void>() {
            @Override
            public Void answer(InvocationOnMock invocationOnMock) throws Throwable {
                ActionListener<PutIndexTemplateResponse> listener =
                        (ActionListener<PutIndexTemplateResponse>) invocationOnMock.getArguments()[1];
                listener.onResponse(mock(PutIndexTemplateResponse.class));
                return null;
            }
        }).when(indicesAdminClient).putTemplate(requestCaptor.capture(), any(ActionListener.class));
        return this;
    }


    public Client build() {
        return client;
    }

    public void verifyIndexCreated(String index) {
        ArgumentCaptor<CreateIndexRequest> requestCaptor = ArgumentCaptor.forClass(CreateIndexRequest.class);
        verify(indicesAdminClient).create(requestCaptor.capture(), any());
        assertEquals(index, requestCaptor.getValue().index());
    }

    public void resetIndices() {
        reset(indicesAdminClient);
    }

}
