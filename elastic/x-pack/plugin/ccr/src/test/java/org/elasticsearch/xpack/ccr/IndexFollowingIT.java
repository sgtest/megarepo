/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.ccr;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.admin.cluster.node.tasks.list.ListTasksRequest;
import org.elasticsearch.action.admin.cluster.node.tasks.list.ListTasksResponse;
import org.elasticsearch.action.admin.indices.close.CloseIndexRequest;
import org.elasticsearch.action.admin.indices.delete.DeleteIndexRequest;
import org.elasticsearch.action.admin.indices.open.OpenIndexRequest;
import org.elasticsearch.action.admin.indices.settings.put.UpdateSettingsRequest;
import org.elasticsearch.action.admin.indices.stats.ShardStats;
import org.elasticsearch.action.bulk.BulkProcessor;
import org.elasticsearch.action.bulk.BulkRequest;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.action.get.GetResponse;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.metadata.MappingMetaData;
import org.elasticsearch.common.CheckedRunnable;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.common.xcontent.support.XContentMapValues;
import org.elasticsearch.index.IndexNotFoundException;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.persistent.PersistentTasksCustomMetaData;
import org.elasticsearch.tasks.TaskInfo;
import org.elasticsearch.xpack.CcrIntegTestCase;
import org.elasticsearch.xpack.ccr.action.ShardFollowTask;
import org.elasticsearch.xpack.core.ccr.ShardFollowNodeTaskStatus;
import org.elasticsearch.xpack.core.ccr.action.FollowStatsAction;
import org.elasticsearch.xpack.core.ccr.action.FollowStatsAction.StatsRequest;
import org.elasticsearch.xpack.core.ccr.action.FollowStatsAction.StatsResponses;
import org.elasticsearch.xpack.core.ccr.action.PauseFollowAction;
import org.elasticsearch.xpack.core.ccr.action.PutAutoFollowPatternAction;
import org.elasticsearch.xpack.core.ccr.action.PutFollowAction;
import org.elasticsearch.xpack.core.ccr.action.ResumeFollowAction;
import org.elasticsearch.xpack.core.ccr.action.UnfollowAction;

import java.io.IOException;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;
import java.util.concurrent.Semaphore;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

import static java.util.Collections.singletonMap;
import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThanOrEqualTo;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;

public class IndexFollowingIT extends CcrIntegTestCase {

    public void testFollowIndex() throws Exception {
        final int numberOfPrimaryShards = randomIntBetween(1, 3);
        final String leaderIndexSettings = getIndexSettings(numberOfPrimaryShards, between(0, 1),
            singletonMap(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true"));
        assertAcked(leaderClient().admin().indices().prepareCreate("index1").setSource(leaderIndexSettings, XContentType.JSON));
        ensureLeaderYellow("index1");

        final PutFollowAction.Request followRequest = putFollow("index1", "index2");
        followerClient().execute(PutFollowAction.INSTANCE, followRequest).get();

        final int firstBatchNumDocs = randomIntBetween(2, 64);
        logger.info("Indexing [{}] docs as first batch", firstBatchNumDocs);
        for (int i = 0; i < firstBatchNumDocs; i++) {
            final String source = String.format(Locale.ROOT, "{\"f\":%d}", i);
            leaderClient().prepareIndex("index1", "doc", Integer.toString(i)).setSource(source, XContentType.JSON).get();
        }

        final Map<ShardId, Long> firstBatchNumDocsPerShard = new HashMap<>();
        final ShardStats[] firstBatchShardStats =
            leaderClient().admin().indices().prepareStats("index1").get().getIndex("index1").getShards();
        for (final ShardStats shardStats : firstBatchShardStats) {
            if (shardStats.getShardRouting().primary()) {
                long value = shardStats.getStats().getIndexing().getTotal().getIndexCount() - 1;
                firstBatchNumDocsPerShard.put(shardStats.getShardRouting().shardId(), value);
            }
        }

        assertBusy(assertTask(numberOfPrimaryShards, firstBatchNumDocsPerShard));

        for (int i = 0; i < firstBatchNumDocs; i++) {
            assertBusy(assertExpectedDocumentRunnable(i));
        }
        assertTotalNumberOfOptimizedIndexing(resolveFollowerIndex("index2"), numberOfPrimaryShards, firstBatchNumDocs);
        pauseFollow("index2");
        followerClient().execute(ResumeFollowAction.INSTANCE, followRequest.getFollowRequest()).get();
        final int secondBatchNumDocs = randomIntBetween(2, 64);
        logger.info("Indexing [{}] docs as second batch", secondBatchNumDocs);
        for (int i = firstBatchNumDocs; i < firstBatchNumDocs + secondBatchNumDocs; i++) {
            final String source = String.format(Locale.ROOT, "{\"f\":%d}", i);
            leaderClient().prepareIndex("index1", "doc", Integer.toString(i)).setSource(source, XContentType.JSON).get();
        }

        final Map<ShardId, Long> secondBatchNumDocsPerShard = new HashMap<>();
        final ShardStats[] secondBatchShardStats =
            leaderClient().admin().indices().prepareStats("index1").get().getIndex("index1").getShards();
        for (final ShardStats shardStats : secondBatchShardStats) {
            if (shardStats.getShardRouting().primary()) {
                final long value = shardStats.getStats().getIndexing().getTotal().getIndexCount() - 1;
                secondBatchNumDocsPerShard.put(shardStats.getShardRouting().shardId(), value);
            }
        }

        assertBusy(assertTask(numberOfPrimaryShards, secondBatchNumDocsPerShard));

        for (int i = firstBatchNumDocs; i < firstBatchNumDocs + secondBatchNumDocs; i++) {
            assertBusy(assertExpectedDocumentRunnable(i));
        }
        assertTotalNumberOfOptimizedIndexing(resolveFollowerIndex("index2"), numberOfPrimaryShards,
            firstBatchNumDocs + secondBatchNumDocs);
        pauseFollow("index2");
        assertMaxSeqNoOfUpdatesIsTransferred(resolveLeaderIndex("index1"), resolveFollowerIndex("index2"), numberOfPrimaryShards);
    }

    public void testSyncMappings() throws Exception {
        final String leaderIndexSettings = getIndexSettings(2, between(0, 1),
            singletonMap(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true"));
        assertAcked(leaderClient().admin().indices().prepareCreate("index1").setSource(leaderIndexSettings, XContentType.JSON));
        ensureLeaderYellow("index1");

        final PutFollowAction.Request followRequest = putFollow("index1", "index2");
        followerClient().execute(PutFollowAction.INSTANCE, followRequest).get();

        final long firstBatchNumDocs = randomIntBetween(2, 64);
        for (long i = 0; i < firstBatchNumDocs; i++) {
            final String source = String.format(Locale.ROOT, "{\"f\":%d}", i);
            leaderClient().prepareIndex("index1", "doc", Long.toString(i)).setSource(source, XContentType.JSON).get();
        }

        assertBusy(() -> assertThat(followerClient().prepareSearch("index2").get().getHits().totalHits, equalTo(firstBatchNumDocs)));
        MappingMetaData mappingMetaData = followerClient().admin().indices().prepareGetMappings("index2").get().getMappings()
            .get("index2").get("doc");
        assertThat(XContentMapValues.extractValue("properties.f.type", mappingMetaData.sourceAsMap()), equalTo("integer"));
        assertThat(XContentMapValues.extractValue("properties.k", mappingMetaData.sourceAsMap()), nullValue());

        final int secondBatchNumDocs = randomIntBetween(2, 64);
        for (long i = firstBatchNumDocs; i < firstBatchNumDocs + secondBatchNumDocs; i++) {
            final String source = String.format(Locale.ROOT, "{\"k\":%d}", i);
            leaderClient().prepareIndex("index1", "doc", Long.toString(i)).setSource(source, XContentType.JSON).get();
        }

        assertBusy(() -> assertThat(followerClient().prepareSearch("index2").get().getHits().totalHits,
            equalTo(firstBatchNumDocs + secondBatchNumDocs)));
        mappingMetaData = followerClient().admin().indices().prepareGetMappings("index2").get().getMappings()
            .get("index2").get("doc");
        assertThat(XContentMapValues.extractValue("properties.f.type", mappingMetaData.sourceAsMap()), equalTo("integer"));
        assertThat(XContentMapValues.extractValue("properties.k.type", mappingMetaData.sourceAsMap()), equalTo("long"));
        pauseFollow("index2");
        assertMaxSeqNoOfUpdatesIsTransferred(resolveLeaderIndex("index1"), resolveFollowerIndex("index2"), 2);
    }

    public void testNoMappingDefined() throws Exception {
        assertAcked(leaderClient().admin().indices().prepareCreate("index1")
            .setSettings(Settings.builder()
                .put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), true)
                .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
                .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0)
                .build()));
        ensureLeaderGreen("index1");

        final PutFollowAction.Request followRequest = putFollow("index1", "index2");
        followerClient().execute(PutFollowAction.INSTANCE, followRequest).get();

        leaderClient().prepareIndex("index1", "doc", "1").setSource("{\"f\":1}", XContentType.JSON).get();
        assertBusy(() -> assertThat(followerClient().prepareSearch("index2").get().getHits().totalHits, equalTo(1L)));
        pauseFollow("index2");

        MappingMetaData mappingMetaData = followerClient().admin().indices().prepareGetMappings("index2").get().getMappings()
            .get("index2").get("doc");
        assertThat(XContentMapValues.extractValue("properties.f.type", mappingMetaData.sourceAsMap()), equalTo("long"));
        assertThat(XContentMapValues.extractValue("properties.k", mappingMetaData.sourceAsMap()), nullValue());
    }

    public void testFollowIndex_backlog() throws Exception {
        int numberOfShards = between(1, 5);
        String leaderIndexSettings = getIndexSettings(numberOfShards, between(0, 1),
            singletonMap(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true"));
        assertAcked(leaderClient().admin().indices().prepareCreate("index1").setSource(leaderIndexSettings, XContentType.JSON));
        BulkProcessor.Listener listener = new BulkProcessor.Listener() {
            @Override
            public void beforeBulk(long executionId, BulkRequest request) {}

            @Override
            public void afterBulk(long executionId, BulkRequest request, BulkResponse response) {}

            @Override
            public void afterBulk(long executionId, BulkRequest request, Throwable failure) {}
        };
        int bulkSize = between(1, 20);
        BulkProcessor bulkProcessor = BulkProcessor.builder(leaderClient(), listener)
            .setBulkActions(bulkSize)
            .setConcurrentRequests(4)
            .build();
        AtomicBoolean run = new AtomicBoolean(true);
        Semaphore availableDocs = new Semaphore(0);
        Thread thread = new Thread(() -> {
            int counter = 0;
            while (run.get()) {
                try {
                    if (availableDocs.tryAcquire(10, TimeUnit.MILLISECONDS) == false) {
                        continue;
                    }
                } catch (InterruptedException e) {
                    throw new AssertionError(e);
                }
                final String source = String.format(Locale.ROOT, "{\"f\":%d}", counter++);
                IndexRequest indexRequest = new IndexRequest("index1", "doc")
                    .source(source, XContentType.JSON)
                    .timeout(TimeValue.timeValueSeconds(1));
                bulkProcessor.add(indexRequest);
            }
        });
        thread.start();

        // Waiting for some document being index before following the index:
        int maxOpsPerRead = randomIntBetween(10, 100);
        int numDocsIndexed = Math.min(between(20, 300), between(maxOpsPerRead, maxOpsPerRead * 10));
        availableDocs.release(numDocsIndexed / 2 + bulkSize);
        atLeastDocsIndexed(leaderClient(), "index1", numDocsIndexed / 3);

        PutFollowAction.Request followRequest = putFollow("index1", "index2");
        followRequest.getFollowRequest().setMaxReadRequestOperationCount(maxOpsPerRead);
        followRequest.getFollowRequest().setMaxOutstandingReadRequests(randomIntBetween(1, 10));
        followRequest.getFollowRequest().setMaxOutstandingWriteRequests(randomIntBetween(1, 10));
        followRequest.getFollowRequest().setMaxWriteBufferCount(randomIntBetween(1024, 10240));
        followerClient().execute(PutFollowAction.INSTANCE, followRequest).get();
        availableDocs.release(numDocsIndexed * 2  + bulkSize);
        atLeastDocsIndexed(leaderClient(), "index1", numDocsIndexed);
        run.set(false);
        thread.join();
        assertThat(bulkProcessor.awaitClose(1L, TimeUnit.MINUTES), is(true));

        assertIndexFullyReplicatedToFollower("index1", "index2");
        pauseFollow("index2");
        leaderClient().admin().indices().prepareRefresh("index1").get();
        assertTotalNumberOfOptimizedIndexing(resolveFollowerIndex("index2"), numberOfShards,
            leaderClient().prepareSearch("index1").get().getHits().totalHits);
        assertMaxSeqNoOfUpdatesIsTransferred(resolveLeaderIndex("index1"), resolveFollowerIndex("index2"), numberOfShards);
    }

    public void testFollowIndexWithNestedField() throws Exception {
        final String leaderIndexSettings =
            getIndexSettingsWithNestedMapping(1, between(0, 1), singletonMap(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true"));
        assertAcked(leaderClient().admin().indices().prepareCreate("index1").setSource(leaderIndexSettings, XContentType.JSON));
        ensureLeaderGreen("index1");

        final PutFollowAction.Request followRequest = putFollow("index1", "index2");
        followerClient().execute(PutFollowAction.INSTANCE, followRequest).get();

        final int numDocs = randomIntBetween(2, 64);
        for (int i = 0; i < numDocs; i++) {
            try (XContentBuilder builder = jsonBuilder()) {
                builder.startObject();
                builder.field("field", "value");
                builder.startArray("objects");
                {
                    builder.startObject();
                    builder.field("field", i);
                    builder.endObject();
                }
                builder.endArray();
                builder.endObject();
                leaderClient().prepareIndex("index1", "doc", Integer.toString(i)).setSource(builder).get();
            }
        }

        for (int i = 0; i < numDocs; i++) {
            int value = i;
            assertBusy(() -> {
                final GetResponse getResponse = followerClient().prepareGet("index2", "doc", Integer.toString(value)).get();
                assertTrue(getResponse.isExists());
                assertTrue((getResponse.getSource().containsKey("field")));
                assertThat(XContentMapValues.extractValue("objects.field", getResponse.getSource()),
                    equalTo(Collections.singletonList(value)));
            });
        }
        pauseFollow("index2");
        assertMaxSeqNoOfUpdatesIsTransferred(resolveLeaderIndex("index1"), resolveFollowerIndex("index2"), 1);
        assertTotalNumberOfOptimizedIndexing(resolveFollowerIndex("index2"), 1, numDocs);
    }

    public void testUnfollowNonExistingIndex() {
        PauseFollowAction.Request unfollowRequest = new PauseFollowAction.Request("non-existing-index");
        expectThrows(IllegalArgumentException.class,
            () -> followerClient().execute(PauseFollowAction.INSTANCE, unfollowRequest).actionGet());
    }

    public void testFollowNonExistentIndex() throws Exception {
        String indexSettings = getIndexSettings(1, 0, Collections.emptyMap());
        assertAcked(leaderClient().admin().indices().prepareCreate("test-leader").setSource(indexSettings, XContentType.JSON).get());
        assertAcked(followerClient().admin().indices().prepareCreate("test-follower").setSource(indexSettings, XContentType.JSON).get());
        ensureLeaderGreen("test-leader");
        ensureFollowerGreen("test-follower");
        // Leader index does not exist.
        expectThrows(IndexNotFoundException.class,
            () -> followerClient().execute(PutFollowAction.INSTANCE, putFollow("non-existent-leader", "test-follower"))
                .actionGet());
        // Follower index does not exist.
        ResumeFollowAction.Request followRequest1 = resumeFollow("non-existent-follower");
        expectThrows(IndexNotFoundException.class, () -> followerClient().execute(ResumeFollowAction.INSTANCE, followRequest1).actionGet());
        // Both indices do not exist.
        ResumeFollowAction.Request followRequest2 = resumeFollow("non-existent-follower");
        expectThrows(IndexNotFoundException.class, () -> followerClient().execute(ResumeFollowAction.INSTANCE, followRequest2).actionGet());
        expectThrows(IndexNotFoundException.class,
            () -> followerClient().execute(PutFollowAction.INSTANCE, putFollow("non-existing-leader", "non-existing-follower"))
                .actionGet());
    }

    public void testFollowIndexMaxOperationSizeInBytes() throws Exception {
        final String leaderIndexSettings = getIndexSettings(1, between(0, 1),
            singletonMap(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true"));
        assertAcked(leaderClient().admin().indices().prepareCreate("index1").setSource(leaderIndexSettings, XContentType.JSON));
        ensureLeaderYellow("index1");

        final int numDocs = between(10, 1024);
        logger.info("Indexing [{}] docs", numDocs);
        for (int i = 0; i < numDocs; i++) {
            final String source = String.format(Locale.ROOT, "{\"f\":%d}", i);
            leaderClient().prepareIndex("index1", "doc", Integer.toString(i)).setSource(source, XContentType.JSON).get();
        }

        PutFollowAction.Request followRequest = putFollow("index1", "index2");
        followRequest.getFollowRequest().setMaxReadRequestSize(new ByteSizeValue(1, ByteSizeUnit.BYTES));
        followerClient().execute(PutFollowAction.INSTANCE, followRequest).get();

        final Map<ShardId, Long> firstBatchNumDocsPerShard = new HashMap<>();
        final ShardStats[] firstBatchShardStats =
            leaderClient().admin().indices().prepareStats("index1").get().getIndex("index1").getShards();
        for (final ShardStats shardStats : firstBatchShardStats) {
            if (shardStats.getShardRouting().primary()) {
                long value = shardStats.getStats().getIndexing().getTotal().getIndexCount() - 1;
                firstBatchNumDocsPerShard.put(shardStats.getShardRouting().shardId(), value);
            }
        }

        assertBusy(assertTask(1, firstBatchNumDocsPerShard));
        for (int i = 0; i < numDocs; i++) {
            assertBusy(assertExpectedDocumentRunnable(i));
        }
        pauseFollow("index2");
        assertMaxSeqNoOfUpdatesIsTransferred(resolveLeaderIndex("index1"), resolveFollowerIndex("index2"), 1);
        assertTotalNumberOfOptimizedIndexing(resolveFollowerIndex("index2"), 1, numDocs);
    }

    public void testAttemptToChangeCcrFollowingIndexSetting() throws Exception {
        String leaderIndexSettings = getIndexSettings(1, 0, singletonMap(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true"));
        assertAcked(leaderClient().admin().indices().prepareCreate("index1").setSource(leaderIndexSettings, XContentType.JSON).get());
        ensureLeaderYellow("index1");
        PutFollowAction.Request followRequest = putFollow("index1", "index2");
        followerClient().execute(PutFollowAction.INSTANCE, followRequest).get();
        pauseFollow("index2");
        followerClient().admin().indices().close(new CloseIndexRequest("index2")).actionGet();

        UpdateSettingsRequest updateSettingsRequest = new UpdateSettingsRequest("index2");
        updateSettingsRequest.settings(Settings.builder().put(CcrSettings.CCR_FOLLOWING_INDEX_SETTING.getKey(), false).build());
        Exception e = expectThrows(IllegalArgumentException.class,
            () -> followerClient().admin().indices().updateSettings(updateSettingsRequest).actionGet());
        assertThat(e.getMessage(), equalTo("can not update internal setting [index.xpack.ccr.following_index]; " +
            "this setting is managed via a dedicated API"));
    }

    public void testCloseLeaderIndex() throws Exception {
        assertAcked(leaderClient().admin().indices().prepareCreate("index1")
            .setSettings(Settings.builder()
                .put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), true)
                .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
                .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0)
                .build()));

        final PutFollowAction.Request followRequest = putFollow("index1", "index2");
        followerClient().execute(PutFollowAction.INSTANCE, followRequest).get();

        leaderClient().prepareIndex("index1", "doc", "1").setSource("{}", XContentType.JSON).get();
        assertBusy(() -> assertThat(followerClient().prepareSearch("index2").get().getHits().totalHits, equalTo(1L)));

        leaderClient().admin().indices().close(new CloseIndexRequest("index1")).actionGet();
        assertBusy(() -> {
            StatsResponses response = followerClient().execute(FollowStatsAction.INSTANCE, new StatsRequest()).actionGet();
            assertThat(response.getNodeFailures(), empty());
            assertThat(response.getTaskFailures(), empty());
            assertThat(response.getStatsResponses(), hasSize(1));
            assertThat(response.getStatsResponses().get(0).status().failedReadRequests(), greaterThanOrEqualTo(1L));
            assertThat(response.getStatsResponses().get(0).status().readExceptions().size(), equalTo(1));
            ElasticsearchException exception = response.getStatsResponses().get(0).status()
                .readExceptions().entrySet().iterator().next().getValue().v2();
            assertThat(exception.getRootCause().getMessage(), equalTo("blocked by: [FORBIDDEN/4/index closed];"));
        });

        leaderClient().admin().indices().open(new OpenIndexRequest("index1")).actionGet();
        leaderClient().prepareIndex("index1", "doc", "2").setSource("{}", XContentType.JSON).get();
        assertBusy(() -> assertThat(followerClient().prepareSearch("index2").get().getHits().totalHits, equalTo(2L)));

        pauseFollow("index2");
    }

    public void testCloseFollowIndex() throws Exception {
        assertAcked(leaderClient().admin().indices().prepareCreate("index1")
            .setSettings(Settings.builder()
                .put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), true)
                .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
                .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0)
                .build()));

        final PutFollowAction.Request followRequest = putFollow("index1", "index2");
        followerClient().execute(PutFollowAction.INSTANCE, followRequest).get();

        leaderClient().prepareIndex("index1", "doc", "1").setSource("{}", XContentType.JSON).get();
        assertBusy(() -> assertThat(followerClient().prepareSearch("index2").get().getHits().totalHits, equalTo(1L)));

        followerClient().admin().indices().close(new CloseIndexRequest("index2")).actionGet();
        leaderClient().prepareIndex("index1", "doc", "2").setSource("{}", XContentType.JSON).get();
        assertBusy(() -> {
            StatsResponses response = followerClient().execute(FollowStatsAction.INSTANCE, new StatsRequest()).actionGet();
            assertThat(response.getNodeFailures(), empty());
            assertThat(response.getTaskFailures(), empty());
            assertThat(response.getStatsResponses(), hasSize(1));
            assertThat(response.getStatsResponses().get(0).status().failedWriteRequests(), greaterThanOrEqualTo(1L));
        });
        followerClient().admin().indices().open(new OpenIndexRequest("index2")).actionGet();
        assertBusy(() -> assertThat(followerClient().prepareSearch("index2").get().getHits().totalHits, equalTo(2L)));

        pauseFollow("index2");
    }

    public void testDeleteLeaderIndex() throws Exception {
        assertAcked(leaderClient().admin().indices().prepareCreate("index1")
            .setSettings(Settings.builder()
                .put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), true)
                .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
                .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0)
                .build()));

        final PutFollowAction.Request followRequest = putFollow("index1", "index2");
        followerClient().execute(PutFollowAction.INSTANCE, followRequest).get();

        leaderClient().prepareIndex("index1", "doc", "1").setSource("{}", XContentType.JSON).get();
        assertBusy(() -> assertThat(followerClient().prepareSearch("index2").get().getHits().totalHits, equalTo(1L)));

        leaderClient().admin().indices().delete(new DeleteIndexRequest("index1")).actionGet();
        assertBusy(() -> {
            StatsResponses response = followerClient().execute(FollowStatsAction.INSTANCE, new StatsRequest()).actionGet();
            assertThat(response.getNodeFailures(), empty());
            assertThat(response.getTaskFailures(), empty());
            assertThat(response.getStatsResponses(), hasSize(1));
            assertThat(response.getStatsResponses().get(0).status().failedReadRequests(), greaterThanOrEqualTo(1L));
            ElasticsearchException fatalException = response.getStatsResponses().get(0).status().getFatalException();
            assertThat(fatalException, notNullValue());
            assertThat(fatalException.getRootCause().getMessage(), equalTo("no such index [index1]"));
        });
        pauseFollow("index2");
        ensureNoCcrTasks();
    }

    public void testDeleteFollowerIndex() throws Exception {
        assertAcked(leaderClient().admin().indices().prepareCreate("index1")
            .setSettings(Settings.builder()
                .put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), true)
                .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
                .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0)
                .build()));

        final PutFollowAction.Request followRequest = putFollow("index1", "index2");
        followerClient().execute(PutFollowAction.INSTANCE, followRequest).get();

        leaderClient().prepareIndex("index1", "doc", "1").setSource("{}", XContentType.JSON).get();
        assertBusy(() -> assertThat(followerClient().prepareSearch("index2").get().getHits().totalHits, equalTo(1L)));

        followerClient().admin().indices().delete(new DeleteIndexRequest("index2")).actionGet();
        leaderClient().prepareIndex("index1", "doc", "2").setSource("{}", XContentType.JSON).get();
        assertBusy(() -> {
            StatsResponses response = followerClient().execute(FollowStatsAction.INSTANCE, new StatsRequest()).actionGet();
            assertThat(response.getNodeFailures(), empty());
            assertThat(response.getTaskFailures(), empty());
            assertThat(response.getStatsResponses(), hasSize(1));
            assertThat(response.getStatsResponses().get(0).status().failedWriteRequests(), greaterThanOrEqualTo(1L));
            ElasticsearchException fatalException = response.getStatsResponses().get(0).status().getFatalException();
            assertThat(fatalException, notNullValue());
            assertThat(fatalException.getMessage(), equalTo("no such index [index2]"));
        });
        pauseFollow("index2");
        ensureNoCcrTasks();
    }

    public void testUnfollowIndex() throws Exception {
        String leaderIndexSettings = getIndexSettings(1, 0, singletonMap(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true"));
        assertAcked(leaderClient().admin().indices().prepareCreate("index1").setSource(leaderIndexSettings, XContentType.JSON).get());
        PutFollowAction.Request followRequest = putFollow("index1", "index2");
        followerClient().execute(PutFollowAction.INSTANCE, followRequest).get();
        leaderClient().prepareIndex("index1", "doc").setSource("{}", XContentType.JSON).get();
        assertBusy(() -> {
            assertThat(followerClient().prepareSearch("index2").get().getHits().getTotalHits(), equalTo(1L));
        });

        // Indexing directly into index2 would fail now, because index2 is a follow index.
        // We can't test this here because an assertion trips before an actual error is thrown and then index call hangs.

        // Turn follow index into a regular index by: pausing shard follow, close index, unfollow index and then open index:
        pauseFollow("index2");
        followerClient().admin().indices().close(new CloseIndexRequest("index2")).actionGet();
        assertAcked(followerClient().execute(UnfollowAction.INSTANCE, new UnfollowAction.Request("index2")).actionGet());
        followerClient().admin().indices().open(new OpenIndexRequest("index2")).actionGet();
        ensureFollowerGreen("index2");

        // Indexing succeeds now, because index2 is no longer a follow index:
        followerClient().prepareIndex("index2", "doc").setSource("{}", XContentType.JSON)
            .setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE)
            .get();
        assertThat(followerClient().prepareSearch("index2").get().getHits().getTotalHits(), equalTo(2L));
    }

    public void testUnknownClusterAlias() throws Exception {
        String leaderIndexSettings = getIndexSettings(1, 0,
            Collections.singletonMap(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true"));
        assertAcked(leaderClient().admin().indices().prepareCreate("index1").setSource(leaderIndexSettings, XContentType.JSON));
        ensureLeaderGreen("index1");
        PutFollowAction.Request followRequest = putFollow("index1", "index2");
        followRequest.setRemoteCluster("another_cluster");
        Exception e = expectThrows(IllegalArgumentException.class,
            () -> followerClient().execute(PutFollowAction.INSTANCE, followRequest).actionGet());
        assertThat(e.getMessage(), equalTo("unknown cluster alias [another_cluster]"));
        PutAutoFollowPatternAction.Request putAutoFollowRequest = new PutAutoFollowPatternAction.Request();
        putAutoFollowRequest.setName("name");
        putAutoFollowRequest.setRemoteCluster("another_cluster");
        putAutoFollowRequest.setLeaderIndexPatterns(Collections.singletonList("logs-*"));
        e = expectThrows(IllegalArgumentException.class,
            () -> followerClient().execute(PutAutoFollowPatternAction.INSTANCE, putAutoFollowRequest).actionGet());
        assertThat(e.getMessage(), equalTo("unknown cluster alias [another_cluster]"));
    }

    private CheckedRunnable<Exception> assertTask(final int numberOfPrimaryShards, final Map<ShardId, Long> numDocsPerShard) {
        return () -> {
            final ClusterState clusterState = followerClient().admin().cluster().prepareState().get().getState();
            final PersistentTasksCustomMetaData taskMetadata = clusterState.getMetaData().custom(PersistentTasksCustomMetaData.TYPE);

            ListTasksRequest listTasksRequest = new ListTasksRequest();
            listTasksRequest.setDetailed(true);
            listTasksRequest.setActions(ShardFollowTask.NAME + "[c]");
            ListTasksResponse listTasksResponse = followerClient().admin().cluster().listTasks(listTasksRequest).actionGet();
            assertThat(listTasksResponse.getNodeFailures().size(), equalTo(0));
            assertThat(listTasksResponse.getTaskFailures().size(), equalTo(0));

            List<TaskInfo> taskInfos = listTasksResponse.getTasks();
            assertThat(taskInfos.size(), equalTo(numberOfPrimaryShards));
            Collection<PersistentTasksCustomMetaData.PersistentTask<?>> shardFollowTasks =
                taskMetadata.findTasks(ShardFollowTask.NAME, Objects::nonNull);
            for (PersistentTasksCustomMetaData.PersistentTask<?> shardFollowTask : shardFollowTasks) {
                final ShardFollowTask shardFollowTaskParams = (ShardFollowTask) shardFollowTask.getParams();
                TaskInfo taskInfo = null;
                String expectedId = "id=" + shardFollowTask.getId();
                for (TaskInfo info : taskInfos) {
                    if (expectedId.equals(info.getDescription())) {
                        taskInfo = info;
                        break;
                    }
                }
                assertThat(taskInfo, notNullValue());
                ShardFollowNodeTaskStatus status = (ShardFollowNodeTaskStatus) taskInfo.getStatus();
                assertThat(status, notNullValue());
                assertThat("incorrect global checkpoint " + shardFollowTaskParams,
                    status.followerGlobalCheckpoint(),
                    equalTo(numDocsPerShard.get(shardFollowTaskParams.getLeaderShardId())));
            }
        };
    }

    private CheckedRunnable<Exception> assertExpectedDocumentRunnable(final int value) {
        return () -> {
            final GetResponse getResponse = followerClient().prepareGet("index2", "doc", Integer.toString(value)).get();
            assertTrue("Doc with id [" + value + "] is missing", getResponse.isExists());
            assertTrue((getResponse.getSource().containsKey("f")));
            assertThat(getResponse.getSource().get("f"), equalTo(value));
        };
    }

    private String getIndexSettingsWithNestedMapping(final int numberOfShards, final int numberOfReplicas,
                                                     final Map<String, String> additionalIndexSettings) throws IOException {
        final String settings;
        try (XContentBuilder builder = jsonBuilder()) {
            builder.startObject();
            {
                builder.startObject("settings");
                {
                    builder.field("index.number_of_shards", numberOfShards);
                    builder.field("index.number_of_replicas", numberOfReplicas);
                    for (final Map.Entry<String, String> additionalSetting : additionalIndexSettings.entrySet()) {
                        builder.field(additionalSetting.getKey(), additionalSetting.getValue());
                    }
                }
                builder.endObject();
                builder.startObject("mappings");
                {
                    builder.startObject("doc");
                    {
                        builder.startObject("properties");
                        {
                            builder.startObject("objects");
                            {
                                builder.field("type", "nested");
                                builder.startObject("properties");
                                {
                                    builder.startObject("field");
                                    {
                                        builder.field("type", "long");
                                    }
                                    builder.endObject();
                                }
                                builder.endObject();
                            }
                            builder.endObject();
                            builder.startObject("field");
                            {
                                builder.field("type", "keyword");
                            }
                            builder.endObject();
                        }
                        builder.endObject();
                    }
                    builder.endObject();
                }
                builder.endObject();
            }
            builder.endObject();
            settings = BytesReference.bytes(builder).utf8ToString();
        }
        return settings;
    }

}
