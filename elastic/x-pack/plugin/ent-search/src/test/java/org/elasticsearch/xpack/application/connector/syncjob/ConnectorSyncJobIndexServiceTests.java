/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.application.connector.syncjob;

import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.ActionFuture;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.DocWriteRequest;
import org.elasticsearch.action.DocWriteResponse;
import org.elasticsearch.action.delete.DeleteResponse;
import org.elasticsearch.action.get.GetRequest;
import org.elasticsearch.action.get.GetResponse;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.action.update.UpdateResponse;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.test.ESSingleNodeTestCase;
import org.elasticsearch.xcontent.ParseField;
import org.elasticsearch.xcontent.ToXContent;
import org.elasticsearch.xpack.application.connector.Connector;
import org.elasticsearch.xpack.application.connector.ConnectorIndexService;
import org.elasticsearch.xpack.application.connector.ConnectorSyncStatus;
import org.elasticsearch.xpack.application.connector.ConnectorTestUtils;
import org.elasticsearch.xpack.application.connector.syncjob.action.PostConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.UpdateConnectorSyncJobErrorAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.UpdateConnectorSyncJobIngestionStatsAction;
import org.junit.Before;

import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import java.util.concurrent.atomic.AtomicReference;
import java.util.stream.Collectors;

import static org.elasticsearch.xcontent.XContentFactory.jsonBuilder;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThanOrEqualTo;
import static org.hamcrest.Matchers.not;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;

public class ConnectorSyncJobIndexServiceTests extends ESSingleNodeTestCase {

    private static final String NON_EXISTING_CONNECTOR_ID = "non-existing-connector-id";
    private static final String NON_EXISTING_SYNC_JOB_ID = "non-existing-sync-job-id";
    private static final String LAST_SEEN_FIELD_NAME = ConnectorSyncJob.LAST_SEEN_FIELD.getPreferredName();
    private static final int TIMEOUT_SECONDS = 10;
    private static final int ONE_SECOND_IN_MILLIS = 1000;

    private ConnectorSyncJobIndexService connectorSyncJobIndexService;
    private Connector connector;

    @Before
    public void setup() throws Exception {
        connector = ConnectorTestUtils.getRandomSyncJobConnectorInfo();

        final IndexRequest indexRequest = new IndexRequest(ConnectorIndexService.CONNECTOR_INDEX_NAME).opType(DocWriteRequest.OpType.INDEX)
            .id(connector.getConnectorId())
            .setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE)
            .source(connector.toXContent(jsonBuilder(), ToXContent.EMPTY_PARAMS));
        ActionFuture<DocWriteResponse> index = client().index(indexRequest);

        // wait 10 seconds for connector creation
        index.get(TIMEOUT_SECONDS, TimeUnit.SECONDS);

        this.connectorSyncJobIndexService = new ConnectorSyncJobIndexService(client());
    }

    public void testCreateConnectorSyncJob() throws Exception {
        PostConnectorSyncJobAction.Request syncJobRequest = ConnectorSyncJobTestUtils.getRandomPostConnectorSyncJobActionRequest(
            connector.getConnectorId()
        );
        ConnectorSyncJobType requestJobType = syncJobRequest.getJobType();
        ConnectorSyncJobTriggerMethod requestTriggerMethod = syncJobRequest.getTriggerMethod();
        PostConnectorSyncJobAction.Response response = awaitPutConnectorSyncJob(syncJobRequest);

        ConnectorSyncJob connectorSyncJob = awaitGetConnectorSyncJob(response.getId());

        assertThat(connectorSyncJob.getId(), notNullValue());
        assertThat(connectorSyncJob.getJobType(), equalTo(requestJobType));
        assertThat(connectorSyncJob.getTriggerMethod(), equalTo(requestTriggerMethod));
        assertThat(connectorSyncJob.getStatus(), equalTo(ConnectorSyncJob.DEFAULT_INITIAL_STATUS));
        assertThat(connectorSyncJob.getCreatedAt(), equalTo(connectorSyncJob.getLastSeen()));
        assertThat(connectorSyncJob.getTotalDocumentCount(), equalTo(0L));
        assertThat(connectorSyncJob.getIndexedDocumentCount(), equalTo(0L));
        assertThat(connectorSyncJob.getIndexedDocumentVolume(), equalTo(0L));
        assertThat(connectorSyncJob.getDeletedDocumentCount(), equalTo(0L));
    }

    public void testCreateConnectorSyncJob_WithMissingJobType_ExpectDefaultJobTypeToBeSet() throws Exception {
        PostConnectorSyncJobAction.Request syncJobRequest = new PostConnectorSyncJobAction.Request(
            connector.getConnectorId(),
            null,
            ConnectorSyncJobTriggerMethod.ON_DEMAND
        );
        PostConnectorSyncJobAction.Response response = awaitPutConnectorSyncJob(syncJobRequest);

        ConnectorSyncJob connectorSyncJob = awaitGetConnectorSyncJob(response.getId());

        assertThat(connectorSyncJob.getJobType(), equalTo(ConnectorSyncJob.DEFAULT_JOB_TYPE));
    }

    public void testCreateConnectorSyncJob_WithMissingTriggerMethod_ExpectDefaultTriggerMethodToBeSet() throws Exception {
        PostConnectorSyncJobAction.Request syncJobRequest = new PostConnectorSyncJobAction.Request(
            connector.getConnectorId(),
            ConnectorSyncJobType.FULL,
            null
        );
        PostConnectorSyncJobAction.Response response = awaitPutConnectorSyncJob(syncJobRequest);

        ConnectorSyncJob connectorSyncJob = awaitGetConnectorSyncJob(response.getId());

        assertThat(connectorSyncJob.getTriggerMethod(), equalTo(ConnectorSyncJob.DEFAULT_TRIGGER_METHOD));
    }

    public void testCreateConnectorSyncJob_WithMissingConnectorId_ExpectException() throws Exception {
        PostConnectorSyncJobAction.Request syncJobRequest = new PostConnectorSyncJobAction.Request(
            NON_EXISTING_CONNECTOR_ID,
            ConnectorSyncJobType.FULL,
            ConnectorSyncJobTriggerMethod.ON_DEMAND
        );
        awaitPutConnectorSyncJobExpectingException(
            syncJobRequest,
            ActionListener.wrap(response -> {}, exception -> assertThat(exception.getMessage(), containsString(NON_EXISTING_CONNECTOR_ID)))
        );
    }

    public void testDeleteConnectorSyncJob() throws Exception {
        PostConnectorSyncJobAction.Request syncJobRequest = ConnectorSyncJobTestUtils.getRandomPostConnectorSyncJobActionRequest(
            connector.getConnectorId()
        );
        PostConnectorSyncJobAction.Response response = awaitPutConnectorSyncJob(syncJobRequest);
        String syncJobId = response.getId();

        assertThat(syncJobId, notNullValue());

        DeleteResponse deleteResponse = awaitDeleteConnectorSyncJob(syncJobId);

        assertThat(deleteResponse.status(), equalTo(RestStatus.OK));
    }

    public void testDeleteConnectorSyncJob_WithMissingSyncJobId_ExpectException() {
        expectThrows(ResourceNotFoundException.class, () -> awaitDeleteConnectorSyncJob(NON_EXISTING_SYNC_JOB_ID));
    }

    public void testGetConnectorSyncJob() throws Exception {
        PostConnectorSyncJobAction.Request syncJobRequest = ConnectorSyncJobTestUtils.getRandomPostConnectorSyncJobActionRequest(
            connector.getConnectorId()
        );
        ConnectorSyncJobType jobType = syncJobRequest.getJobType();
        ConnectorSyncJobTriggerMethod triggerMethod = syncJobRequest.getTriggerMethod();

        PostConnectorSyncJobAction.Response response = awaitPutConnectorSyncJob(syncJobRequest);
        String syncJobId = response.getId();

        ConnectorSyncJob syncJob = awaitGetConnectorSyncJob(syncJobId);

        assertThat(syncJob.getId(), equalTo(syncJobId));
        assertThat(syncJob.getJobType(), equalTo(jobType));
        assertThat(syncJob.getTriggerMethod(), equalTo(triggerMethod));
        assertThat(syncJob.getConnector().getConnectorId(), equalTo(connector.getConnectorId()));
    }

    public void testGetConnectorSyncJob_WithMissingSyncJobId_ExpectException() {
        expectThrows(ResourceNotFoundException.class, () -> awaitGetConnectorSyncJob(NON_EXISTING_SYNC_JOB_ID));
    }

    public void testCheckInConnectorSyncJob() throws Exception {
        PostConnectorSyncJobAction.Request syncJobRequest = ConnectorSyncJobTestUtils.getRandomPostConnectorSyncJobActionRequest(
            connector.getConnectorId()
        );
        PostConnectorSyncJobAction.Response response = awaitPutConnectorSyncJob(syncJobRequest);
        String syncJobId = response.getId();

        Map<String, Object> syncJobSourceBeforeUpdate = getConnectorSyncJobSourceById(syncJobId);
        Instant lastSeenBeforeUpdate = Instant.parse((String) syncJobSourceBeforeUpdate.get(LAST_SEEN_FIELD_NAME));

        safeSleep(ONE_SECOND_IN_MILLIS);

        UpdateResponse updateResponse = awaitCheckInConnectorSyncJob(syncJobId);
        Map<String, Object> syncJobSourceAfterUpdate = getConnectorSyncJobSourceById(syncJobId);
        Instant lastSeenAfterUpdate = Instant.parse((String) syncJobSourceAfterUpdate.get(LAST_SEEN_FIELD_NAME));
        long secondsBetweenLastSeenBeforeAndAfterUpdate = ChronoUnit.SECONDS.between(lastSeenBeforeUpdate, lastSeenAfterUpdate);

        assertThat("Wrong sync job was updated", syncJobId, equalTo(updateResponse.getId()));
        assertThat(updateResponse.status(), equalTo(RestStatus.OK));
        assertTrue(
            "[" + LAST_SEEN_FIELD_NAME + "] after the check in is not after [" + LAST_SEEN_FIELD_NAME + "] before the check in",
            lastSeenAfterUpdate.isAfter(lastSeenBeforeUpdate)
        );
        assertThat(
            "there must be at least one second between ["
                + LAST_SEEN_FIELD_NAME
                + "] after the check in and ["
                + LAST_SEEN_FIELD_NAME
                + "] before the check in",
            secondsBetweenLastSeenBeforeAndAfterUpdate,
            greaterThanOrEqualTo(1L)
        );
        assertFieldsExceptLastSeenDidNotUpdate(syncJobSourceBeforeUpdate, syncJobSourceAfterUpdate);
    }

    public void testCheckInConnectorSyncJob_WithMissingSyncJobId_ExpectException() {
        expectThrows(ResourceNotFoundException.class, () -> awaitCheckInConnectorSyncJob(NON_EXISTING_SYNC_JOB_ID));
    }

    public void testCancelConnectorSyncJob() throws Exception {
        PostConnectorSyncJobAction.Request syncJobRequest = ConnectorSyncJobTestUtils.getRandomPostConnectorSyncJobActionRequest(
            connector.getConnectorId()
        );
        PostConnectorSyncJobAction.Response response = awaitPutConnectorSyncJob(syncJobRequest);
        String syncJobId = response.getId();
        Map<String, Object> syncJobSourceBeforeUpdate = getConnectorSyncJobSourceById(syncJobId);
        ConnectorSyncStatus syncStatusBeforeUpdate = ConnectorSyncStatus.fromString(
            (String) syncJobSourceBeforeUpdate.get(ConnectorSyncJob.STATUS_FIELD.getPreferredName())
        );
        Object cancellationRequestedAtBeforeUpdate = syncJobSourceBeforeUpdate.get(
            ConnectorSyncJob.CANCELATION_REQUESTED_AT_FIELD.getPreferredName()
        );

        assertThat(syncJobId, notNullValue());
        assertThat(cancellationRequestedAtBeforeUpdate, nullValue());
        assertThat(syncStatusBeforeUpdate, not(equalTo(ConnectorSyncStatus.CANCELING)));

        UpdateResponse updateResponse = awaitCancelConnectorSyncJob(syncJobId);

        Map<String, Object> syncJobSourceAfterUpdate = getConnectorSyncJobSourceById(syncJobId);
        ConnectorSyncStatus syncStatusAfterUpdate = ConnectorSyncStatus.fromString(
            (String) syncJobSourceAfterUpdate.get(ConnectorSyncJob.STATUS_FIELD.getPreferredName())
        );
        Instant cancellationRequestedAtAfterUpdate = Instant.parse(
            (String) syncJobSourceAfterUpdate.get(ConnectorSyncJob.CANCELATION_REQUESTED_AT_FIELD.getPreferredName())
        );

        assertThat(updateResponse.status(), equalTo(RestStatus.OK));
        assertThat(cancellationRequestedAtAfterUpdate, notNullValue());
        assertThat(syncStatusAfterUpdate, equalTo(ConnectorSyncStatus.CANCELING));
        assertFieldsExceptSyncStatusAndCancellationRequestedAtDidNotUpdate(syncJobSourceBeforeUpdate, syncJobSourceAfterUpdate);
    }

    public void testCancelConnectorSyncJob_WithMissingSyncJobId_ExpectException() {
        expectThrows(ResourceNotFoundException.class, () -> awaitCancelConnectorSyncJob(NON_EXISTING_SYNC_JOB_ID));
    }

    public void testUpdateConnectorSyncJobError() throws Exception {
        PostConnectorSyncJobAction.Request syncJobRequest = ConnectorSyncJobTestUtils.getRandomPostConnectorSyncJobActionRequest(
            connector.getConnectorId()
        );
        PostConnectorSyncJobAction.Response response = awaitPutConnectorSyncJob(syncJobRequest);
        String syncJobId = response.getId();

        UpdateConnectorSyncJobErrorAction.Request request = ConnectorSyncJobTestUtils.getRandomUpdateConnectorSyncJobErrorActionRequest();
        String errorInRequest = request.getError();

        UpdateResponse updateResponse = awaitUpdateConnectorSyncJob(syncJobId, errorInRequest);
        Map<String, Object> connectorSyncJobSource = getConnectorSyncJobSourceById(syncJobId);
        String error = (String) connectorSyncJobSource.get(ConnectorSyncJob.ERROR_FIELD.getPreferredName());
        ConnectorSyncStatus syncStatus = ConnectorSyncStatus.fromString(
            (String) connectorSyncJobSource.get(ConnectorSyncJob.STATUS_FIELD.getPreferredName())
        );

        assertThat(updateResponse.status(), equalTo(RestStatus.OK));
        assertThat(error, equalTo(errorInRequest));
        assertThat(syncStatus, equalTo(ConnectorSyncStatus.ERROR));
    }

    public void testUpdateConnectorSyncJobError_WithMissingSyncJobId_ExceptException() {
        expectThrows(
            ResourceNotFoundException.class,
            () -> awaitUpdateConnectorSyncJob(NON_EXISTING_SYNC_JOB_ID, randomAlphaOfLengthBetween(5, 100))
        );
    }

    public void testUpdateConnectorSyncJobIngestionStats() throws Exception {
        PostConnectorSyncJobAction.Request syncJobRequest = ConnectorSyncJobTestUtils.getRandomPostConnectorSyncJobActionRequest(
            connector.getConnectorId()
        );
        PostConnectorSyncJobAction.Response response = awaitPutConnectorSyncJob(syncJobRequest);
        String syncJobId = response.getId();
        Map<String, Object> syncJobSourceBeforeUpdate = getConnectorSyncJobSourceById(syncJobId);

        UpdateConnectorSyncJobIngestionStatsAction.Request request = ConnectorSyncJobTestUtils
            .getRandomUpdateConnectorSyncJobIngestionStatsActionRequest(syncJobId);
        UpdateResponse updateResponse = awaitUpdateConnectorSyncJobIngestionStats(request);
        Map<String, Object> syncJobSourceAfterUpdate = getConnectorSyncJobSourceById(syncJobId);

        Long requestDeletedDocumentCount = request.getDeletedDocumentCount();
        Long requestIndexedDocumentCount = request.getIndexedDocumentCount();
        Long requestIndexedDocumentVolume = request.getIndexedDocumentVolume();
        Long requestTotalDocumentCount = request.getTotalDocumentCount();
        Instant requestLastSeen = request.getLastSeen();

        Long deletedDocumentCountAfterUpdate = (Long) syncJobSourceAfterUpdate.get(
            ConnectorSyncJob.DELETED_DOCUMENT_COUNT_FIELD.getPreferredName()
        );
        Long indexedDocumentCountAfterUpdate = (Long) syncJobSourceAfterUpdate.get(
            ConnectorSyncJob.INDEXED_DOCUMENT_COUNT_FIELD.getPreferredName()
        );
        Long indexedDocumentVolumeAfterUpdate = (Long) syncJobSourceAfterUpdate.get(
            ConnectorSyncJob.INDEXED_DOCUMENT_VOLUME_FIELD.getPreferredName()
        );
        Long totalDocumentCountAfterUpdate = (Long) syncJobSourceAfterUpdate.get(
            ConnectorSyncJob.TOTAL_DOCUMENT_COUNT_FIELD.getPreferredName()
        );
        Instant lastSeenAfterUpdate = Instant.parse(
            (String) syncJobSourceAfterUpdate.get(ConnectorSyncJob.LAST_SEEN_FIELD.getPreferredName())
        );

        assertThat(updateResponse.status(), equalTo(RestStatus.OK));
        assertThat(deletedDocumentCountAfterUpdate, equalTo(requestDeletedDocumentCount));
        assertThat(indexedDocumentCountAfterUpdate, equalTo(requestIndexedDocumentCount));
        assertThat(indexedDocumentVolumeAfterUpdate, equalTo(requestIndexedDocumentVolume));
        assertThat(totalDocumentCountAfterUpdate, equalTo(requestTotalDocumentCount));
        assertThat(lastSeenAfterUpdate, equalTo(requestLastSeen));
        assertFieldsExceptAllIngestionStatsDidNotUpdate(syncJobSourceBeforeUpdate, syncJobSourceAfterUpdate);
    }

    public void testUpdateConnectorSyncJobIngestionStats_WithoutLastSeen_ExpectUpdateOfLastSeen() throws Exception {
        PostConnectorSyncJobAction.Request syncJobRequest = ConnectorSyncJobTestUtils.getRandomPostConnectorSyncJobActionRequest(
            connector.getConnectorId()
        );
        PostConnectorSyncJobAction.Response response = awaitPutConnectorSyncJob(syncJobRequest);
        String syncJobId = response.getId();
        Map<String, Object> syncJobSourceBeforeUpdate = getConnectorSyncJobSourceById(syncJobId);
        Instant lastSeenBeforeUpdate = Instant.parse(
            (String) syncJobSourceBeforeUpdate.get(ConnectorSyncJob.LAST_SEEN_FIELD.getPreferredName())
        );
        UpdateConnectorSyncJobIngestionStatsAction.Request request = new UpdateConnectorSyncJobIngestionStatsAction.Request(
            syncJobId,
            10L,
            20L,
            100L,
            10L,
            null
        );

        safeSleep(ONE_SECOND_IN_MILLIS);

        UpdateResponse updateResponse = awaitUpdateConnectorSyncJobIngestionStats(request);
        Map<String, Object> syncJobSourceAfterUpdate = getConnectorSyncJobSourceById(syncJobId);
        Instant lastSeenAfterUpdate = Instant.parse(
            (String) syncJobSourceAfterUpdate.get(ConnectorSyncJob.LAST_SEEN_FIELD.getPreferredName())
        );
        long secondsBetweenLastSeenBeforeAndAfterUpdate = ChronoUnit.SECONDS.between(lastSeenBeforeUpdate, lastSeenAfterUpdate);

        assertThat(updateResponse.status(), equalTo(RestStatus.OK));
        assertTrue(lastSeenAfterUpdate.isAfter(lastSeenBeforeUpdate));
        assertThat(secondsBetweenLastSeenBeforeAndAfterUpdate, greaterThanOrEqualTo(1L));
        assertFieldsExceptAllIngestionStatsDidNotUpdate(syncJobSourceBeforeUpdate, syncJobSourceAfterUpdate);
    }

    public void testUpdateConnectorSyncJobIngestionStats_WithMissingSyncJobId_ExpectException() {
        expectThrows(
            ResourceNotFoundException.class,
            () -> awaitUpdateConnectorSyncJobIngestionStats(
                new UpdateConnectorSyncJobIngestionStatsAction.Request(NON_EXISTING_SYNC_JOB_ID, 0L, 0L, 0L, 0L, Instant.now())
            )
        );
    }

    private UpdateResponse awaitUpdateConnectorSyncJobIngestionStats(UpdateConnectorSyncJobIngestionStatsAction.Request request)
        throws Exception {
        CountDownLatch latch = new CountDownLatch(1);
        final AtomicReference<UpdateResponse> resp = new AtomicReference<>(null);
        final AtomicReference<Exception> exc = new AtomicReference<>(null);
        connectorSyncJobIndexService.updateConnectorSyncJobIngestionStats(request, new ActionListener<>() {
            @Override
            public void onResponse(UpdateResponse updateResponse) {
                resp.set(updateResponse);
                latch.countDown();
            }

            @Override
            public void onFailure(Exception e) {
                exc.set(e);
                latch.countDown();
            }
        });
        assertTrue("Timeout waiting for update request", latch.await(TIMEOUT_SECONDS, TimeUnit.SECONDS));
        if (exc.get() != null) {
            throw exc.get();
        }
        assertNotNull("Received null response from update request", resp.get());
        return resp.get();
    }

    private static void assertFieldsExceptAllIngestionStatsDidNotUpdate(
        Map<String, Object> syncJobSourceBeforeUpdate,
        Map<String, Object> syncJobSourceAfterUpdate
    ) {
        assertFieldsDidNotUpdateExceptFieldList(
            syncJobSourceBeforeUpdate,
            syncJobSourceAfterUpdate,
            List.of(
                ConnectorSyncJob.DELETED_DOCUMENT_COUNT_FIELD,
                ConnectorSyncJob.INDEXED_DOCUMENT_COUNT_FIELD,
                ConnectorSyncJob.INDEXED_DOCUMENT_VOLUME_FIELD,
                ConnectorSyncJob.TOTAL_DOCUMENT_COUNT_FIELD,
                ConnectorSyncJob.LAST_SEEN_FIELD
            )
        );
    }

    private static void assertFieldsExceptSyncStatusAndCancellationRequestedAtDidNotUpdate(
        Map<String, Object> syncJobSourceBeforeUpdate,
        Map<String, Object> syncJobSourceAfterUpdate
    ) {
        assertFieldsDidNotUpdateExceptFieldList(
            syncJobSourceBeforeUpdate,
            syncJobSourceAfterUpdate,
            List.of(ConnectorSyncJob.STATUS_FIELD, ConnectorSyncJob.CANCELATION_REQUESTED_AT_FIELD)
        );
    }

    private static void assertFieldsExceptLastSeenDidNotUpdate(
        Map<String, Object> syncJobSourceBeforeUpdate,
        Map<String, Object> syncJobSourceAfterUpdate
    ) {
        assertFieldsDidNotUpdateExceptFieldList(
            syncJobSourceBeforeUpdate,
            syncJobSourceAfterUpdate,
            List.of(ConnectorSyncJob.LAST_SEEN_FIELD)
        );
    }

    private static void assertFieldsDidNotUpdateExceptFieldList(
        Map<String, Object> syncJobSourceBeforeUpdate,
        Map<String, Object> syncJobSourceAfterUpdate,
        List<ParseField> fieldsWhichShouldUpdate
    ) {
        Set<String> fieldsNamesWhichShouldUpdate = fieldsWhichShouldUpdate.stream()
            .map(ParseField::getPreferredName)
            .collect(Collectors.toSet());

        for (Map.Entry<String, Object> field : syncJobSourceBeforeUpdate.entrySet()) {
            String fieldName = field.getKey();
            boolean isFieldWhichShouldNotUpdate = fieldsNamesWhichShouldUpdate.contains(fieldName) == false;

            if (isFieldWhichShouldNotUpdate) {
                Object fieldValueBeforeUpdate = field.getValue();
                Object fieldValueAfterUpdate = syncJobSourceAfterUpdate.get(fieldName);

                assertThat(
                    "Every field except ["
                        + String.join(",", fieldsNamesWhichShouldUpdate)
                        + "] should stay the same. ["
                        + fieldName
                        + "] did change.",
                    fieldValueBeforeUpdate,
                    equalTo(fieldValueAfterUpdate)
                );
            }
        }
    }

    private UpdateResponse awaitUpdateConnectorSyncJob(String syncJobId, String error) throws Exception {
        CountDownLatch latch = new CountDownLatch(1);
        final AtomicReference<UpdateResponse> resp = new AtomicReference<>(null);
        final AtomicReference<Exception> exc = new AtomicReference<>(null);
        connectorSyncJobIndexService.updateConnectorSyncJobError(syncJobId, error, new ActionListener<>() {
            @Override
            public void onResponse(UpdateResponse updateResponse) {
                resp.set(updateResponse);
                latch.countDown();
            }

            @Override
            public void onFailure(Exception e) {
                exc.set(e);
                latch.countDown();
            }
        });
        assertTrue("Timeout waiting for update request", latch.await(TIMEOUT_SECONDS, TimeUnit.SECONDS));
        if (exc.get() != null) {
            throw exc.get();
        }
        assertNotNull("Received null response from update request", resp.get());
        return resp.get();
    }

    private UpdateResponse awaitCancelConnectorSyncJob(String syncJobId) throws Exception {
        CountDownLatch latch = new CountDownLatch(1);
        final AtomicReference<UpdateResponse> resp = new AtomicReference<>(null);
        final AtomicReference<Exception> exc = new AtomicReference<>(null);
        connectorSyncJobIndexService.cancelConnectorSyncJob(syncJobId, new ActionListener<>() {
            @Override
            public void onResponse(UpdateResponse updateResponse) {
                resp.set(updateResponse);
                latch.countDown();
            }

            @Override
            public void onFailure(Exception e) {
                exc.set(e);
                latch.countDown();
            }
        });
        assertTrue("Timeout waiting for cancel request", latch.await(TIMEOUT_SECONDS, TimeUnit.SECONDS));
        if (exc.get() != null) {
            throw exc.get();
        }
        assertNotNull("Received null response from cancel request", resp.get());
        return resp.get();
    }

    private Map<String, Object> getConnectorSyncJobSourceById(String syncJobId) throws ExecutionException, InterruptedException,
        TimeoutException {
        GetRequest getRequest = new GetRequest(ConnectorSyncJobIndexService.CONNECTOR_SYNC_JOB_INDEX_NAME, syncJobId);
        ActionFuture<GetResponse> getResponseActionFuture = client().get(getRequest);

        return getResponseActionFuture.get(TIMEOUT_SECONDS, TimeUnit.SECONDS).getSource();
    }

    private ConnectorSyncJob awaitGetConnectorSyncJob(String connectorSyncJobId) throws Exception {
        CountDownLatch latch = new CountDownLatch(1);
        final AtomicReference<ConnectorSyncJob> resp = new AtomicReference<>(null);
        final AtomicReference<Exception> exc = new AtomicReference<>(null);

        connectorSyncJobIndexService.getConnectorSyncJob(connectorSyncJobId, new ActionListener<ConnectorSyncJob>() {
            @Override
            public void onResponse(ConnectorSyncJob connectorSyncJob) {
                resp.set(connectorSyncJob);
                latch.countDown();
            }

            @Override
            public void onFailure(Exception e) {
                exc.set(e);
                latch.countDown();
            }
        });

        assertTrue("Timeout waiting for get request", latch.await(TIMEOUT_SECONDS, TimeUnit.SECONDS));
        if (exc.get() != null) {
            throw exc.get();
        }
        assertNotNull("Received null response from get request", resp.get());
        return resp.get();
    }

    private UpdateResponse awaitCheckInConnectorSyncJob(String connectorSyncJobId) throws Exception {
        CountDownLatch latch = new CountDownLatch(1);
        final AtomicReference<UpdateResponse> resp = new AtomicReference<>(null);
        final AtomicReference<Exception> exc = new AtomicReference<>(null);
        connectorSyncJobIndexService.checkInConnectorSyncJob(connectorSyncJobId, new ActionListener<>() {
            @Override
            public void onResponse(UpdateResponse updateResponse) {
                resp.set(updateResponse);
                latch.countDown();
            }

            @Override
            public void onFailure(Exception e) {
                exc.set(e);
                latch.countDown();
            }
        });
        assertTrue("Timeout waiting for check in request", latch.await(TIMEOUT_SECONDS, TimeUnit.SECONDS));
        if (exc.get() != null) {
            throw exc.get();
        }
        assertNotNull("Received null response from check in request", resp.get());
        return resp.get();
    }

    private void awaitPutConnectorSyncJobExpectingException(
        PostConnectorSyncJobAction.Request syncJobRequest,
        ActionListener<PostConnectorSyncJobAction.Response> listener
    ) throws InterruptedException {
        CountDownLatch latch = new CountDownLatch(1);

        connectorSyncJobIndexService.createConnectorSyncJob(syncJobRequest, new ActionListener<>() {
            @Override
            public void onResponse(PostConnectorSyncJobAction.Response putConnectorSyncJobResponse) {
                fail("Expected an exception and not a successful response");
                latch.countDown();
            }

            @Override
            public void onFailure(Exception e) {
                listener.onFailure(e);
                latch.countDown();
            }
        });

        boolean requestTimedOut = latch.await(TIMEOUT_SECONDS, TimeUnit.SECONDS);
        assertTrue("Timeout waiting for put request", requestTimedOut);
    }

    private DeleteResponse awaitDeleteConnectorSyncJob(String connectorSyncJobId) throws Exception {
        CountDownLatch latch = new CountDownLatch(1);
        final AtomicReference<DeleteResponse> resp = new AtomicReference<>(null);
        final AtomicReference<Exception> exc = new AtomicReference<>(null);
        connectorSyncJobIndexService.deleteConnectorSyncJob(connectorSyncJobId, new ActionListener<>() {
            @Override
            public void onResponse(DeleteResponse deleteResponse) {
                resp.set(deleteResponse);
                latch.countDown();
            }

            @Override
            public void onFailure(Exception e) {
                exc.set(e);
                latch.countDown();
            }
        });
        assertTrue("Timeout waiting for delete request", latch.await(TIMEOUT_SECONDS, TimeUnit.SECONDS));
        if (exc.get() != null) {
            throw exc.get();
        }
        assertNotNull("Received null response from delete request", resp.get());
        return resp.get();
    }

    private PostConnectorSyncJobAction.Response awaitPutConnectorSyncJob(PostConnectorSyncJobAction.Request syncJobRequest)
        throws Exception {
        CountDownLatch latch = new CountDownLatch(1);

        final AtomicReference<PostConnectorSyncJobAction.Response> responseRef = new AtomicReference<>(null);
        final AtomicReference<Exception> exception = new AtomicReference<>(null);

        connectorSyncJobIndexService.createConnectorSyncJob(syncJobRequest, new ActionListener<>() {
            @Override
            public void onResponse(PostConnectorSyncJobAction.Response putConnectorSyncJobResponse) {
                responseRef.set(putConnectorSyncJobResponse);
                latch.countDown();
            }

            @Override
            public void onFailure(Exception e) {
                exception.set(e);
                latch.countDown();
            }
        });

        if (exception.get() != null) {
            throw exception.get();
        }

        boolean requestTimedOut = latch.await(TIMEOUT_SECONDS, TimeUnit.SECONDS);
        PostConnectorSyncJobAction.Response response = responseRef.get();

        assertTrue("Timeout waiting for post request", requestTimedOut);
        assertNotNull("Received null response from post request", response);

        return response;
    }

}
