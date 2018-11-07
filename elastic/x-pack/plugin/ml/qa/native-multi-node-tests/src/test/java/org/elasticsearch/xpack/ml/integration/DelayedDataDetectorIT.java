/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.integration;

import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.bulk.BulkItemResponse;
import org.elasticsearch.action.bulk.BulkRequestBuilder;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.index.query.RangeQueryBuilder;
import org.elasticsearch.search.aggregations.AggregationBuilders;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.elasticsearch.search.aggregations.metrics.AvgAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.MaxAggregationBuilder;
import org.elasticsearch.xpack.core.ml.action.GetBucketsAction;
import org.elasticsearch.xpack.core.ml.action.util.PageParams;
import org.elasticsearch.xpack.core.ml.datafeed.DatafeedConfig;
import org.elasticsearch.xpack.core.ml.job.config.AnalysisConfig;
import org.elasticsearch.xpack.core.ml.job.config.DataDescription;
import org.elasticsearch.xpack.core.ml.job.config.Detector;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.results.Bucket;
import org.elasticsearch.xpack.core.ml.job.results.Result;
import org.elasticsearch.xpack.ml.datafeed.DelayedDataDetector;
import org.elasticsearch.xpack.ml.datafeed.DelayedDataDetector.BucketWithMissingData;
import org.junit.After;
import org.junit.Before;

import java.util.Collections;
import java.util.Date;
import java.util.List;

import static org.elasticsearch.xpack.ml.support.BaseMlIntegTestCase.createDatafeed;
import static org.elasticsearch.xpack.ml.support.BaseMlIntegTestCase.createDatafeedBuilder;
import static org.hamcrest.Matchers.equalTo;

public class DelayedDataDetectorIT extends MlNativeAutodetectIntegTestCase {

    private String index = "delayed-data";
    private long now = System.currentTimeMillis();
    private long numDocs;

    @Before
    public void putDataintoIndex() {
        client().admin().indices().prepareCreate(index)
            .addMapping("type", "time", "type=date", "value", "type=long")
            .get();
        numDocs = randomIntBetween(32, 128);
        long oneDayAgo = now - 86400000;
        writeData(logger, index, numDocs, oneDayAgo, now);
    }

    @After
    public void cleanUpTest() {
        cleanUp();
    }

    public void testMissingDataDetection() throws Exception {
        final String jobId = "delayed-data-detection-job";
        Job.Builder job = createJob(jobId, TimeValue.timeValueMinutes(5), "count", null);

        DatafeedConfig datafeedConfig = createDatafeed(job.getId() + "-datafeed", job.getId(), Collections.singletonList(index));
        registerJob(job);
        putJob(job);
        openJob(job.getId());

        registerDatafeed(datafeedConfig);
        putDatafeed(datafeedConfig);
        startDatafeed(datafeedConfig.getId(), 0L, now);
        waitUntilJobIsClosed(jobId);

        // Get the latest finalized bucket
        Bucket lastBucket = getLatestFinalizedBucket(jobId);

        DelayedDataDetector delayedDataDetector =
            new DelayedDataDetector(job.build(new Date()), datafeedConfig, TimeValue.timeValueHours(12), client());

        List<BucketWithMissingData> response = delayedDataDetector.detectMissingData(lastBucket.getEpoch()*1000);
        assertThat(response.stream().mapToLong(BucketWithMissingData::getMissingDocumentCount).sum(), equalTo(0L));

        long missingDocs = randomIntBetween(32, 128);
        // Simply adding data within the current delayed data detection, the choice of 43100000 is arbitrary and within the window
        // for the DelayedDataDetector
        writeData(logger, index, missingDocs, now - 43100000, lastBucket.getEpoch()*1000);

        response = delayedDataDetector.detectMissingData(lastBucket.getEpoch()*1000);
        assertThat(response.stream().mapToLong(BucketWithMissingData::getMissingDocumentCount).sum(), equalTo(missingDocs));
    }

    public void testMissingDataDetectionInSpecificBucket() throws Exception {
        final String jobId = "delayed-data-detection-job-missing-test-specific-bucket";
        Job.Builder job = createJob(jobId, TimeValue.timeValueMinutes(5), "count", null);

        DatafeedConfig datafeedConfig = createDatafeed(job.getId() + "-datafeed", job.getId(), Collections.singletonList(index));
        registerJob(job);
        putJob(job);
        openJob(job.getId());

        registerDatafeed(datafeedConfig);
        putDatafeed(datafeedConfig);

        startDatafeed(datafeedConfig.getId(), 0L, now);
        waitUntilJobIsClosed(jobId);

        // Get the latest finalized bucket
        Bucket lastBucket = getLatestFinalizedBucket(jobId);

        DelayedDataDetector delayedDataDetector =
            new DelayedDataDetector(job.build(new Date()), datafeedConfig, TimeValue.timeValueHours(12), client());

        long missingDocs = randomIntBetween(1, 10);

        // Write our missing data in the bucket right before the last finalized bucket
        writeData(logger, index, missingDocs, (lastBucket.getEpoch() - lastBucket.getBucketSpan())*1000, lastBucket.getEpoch()*1000);
        List<BucketWithMissingData> response = delayedDataDetector.detectMissingData(lastBucket.getEpoch()*1000);

        boolean hasBucketWithMissing = false;
        for (BucketWithMissingData bucketWithMissingData : response) {
            if (bucketWithMissingData.getBucket().getEpoch() == lastBucket.getEpoch() - lastBucket.getBucketSpan()) {
                assertThat(bucketWithMissingData.getMissingDocumentCount(), equalTo(missingDocs));
                hasBucketWithMissing = true;
            }
        }
        assertThat(hasBucketWithMissing, equalTo(true));
    }

    public void testMissingDataDetectionWithAggregationsAndQuery() throws Exception {
        TimeValue bucketSpan = TimeValue.timeValueMinutes(10);
        final String jobId = "delayed-data-detection-job-aggs-no-missing-test";
        Job.Builder job = createJob(jobId, bucketSpan, "mean", "value", "doc_count");

        MaxAggregationBuilder maxTime = AggregationBuilders.max("time").field("time");
        AvgAggregationBuilder avgAggregationBuilder = AggregationBuilders.avg("value").field("value");
        DatafeedConfig.Builder datafeedConfigBuilder = createDatafeedBuilder(job.getId() + "-datafeed",
            job.getId(),
            Collections.singletonList(index));
        datafeedConfigBuilder.setAggregations(new AggregatorFactories.Builder().addAggregator(
                AggregationBuilders.histogram("time")
                    .subAggregation(maxTime)
                    .subAggregation(avgAggregationBuilder)
                    .field("time")
                    .interval(TimeValue.timeValueMinutes(5).millis())));
        datafeedConfigBuilder.setQuery(new RangeQueryBuilder("value").gte(numDocs/2));
        datafeedConfigBuilder.setFrequency(TimeValue.timeValueMinutes(5));
        DatafeedConfig datafeedConfig = datafeedConfigBuilder.build();
        registerJob(job);
        putJob(job);
        openJob(job.getId());

        registerDatafeed(datafeedConfig);
        putDatafeed(datafeedConfig);
        startDatafeed(datafeedConfig.getId(), 0L, now);
        waitUntilJobIsClosed(jobId);

        // Get the latest finalized bucket
        Bucket lastBucket = getLatestFinalizedBucket(jobId);

        DelayedDataDetector delayedDataDetector =
            new DelayedDataDetector(job.build(new Date()), datafeedConfig, TimeValue.timeValueHours(12), client());

        List<BucketWithMissingData> response = delayedDataDetector.detectMissingData(lastBucket.getEpoch()*1000);
        assertThat(response.stream().mapToLong(BucketWithMissingData::getMissingDocumentCount).sum(), equalTo(0L));

        long missingDocs = numDocs;
        // Simply adding data within the current delayed data detection, the choice of 43100000 is arbitrary and within the window
        // for the DelayedDataDetector
        writeData(logger, index, missingDocs, now - 43100000, lastBucket.getEpoch()*1000);

        response = delayedDataDetector.detectMissingData(lastBucket.getEpoch()*1000);
        assertThat(response.stream().mapToLong(BucketWithMissingData::getMissingDocumentCount).sum(), equalTo((missingDocs+1)/2));
    }

    private Job.Builder createJob(String id, TimeValue bucketSpan, String function, String field) {
        return createJob(id, bucketSpan, function, field, null);
    }

    private Job.Builder createJob(String id, TimeValue bucketSpan, String function, String field, String summaryCountField) {
        DataDescription.Builder dataDescription = new DataDescription.Builder();
        dataDescription.setFormat(DataDescription.DataFormat.XCONTENT);
        dataDescription.setTimeField("time");
        dataDescription.setTimeFormat(DataDescription.EPOCH_MS);

        Detector.Builder d = new Detector.Builder(function, field);
        AnalysisConfig.Builder analysisConfig = new AnalysisConfig.Builder(Collections.singletonList(d.build()));
        analysisConfig.setBucketSpan(bucketSpan);
        analysisConfig.setSummaryCountFieldName(summaryCountField);

        Job.Builder builder = new Job.Builder();
        builder.setId(id);
        builder.setAnalysisConfig(analysisConfig);
        builder.setDataDescription(dataDescription);
        return builder;
    }

    private void writeData(Logger logger, String index, long numDocs, long start, long end) {
        int maxDelta = (int) (end - start - 1);
        BulkRequestBuilder bulkRequestBuilder = client().prepareBulk();
        for (int i = 0; i < numDocs; i++) {
            IndexRequest indexRequest = new IndexRequest(index, "type");
            long timestamp = start + randomIntBetween(0, maxDelta);
            assert timestamp >= start && timestamp < end;
            indexRequest.source("time", timestamp, "value", i);
            bulkRequestBuilder.add(indexRequest);
        }
        BulkResponse bulkResponse = bulkRequestBuilder
            .setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE)
            .get();
        if (bulkResponse.hasFailures()) {
            int failures = 0;
            for (BulkItemResponse itemResponse : bulkResponse) {
                if (itemResponse.isFailed()) {
                    failures++;
                    logger.error("Item response failure [{}]", itemResponse.getFailureMessage());
                }
            }
            fail("Bulk response contained " + failures + " failures");
        }
        logger.info("Indexed [{}] documents", numDocs);
    }

    private Bucket getLatestFinalizedBucket(String jobId) {
        GetBucketsAction.Request getBucketsRequest = new GetBucketsAction.Request(jobId);
        getBucketsRequest.setExcludeInterim(true);
        getBucketsRequest.setSort(Result.TIMESTAMP.getPreferredName());
        getBucketsRequest.setDescending(true);
        getBucketsRequest.setPageParams(new PageParams(0, 1));
        return getBuckets(getBucketsRequest).get(0);
    }
}
