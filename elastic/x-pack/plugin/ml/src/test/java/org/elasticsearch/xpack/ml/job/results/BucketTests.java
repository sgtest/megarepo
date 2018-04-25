/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.job.results;

import org.elasticsearch.common.io.stream.Writeable.Reader;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.test.AbstractSerializingTestCase;
import org.elasticsearch.xpack.core.ml.job.results.AnomalyRecord;
import org.elasticsearch.xpack.core.ml.job.results.AnomalyRecordTests;
import org.elasticsearch.xpack.core.ml.job.results.Bucket;
import org.elasticsearch.xpack.core.ml.job.results.BucketInfluencer;
import org.elasticsearch.xpack.core.ml.job.results.PartitionScore;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.Date;
import java.util.List;
import java.util.stream.IntStream;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;

public class BucketTests extends AbstractSerializingTestCase<Bucket> {

    @Override
    public Bucket createTestInstance() {
        return createTestInstance("foo");
    }

    public Bucket createTestInstance(String jobId) {
        Bucket bucket = new Bucket(jobId, new Date(randomNonNegativeLong()), randomNonNegativeLong());
        if (randomBoolean()) {
            bucket.setAnomalyScore(randomDouble());
        }
        if (randomBoolean()) {
            int size = randomInt(10);
            List<BucketInfluencer> bucketInfluencers = new ArrayList<>(size);
            for (int i = 0; i < size; i++) {
                BucketInfluencer bucketInfluencer = new BucketInfluencer(jobId, new Date(), 600);
                bucketInfluencer.setAnomalyScore(randomDouble());
                bucketInfluencer.setInfluencerFieldName(randomAlphaOfLengthBetween(1, 20));
                bucketInfluencer.setInitialAnomalyScore(randomDouble());
                bucketInfluencer.setProbability(randomDouble());
                bucketInfluencer.setRawAnomalyScore(randomDouble());
                bucketInfluencers.add(bucketInfluencer);
            }
            bucket.setBucketInfluencers(bucketInfluencers);
        }
        if (randomBoolean()) {
            bucket.setEventCount(randomNonNegativeLong());
        }
        if (randomBoolean()) {
            bucket.setInitialAnomalyScore(randomDouble());
        }
        if (randomBoolean()) {
            bucket.setInterim(randomBoolean());
        }
        if (randomBoolean()) {
            int size = randomInt(10);
            List<PartitionScore> partitionScores = new ArrayList<>(size);
            for (int i = 0; i < size; i++) {
                partitionScores.add(new PartitionScore(randomAlphaOfLengthBetween(1, 20), randomAlphaOfLengthBetween(1, 20), randomDouble(),
                        randomDouble(), randomDouble()));
            }
            bucket.setPartitionScores(partitionScores);
        }
        if (randomBoolean()) {
            bucket.setProcessingTimeMs(randomLong());
        }
        if (randomBoolean()) {
            int size = randomInt(10);
            List<AnomalyRecord> records = new ArrayList<>(size);
            for (int i = 0; i < size; i++) {
                AnomalyRecord anomalyRecord = new AnomalyRecordTests().createTestInstance(jobId);
                records.add(anomalyRecord);
            }
            bucket.setRecords(records);
        }
        if (randomBoolean()) {
            int size = randomInt(10);
            List<String> scheduledEvents = new ArrayList<>(size);
            IntStream.range(0, size).forEach(i -> scheduledEvents.add(randomAlphaOfLength(20)));
            bucket.setScheduledEvents(scheduledEvents);
        }
        return bucket;
    }

    @Override
    protected Reader<Bucket> instanceReader() {
        return Bucket::new;
    }

    @Override
    protected Bucket doParseInstance(XContentParser parser) {
        return Bucket.STRICT_PARSER.apply(parser, null);
    }

    public void testEquals_GivenDifferentClass() {
        Bucket bucket = new Bucket("foo", new Date(randomLong()), randomNonNegativeLong());
        assertFalse(bucket.equals("a string"));
    }

    public void testEquals_GivenTwoDefaultBuckets() {
        Bucket bucket1 = new Bucket("foo", new Date(123), 123);
        Bucket bucket2 = new Bucket("foo", new Date(123), 123);

        assertTrue(bucket1.equals(bucket2));
        assertTrue(bucket2.equals(bucket1));
    }

    public void testEquals_GivenDifferentAnomalyScore() {
        Bucket bucket1 = new Bucket("foo", new Date(123), 123);
        bucket1.setAnomalyScore(3.0);
        Bucket bucket2 = new Bucket("foo", new Date(123), 123);
        bucket2.setAnomalyScore(2.0);

        assertFalse(bucket1.equals(bucket2));
        assertFalse(bucket2.equals(bucket1));
    }

    public void testEquals_GivenSameDates() {
        Bucket b1 = new Bucket("foo", new Date(1234567890L), 123);
        Bucket b2 = new Bucket("foo", new Date(1234567890L), 123);
        assertTrue(b1.equals(b2));
    }

    public void testEquals_GivenDifferentEventCount() {
        Bucket bucket1 = new Bucket("foo", new Date(123), 123);
        bucket1.setEventCount(3);
        Bucket bucket2 = new Bucket("foo", new Date(123), 123);
        bucket2.setEventCount(100);

        assertFalse(bucket1.equals(bucket2));
        assertFalse(bucket2.equals(bucket1));
    }

    public void testEquals_GivenOneHasRecordsAndTheOtherDoesNot() {
        Bucket bucket1 = new Bucket("foo", new Date(123), 123);
        bucket1.setRecords(Collections.singletonList(new AnomalyRecord("foo", new Date(123), 123)));
        Bucket bucket2 = new Bucket("foo", new Date(123), 123);
        bucket2.setRecords(Collections.emptyList());

        assertFalse(bucket1.equals(bucket2));
        assertFalse(bucket2.equals(bucket1));
    }

    public void testEquals_GivenDifferentNumberOfRecords() {
        Bucket bucket1 = new Bucket("foo", new Date(123), 123);
        bucket1.setRecords(Collections.singletonList(new AnomalyRecord("foo", new Date(123), 123)));
        Bucket bucket2 = new Bucket("foo", new Date(123), 123);
        bucket2.setRecords(Arrays.asList(new AnomalyRecord("foo", new Date(123), 123),
                new AnomalyRecord("foo", new Date(123), 123)));

        assertFalse(bucket1.equals(bucket2));
        assertFalse(bucket2.equals(bucket1));
    }

    public void testEquals_GivenSameNumberOfRecordsButDifferent() {
        AnomalyRecord anomalyRecord1 = new AnomalyRecord("foo", new Date(123), 123);
        anomalyRecord1.setRecordScore(1.0);
        AnomalyRecord anomalyRecord2 = new AnomalyRecord("foo", new Date(123), 123);
        anomalyRecord1.setRecordScore(2.0);

        Bucket bucket1 = new Bucket("foo", new Date(123), 123);
        bucket1.setRecords(Collections.singletonList(anomalyRecord1));
        Bucket bucket2 = new Bucket("foo", new Date(123), 123);
        bucket2.setRecords(Collections.singletonList(anomalyRecord2));

        assertFalse(bucket1.equals(bucket2));
        assertFalse(bucket2.equals(bucket1));
    }

    public void testEquals_GivenDifferentIsInterim() {
        Bucket bucket1 = new Bucket("foo", new Date(123), 123);
        bucket1.setInterim(true);
        Bucket bucket2 = new Bucket("foo", new Date(123), 123);
        bucket2.setInterim(false);

        assertFalse(bucket1.equals(bucket2));
        assertFalse(bucket2.equals(bucket1));
    }

    public void testEquals_GivenDifferentBucketInfluencers() {
        Bucket bucket1 = new Bucket("foo", new Date(123), 123);
        BucketInfluencer influencer1 = new BucketInfluencer("foo", new Date(123), 123);
        influencer1.setInfluencerFieldName("foo");
        bucket1.addBucketInfluencer(influencer1);

        Bucket bucket2 = new Bucket("foo", new Date(123), 123);
        BucketInfluencer influencer2 = new BucketInfluencer("foo", new Date(123), 123);
        influencer2.setInfluencerFieldName("bar");
        bucket2.addBucketInfluencer(influencer2);

        assertFalse(bucket1.equals(bucket2));
        assertFalse(bucket2.equals(bucket1));
    }

    public void testEquals_GivenEqualBuckets() {
        AnomalyRecord record = new AnomalyRecord("job_id", new Date(123), 123);
        BucketInfluencer bucketInfluencer = new BucketInfluencer("foo", new Date(123), 123);
        Date date = new Date();

        Bucket bucket1 = new Bucket("foo", date, 123);
        bucket1.setAnomalyScore(42.0);
        bucket1.setInitialAnomalyScore(92.0);
        bucket1.setEventCount(134);
        bucket1.setInterim(true);
        bucket1.setRecords(Collections.singletonList(record));
        bucket1.addBucketInfluencer(bucketInfluencer);

        Bucket bucket2 = new Bucket("foo", date, 123);
        bucket2.setAnomalyScore(42.0);
        bucket2.setInitialAnomalyScore(92.0);
        bucket2.setEventCount(134);
        bucket2.setInterim(true);
        bucket2.setRecords(Collections.singletonList(record));
        bucket2.addBucketInfluencer(bucketInfluencer);

        assertTrue(bucket1.equals(bucket2));
        assertTrue(bucket2.equals(bucket1));
        assertEquals(bucket1.hashCode(), bucket2.hashCode());
    }

    public void testIsNormalizable_GivenAnomalyScoreIsZeroAndRecordCountIsZero() {
        Bucket bucket = new Bucket("foo", new Date(123), 123);
        bucket.addBucketInfluencer(new BucketInfluencer("foo", new Date(123), 123));
        bucket.setAnomalyScore(0.0);

        assertFalse(bucket.isNormalizable());
    }

    public void testIsNormalizable_GivenAnomalyScoreIsZeroAndPartitionsScoresAreNonZero() {
        Bucket bucket = new Bucket("foo", new Date(123), 123);
        bucket.addBucketInfluencer(new BucketInfluencer("foo", new Date(123), 123));
        bucket.setAnomalyScore(0.0);
        bucket.setPartitionScores(Collections.singletonList(new PartitionScore("n", "v", 50.0, 40.0, 0.01)));

        assertTrue(bucket.isNormalizable());
    }

    public void testIsNormalizable_GivenAnomalyScoreIsNonZeroAndRecordCountIsZero() {
        Bucket bucket = new Bucket("foo", new Date(123), 123);
        bucket.addBucketInfluencer(new BucketInfluencer("foo", new Date(123), 123));
        bucket.setAnomalyScore(1.0);

        assertTrue(bucket.isNormalizable());
    }

    public void testIsNormalizable_GivenAnomalyScoreIsNonZeroAndRecordCountIsNonZero() {
        Bucket bucket = new Bucket("foo", new Date(123), 123);
        bucket.addBucketInfluencer(new BucketInfluencer("foo", new Date(123), 123));
        bucket.setAnomalyScore(1.0);

        assertTrue(bucket.isNormalizable());
    }

    public void testPartitionAnomalyScore() {
        List<PartitionScore> pScore = new ArrayList<>();
        pScore.add(new PartitionScore("pf", "pv1", 11.0, 10.0, 0.1));
        pScore.add(new PartitionScore("pf", "pv3", 51.0, 50.0, 0.1));
        pScore.add(new PartitionScore("pf", "pv4", 61.0, 60.0, 0.1));
        pScore.add(new PartitionScore("pf", "pv2", 41.0, 40.0, 0.1));

        Bucket bucket = new Bucket("foo", new Date(123), 123);
        bucket.setPartitionScores(pScore);

        double initialAnomalyScore = bucket.partitionInitialAnomalyScore("pv1");
        assertEquals(11.0, initialAnomalyScore, 0.001);
        double anomalyScore = bucket.partitionAnomalyScore("pv1");
        assertEquals(10.0, anomalyScore, 0.001);
        initialAnomalyScore = bucket.partitionInitialAnomalyScore("pv2");
        assertEquals(41.0, initialAnomalyScore, 0.001);
        anomalyScore = bucket.partitionAnomalyScore("pv2");
        assertEquals(40.0, anomalyScore, 0.001);
        initialAnomalyScore = bucket.partitionInitialAnomalyScore("pv3");
        assertEquals(51.0, initialAnomalyScore, 0.001);
        anomalyScore = bucket.partitionAnomalyScore("pv3");
        assertEquals(50.0, anomalyScore, 0.001);
        initialAnomalyScore = bucket.partitionInitialAnomalyScore("pv4");
        assertEquals(61.0, initialAnomalyScore, 0.001);
        anomalyScore = bucket.partitionAnomalyScore("pv4");
        assertEquals(60.0, anomalyScore, 0.001);
    }

    public void testId() {
        Bucket bucket = new Bucket("foo", new Date(123), 60L);
        assertEquals("foo_bucket_123_60", bucket.getId());
    }

    public void testCopyConstructor() {
        for (int i = 0; i < NUMBER_OF_TEST_RUNS; ++i) {
            Bucket bucket = createTestInstance();
            Bucket copy = new Bucket(bucket);
            assertThat(copy, equalTo(bucket));
        }
    }

    public void testStrictParser() throws IOException {
        String json = "{\"job_id\":\"job_1\", \"timestamp\": 123544456, \"bucket_span\": 3600, \"foo\":\"bar\"}";
        try (XContentParser parser = createParser(JsonXContent.jsonXContent, json)) {
            IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
                    () -> Bucket.STRICT_PARSER.apply(parser, null));

            assertThat(e.getMessage(), containsString("unknown field [foo]"));
        }
    }

    public void testLenientParser() throws IOException {
        String json = "{\"job_id\":\"job_1\", \"timestamp\": 123544456, \"bucket_span\": 3600, \"foo\":\"bar\"}";
        try (XContentParser parser = createParser(JsonXContent.jsonXContent, json)) {
            Bucket.LENIENT_PARSER.apply(parser, null);
        }
    }
}
