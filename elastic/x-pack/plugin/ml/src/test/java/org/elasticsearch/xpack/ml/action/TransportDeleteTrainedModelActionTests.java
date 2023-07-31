/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.action;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.cluster.node.tasks.cancel.CancelTasksAction;
import org.elasticsearch.action.admin.cluster.node.tasks.cancel.CancelTasksRequestBuilder;
import org.elasticsearch.action.admin.cluster.node.tasks.cancel.CancelTasksResponse;
import org.elasticsearch.action.admin.cluster.node.tasks.list.ListTasksAction;
import org.elasticsearch.action.admin.cluster.node.tasks.list.ListTasksResponse;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.threadpool.TestThreadPool;
import org.elasticsearch.threadpool.ThreadPool;
import org.junit.After;
import org.junit.Before;

import java.util.Collections;
import java.util.concurrent.TimeUnit;

import static org.elasticsearch.xpack.ml.action.TransportDeleteTrainedModelAction.cancelDownloadTask;
import static org.elasticsearch.xpack.ml.utils.TaskRetrieverTests.getTaskInfoListOfOne;
import static org.elasticsearch.xpack.ml.utils.TaskRetrieverTests.mockClientWithTasksResponse;
import static org.elasticsearch.xpack.ml.utils.TaskRetrieverTests.mockListTasksClient;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.nullValue;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.same;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class TransportDeleteTrainedModelActionTests extends ESTestCase {
    private static final TimeValue TIMEOUT = new TimeValue(30, TimeUnit.SECONDS);

    private ThreadPool threadPool;

    @Before
    public void setUpThreadPool() {
        threadPool = new TestThreadPool(getTestName());
    }

    @After
    public void tearDownThreadPool() {
        terminate(threadPool);
    }

    public void testCancelDownloadTaskCallsListenerWithNullWhenNoTasksExist() {
        var client = mockClientWithTasksResponse(Collections.emptyList(), threadPool);
        var listener = new PlainActionFuture<CancelTasksResponse>();

        cancelDownloadTask(client, "modelId", listener, TIMEOUT);

        assertThat(listener.actionGet(TIMEOUT), nullValue());
    }

    public void testCancelDownloadTaskCallsOnFailureWithErrorWhenCancellingFailsWithAnError() {
        var client = mockClientWithTasksResponse(getTaskInfoListOfOne(), threadPool);
        mockCancelTask(client);

        doAnswer(invocationOnMock -> {
            @SuppressWarnings("unchecked")
            ActionListener<CancelTasksResponse> listener = (ActionListener<CancelTasksResponse>) invocationOnMock.getArguments()[2];
            listener.onFailure(new Exception("cancel error"));

            return Void.TYPE;
        }).when(client).execute(same(CancelTasksAction.INSTANCE), any(), any());

        var listener = new PlainActionFuture<CancelTasksResponse>();

        cancelDownloadTask(client, "modelId", listener, TIMEOUT);

        var exception = expectThrows(ElasticsearchException.class, () -> listener.actionGet(TIMEOUT));
        assertThat(exception.status(), is(RestStatus.INTERNAL_SERVER_ERROR));
        assertThat(exception.getMessage(), is("Unable to cancel task for model id [modelId]"));
    }

    public void testCancelDownloadTaskCallsOnResponseNullWhenTheTaskNoLongerExistsWhenCancelling() {
        var client = mockClientWithTasksResponse(getTaskInfoListOfOne(), threadPool);
        mockCancelTask(client);

        doAnswer(invocationOnMock -> {
            @SuppressWarnings("unchecked")
            ActionListener<CancelTasksResponse> listener = (ActionListener<CancelTasksResponse>) invocationOnMock.getArguments()[2];
            listener.onFailure(new ResourceNotFoundException("task no longer there"));

            return Void.TYPE;
        }).when(client).execute(same(CancelTasksAction.INSTANCE), any(), any());

        var listener = new PlainActionFuture<CancelTasksResponse>();

        cancelDownloadTask(client, "modelId", listener, TIMEOUT);

        assertThat(listener.actionGet(TIMEOUT), nullValue());
    }

    public void testCancelDownloadTasksCallsGetsUnableToRetrieveTaskInfoError() {
        var client = mockListTasksClient(threadPool);

        doAnswer(invocationOnMock -> {
            @SuppressWarnings("unchecked")
            ActionListener<ListTasksResponse> actionListener = (ActionListener<ListTasksResponse>) invocationOnMock.getArguments()[2];
            actionListener.onFailure(new Exception("error"));

            return Void.TYPE;
        }).when(client).execute(same(ListTasksAction.INSTANCE), any(), any());

        var listener = new PlainActionFuture<CancelTasksResponse>();

        cancelDownloadTask(client, "modelId", listener, TIMEOUT);

        var exception = expectThrows(ElasticsearchException.class, () -> listener.actionGet(TIMEOUT));
        assertThat(exception.status(), is(RestStatus.INTERNAL_SERVER_ERROR));
        assertThat(exception.getMessage(), is("Unable to retrieve existing task information for model id [modelId]"));
    }

    public void testCancelDownloadTaskCallsOnResponseWithTheCancelResponseWhenATaskExists() {
        var client = mockClientWithTasksResponse(getTaskInfoListOfOne(), threadPool);

        var cancelResponse = mock(CancelTasksResponse.class);
        mockCancelTasksResponse(client, cancelResponse);

        var listener = new PlainActionFuture<CancelTasksResponse>();

        cancelDownloadTask(client, "modelId", listener, TIMEOUT);

        assertThat(listener.actionGet(TIMEOUT), is(cancelResponse));
    }

    private static void mockCancelTask(Client client) {
        var cluster = client.admin().cluster();
        when(cluster.prepareCancelTasks()).thenReturn(new CancelTasksRequestBuilder(client, CancelTasksAction.INSTANCE));
    }

    private static void mockCancelTasksResponse(Client client, CancelTasksResponse response) {
        mockCancelTask(client);

        doAnswer(invocationOnMock -> {
            @SuppressWarnings("unchecked")
            ActionListener<CancelTasksResponse> listener = (ActionListener<CancelTasksResponse>) invocationOnMock.getArguments()[2];
            listener.onResponse(response);

            return Void.TYPE;
        }).when(client).execute(same(CancelTasksAction.INSTANCE), any(), any());
    }
}
