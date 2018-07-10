/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.integration;

import org.elasticsearch.action.bulk.BulkRequestBuilder;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.action.update.UpdateAction;
import org.elasticsearch.action.update.UpdateRequest;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.xpack.core.ml.action.DeleteExpiredDataAction;
import org.elasticsearch.xpack.core.ml.action.UpdateModelSnapshotAction;
import org.elasticsearch.xpack.core.ml.datafeed.DatafeedConfig;
import org.elasticsearch.xpack.core.ml.job.config.AnalysisConfig;
import org.elasticsearch.xpack.core.ml.job.config.DataDescription;
import org.elasticsearch.xpack.core.ml.job.config.Detector;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.persistence.AnomalyDetectorsIndex;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSnapshot;
import org.elasticsearch.xpack.core.ml.job.results.Bucket;
import org.elasticsearch.xpack.core.ml.job.results.ForecastRequestStats;
import org.junit.After;
import org.junit.Before;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.TimeUnit;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;
import static org.hamcrest.Matchers.greaterThanOrEqualTo;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.lessThan;
import static org.hamcrest.Matchers.lessThanOrEqualTo;

public class DeleteExpiredDataIT extends MlNativeAutodetectIntegTestCase {

    private static final String DATA_INDEX = "delete-expired-data-test-data";
    private static final String DATA_TYPE = "my_type";

    @Before
    public void setUpData() throws IOException {
        client().admin().indices().prepareCreate(DATA_INDEX)
                .addMapping(DATA_TYPE, "time", "type=date,format=epoch_millis")
                .get();

        // We are going to create data for last 2 days
        long nowMillis = System.currentTimeMillis();
        int totalBuckets = 3 * 24;
        int normalRate = 10;
        int anomalousRate = 100;
        int anomalousBucket = 30;
        BulkRequestBuilder bulkRequestBuilder = client().prepareBulk();
        for (int bucket = 0; bucket < totalBuckets; bucket++) {
            long timestamp = nowMillis - TimeValue.timeValueHours(totalBuckets - bucket).getMillis();
            int bucketRate = bucket == anomalousBucket ? anomalousRate : normalRate;
            for (int point = 0; point < bucketRate; point++) {
                IndexRequest indexRequest = new IndexRequest(DATA_INDEX, DATA_TYPE);
                indexRequest.source("time", timestamp);
                bulkRequestBuilder.add(indexRequest);
            }
        }

        BulkResponse bulkResponse = bulkRequestBuilder
                .setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE)
                .get();
        assertThat(bulkResponse.hasFailures(), is(false));

        // Ensure all data is searchable
        client().admin().indices().prepareRefresh(DATA_INDEX).get();
    }

    @After
    public void tearDownData() {
        client().admin().indices().prepareDelete(DATA_INDEX).get();
        cleanUp();
    }

    public void testDeleteExpiredDataGivenNothingToDelete() throws Exception {
        // Tests that nothing goes wrong when there's nothing to delete
        client().execute(DeleteExpiredDataAction.INSTANCE, new DeleteExpiredDataAction.Request()).get();
    }

    public void testDeleteExpiredData() throws Exception {
        registerJob(newJobBuilder("no-retention").setResultsRetentionDays(null).setModelSnapshotRetentionDays(null));
        registerJob(newJobBuilder("results-retention").setResultsRetentionDays(1L).setModelSnapshotRetentionDays(null));
        registerJob(newJobBuilder("snapshots-retention").setResultsRetentionDays(null).setModelSnapshotRetentionDays(2L));
        registerJob(newJobBuilder("snapshots-retention-with-retain").setResultsRetentionDays(null).setModelSnapshotRetentionDays(2L));
        registerJob(newJobBuilder("results-and-snapshots-retention").setResultsRetentionDays(1L).setModelSnapshotRetentionDays(2L));

        List<String> shortExpiryForecastIds = new ArrayList<>();

        long now = System.currentTimeMillis();
        long oneDayAgo = now - TimeValue.timeValueHours(48).getMillis() - 1;
        for (Job.Builder job : getJobs()) {
            putJob(job);

            String datafeedId = job.getId() + "-feed";
            DatafeedConfig.Builder datafeedConfig = new DatafeedConfig.Builder(datafeedId, job.getId());
            datafeedConfig.setIndices(Arrays.asList(DATA_INDEX));
            datafeedConfig.setTypes(Arrays.asList(DATA_TYPE));
            DatafeedConfig datafeed = datafeedConfig.build();
            registerDatafeed(datafeed);
            putDatafeed(datafeed);

            // Run up to a day ago
            openJob(job.getId());
            startDatafeed(datafeedId, 0, now - TimeValue.timeValueHours(24).getMillis());
            waitUntilJobIsClosed(job.getId());
            assertThat(getBuckets(job.getId()).size(), is(greaterThanOrEqualTo(47)));
            assertThat(getRecords(job.getId()).size(), equalTo(1));
            List<ModelSnapshot> modelSnapshots = getModelSnapshots(job.getId());
            assertThat(modelSnapshots.size(), equalTo(1));
            String snapshotDocId = ModelSnapshot.documentId(modelSnapshots.get(0));

            // Update snapshot timestamp to force it out of snapshot retention window
            String snapshotUpdate = "{ \"timestamp\": " + oneDayAgo + "}";
            UpdateRequest updateSnapshotRequest = new UpdateRequest(".ml-anomalies-" + job.getId(), "doc", snapshotDocId);
            updateSnapshotRequest.doc(snapshotUpdate.getBytes(StandardCharsets.UTF_8), XContentType.JSON);
            client().execute(UpdateAction.INSTANCE, updateSnapshotRequest).get();

            // Now let's create some forecasts
            openJob(job.getId());

            // We must set a very small value for expires_in to keep this testable as the deletion cutoff point is the moment
            // the DeleteExpiredDataAction is called.
            String forecastShortExpiryId = forecast(job.getId(), TimeValue.timeValueHours(3), TimeValue.timeValueSeconds(1));
            shortExpiryForecastIds.add(forecastShortExpiryId);
            String forecastDefaultExpiryId = forecast(job.getId(), TimeValue.timeValueHours(3), null);
            String forecastNoExpiryId = forecast(job.getId(), TimeValue.timeValueHours(3), TimeValue.ZERO);
            waitForecastToFinish(job.getId(), forecastShortExpiryId);
            waitForecastToFinish(job.getId(), forecastDefaultExpiryId);
            waitForecastToFinish(job.getId(), forecastNoExpiryId);
        }
        // Refresh to ensure the snapshot timestamp updates are visible
        client().admin().indices().prepareRefresh("*").get();

        // We need to wait a second to ensure the second time around model snapshots will have a different ID (it depends on epoch seconds)
        awaitBusy(() -> false, 1, TimeUnit.SECONDS);

        for (Job.Builder job : getJobs()) {
            // Run up to now
            startDatafeed(job.getId() + "-feed", 0, now);
            waitUntilJobIsClosed(job.getId());
            assertThat(getBuckets(job.getId()).size(), is(greaterThanOrEqualTo(70)));
            assertThat(getRecords(job.getId()).size(), equalTo(1));
            List<ModelSnapshot> modelSnapshots = getModelSnapshots(job.getId());
            assertThat(modelSnapshots.size(), equalTo(2));
        }

        retainAllSnapshots("snapshots-retention-with-retain");

        long totalModelSizeStatsBeforeDelete = client().prepareSearch("*")
                .setQuery(QueryBuilders.termQuery("result_type", "model_size_stats"))
                .get().getHits().totalHits;
        long totalNotificationsCountBeforeDelete = client().prepareSearch(".ml-notifications").get().getHits().totalHits;
        assertThat(totalModelSizeStatsBeforeDelete, greaterThan(0L));
        assertThat(totalNotificationsCountBeforeDelete, greaterThan(0L));

        // Verify forecasts were created
        List<ForecastRequestStats> forecastStats = getForecastStats();
        assertThat(forecastStats.size(), equalTo(getJobs().size() * 3));
        for (ForecastRequestStats forecastStat : forecastStats) {
            assertThat(countForecastDocs(forecastStat.getJobId(), forecastStat.getForecastId()), equalTo(forecastStat.getRecordCount()));
        }

        // Index some unused state documents (more than 10K to test scrolling works)
        BulkRequestBuilder bulkRequestBuilder = client().prepareBulk();
        bulkRequestBuilder.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);
        for (int i = 0; i < 10010; i++) {
            String docId = "non_existing_job_" + randomFrom("model_state_1234567#" + i, "quantiles", "categorizer_state#" + i);
            IndexRequest indexRequest = new IndexRequest(AnomalyDetectorsIndex.jobStateIndexName(), "doc", docId);
            indexRequest.source(Collections.emptyMap());
            bulkRequestBuilder.add(indexRequest);
        }
        assertThat(bulkRequestBuilder.get().status(), equalTo(RestStatus.OK));

        // Now call the action under test
        client().execute(DeleteExpiredDataAction.INSTANCE, new DeleteExpiredDataAction.Request()).get();

        // We need to refresh to ensure the deletion is visible
        client().admin().indices().prepareRefresh("*").get();

        // no-retention job should have kept all data
        assertThat(getBuckets("no-retention").size(), is(greaterThanOrEqualTo(70)));
        assertThat(getRecords("no-retention").size(), equalTo(1));
        assertThat(getModelSnapshots("no-retention").size(), equalTo(2));

        List<Bucket> buckets = getBuckets("results-retention");
        assertThat(buckets.size(), is(lessThanOrEqualTo(24)));
        assertThat(buckets.size(), is(greaterThanOrEqualTo(22)));
        assertThat(buckets.get(0).getTimestamp().getTime(), greaterThanOrEqualTo(oneDayAgo));
        assertThat(getRecords("results-retention").size(), equalTo(0));
        assertThat(getModelSnapshots("results-retention").size(), equalTo(2));

        assertThat(getBuckets("snapshots-retention").size(), is(greaterThanOrEqualTo(70)));
        assertThat(getRecords("snapshots-retention").size(), equalTo(1));
        assertThat(getModelSnapshots("snapshots-retention").size(), equalTo(1));

        assertThat(getBuckets("snapshots-retention-with-retain").size(), is(greaterThanOrEqualTo(70)));
        assertThat(getRecords("snapshots-retention-with-retain").size(), equalTo(1));
        assertThat(getModelSnapshots("snapshots-retention-with-retain").size(), equalTo(2));

        buckets = getBuckets("results-and-snapshots-retention");
        assertThat(buckets.size(), is(lessThanOrEqualTo(24)));
        assertThat(buckets.size(), is(greaterThanOrEqualTo(22)));
        assertThat(buckets.get(0).getTimestamp().getTime(), greaterThanOrEqualTo(oneDayAgo));
        assertThat(getRecords("results-and-snapshots-retention").size(), equalTo(0));
        assertThat(getModelSnapshots("results-and-snapshots-retention").size(), equalTo(1));

        long totalModelSizeStatsAfterDelete = client().prepareSearch("*")
                .setQuery(QueryBuilders.termQuery("result_type", "model_size_stats"))
                .get().getHits().totalHits;
        long totalNotificationsCountAfterDelete = client().prepareSearch(".ml-notifications").get().getHits().totalHits;
        assertThat(totalModelSizeStatsAfterDelete, equalTo(totalModelSizeStatsBeforeDelete));
        assertThat(totalNotificationsCountAfterDelete, greaterThanOrEqualTo(totalNotificationsCountBeforeDelete));

        // Verify short expiry forecasts were deleted only
        forecastStats = getForecastStats();
        assertThat(forecastStats.size(), equalTo(getJobs().size() * 2));
        for (ForecastRequestStats forecastStat : forecastStats) {
            assertThat(countForecastDocs(forecastStat.getJobId(), forecastStat.getForecastId()), equalTo(forecastStat.getRecordCount()));
        }
        for (Job.Builder job : getJobs()) {
            for (String forecastId : shortExpiryForecastIds) {
                assertThat(countForecastDocs(job.getId(), forecastId), equalTo(0L));
            }
        }

        // Verify .ml-state doesn't contain unused state documents
        SearchResponse stateDocsResponse = client().prepareSearch(AnomalyDetectorsIndex.jobStateIndexName())
                .setFetchSource(false)
                .setSize(10000)
                .get();
        assertThat(stateDocsResponse.getHits().getTotalHits(), lessThan(10000L));
        for (SearchHit hit : stateDocsResponse.getHits().getHits()) {
            assertThat(hit.getId().startsWith("non_existing_job"), is(false));
        }
    }

    private static Job.Builder newJobBuilder(String id) {
        Detector.Builder detector = new Detector.Builder();
        detector.setFunction("count");
        AnalysisConfig.Builder analysisConfig = new AnalysisConfig.Builder(Arrays.asList(detector.build()));
        analysisConfig.setBucketSpan(TimeValue.timeValueHours(1));
        DataDescription.Builder dataDescription = new DataDescription.Builder();
        dataDescription.setTimeField("time");
        Job.Builder jobBuilder = new Job.Builder(id);
        jobBuilder.setAnalysisConfig(analysisConfig);
        jobBuilder.setDataDescription(dataDescription);
        return jobBuilder;
    }

    private void retainAllSnapshots(String jobId) throws Exception {
        List<ModelSnapshot> modelSnapshots = getModelSnapshots(jobId);
        for (ModelSnapshot modelSnapshot : modelSnapshots) {
            UpdateModelSnapshotAction.Request request = new UpdateModelSnapshotAction.Request(jobId, modelSnapshot.getSnapshotId());
            request.setRetain(true);
            client().execute(UpdateModelSnapshotAction.INSTANCE, request).get();
        }
        // We need to refresh to ensure the updates are visible
        client().admin().indices().prepareRefresh("*").get();
    }
}
