/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.index.engine;

import org.elasticsearch.action.index.IndexResponse;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.search.SearchType;
import org.elasticsearch.action.support.ActiveShardCount;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.cluster.metadata.DataStream;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.routing.allocation.command.AllocateStalePrimaryAllocationCommand;
import org.elasticsearch.cluster.routing.allocation.command.CancelAllocationCommand;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.index.Index;
import org.elasticsearch.index.mapper.DateFieldMapper;
import org.elasticsearch.index.query.RangeQueryBuilder;
import org.elasticsearch.index.shard.IndexLongFieldRange;
import org.elasticsearch.indices.IndicesService;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.protocol.xpack.frozen.FreezeRequest;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.search.builder.PointInTimeBuilder;
import org.elasticsearch.test.ESIntegTestCase;
import org.elasticsearch.test.InternalTestCluster;
import org.elasticsearch.xpack.core.LocalStateCompositeXPackPlugin;
import org.elasticsearch.xpack.core.frozen.action.FreezeIndexAction;
import org.elasticsearch.action.search.ClosePointInTimeAction;
import org.elasticsearch.action.search.ClosePointInTimeRequest;
import org.elasticsearch.action.search.OpenPointInTimeAction;
import org.elasticsearch.action.search.OpenPointInTimeRequest;
import org.elasticsearch.xpack.frozen.FrozenIndices;
import org.joda.time.Instant;

import java.io.IOException;
import java.util.Collection;
import java.util.List;
import java.util.stream.Collectors;
import java.util.stream.StreamSupport;

import static org.elasticsearch.cluster.metadata.IndexMetadata.INDEX_ROUTING_EXCLUDE_GROUP_SETTING;
import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertHitCount;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertNoFailures;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.not;
import static org.hamcrest.Matchers.sameInstance;

@ESIntegTestCase.ClusterScope(numDataNodes = 0)
public class FrozenIndexIT extends ESIntegTestCase {

    @Override
    protected Collection<Class<? extends Plugin>> nodePlugins() {
        return List.of(FrozenIndices.class, LocalStateCompositeXPackPlugin.class);
    }

    @Override
    protected boolean addMockInternalEngine() {
        return false;
    }

    public void testTimestampRangeRecalculatedOnStalePrimaryAllocation() throws IOException {
        final List<String> nodeNames = internalCluster().startNodes(2);

        createIndex("index", Settings.builder()
                .put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, 1)
                .put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, 1)
                .build());

        final IndexResponse indexResponse = client().prepareIndex("index")
                .setSource(DataStream.TimestampField.FIXED_TIMESTAMP_FIELD, "2010-01-06T02:03:04.567Z").get();

        ensureGreen("index");

        assertThat(client().admin().indices().prepareFlush("index").get().getSuccessfulShards(), equalTo(2));
        assertThat(client().admin().indices().prepareRefresh("index").get().getSuccessfulShards(), equalTo(2));

        final String excludeSetting = INDEX_ROUTING_EXCLUDE_GROUP_SETTING.getConcreteSettingForNamespace("_name").getKey();
        assertAcked(client().admin().indices().prepareUpdateSettings("index").setSettings(
                Settings.builder().put(excludeSetting, nodeNames.get(0))));
        assertAcked(client().admin().cluster().prepareReroute().add(new CancelAllocationCommand("index", 0, nodeNames.get(0), true)));
        assertThat(client().admin().cluster().prepareHealth("index").get().getUnassignedShards(), equalTo(1));

        assertThat(client().prepareDelete("index", indexResponse.getId()).get().status(), equalTo(RestStatus.OK));

        assertAcked(client().execute(FreezeIndexAction.INSTANCE,
                new FreezeRequest("index").waitForActiveShards(ActiveShardCount.ONE)).actionGet());

        assertThat(client().admin().cluster().prepareState().get().getState().metadata().index("index").getTimestampRange(),
                sameInstance(IndexLongFieldRange.EMPTY));

        internalCluster().stopRandomNode(InternalTestCluster.nameFilter(nodeNames.get(1)));
        assertThat(client().admin().cluster().prepareHealth("index").get().getUnassignedShards(), equalTo(2));
        assertAcked(client().admin().indices().prepareUpdateSettings("index")
                .setSettings(Settings.builder().putNull(excludeSetting)));
        assertThat(client().admin().cluster().prepareHealth("index").get().getUnassignedShards(), equalTo(2));

        assertAcked(client().admin().cluster().prepareReroute().add(
                new AllocateStalePrimaryAllocationCommand("index", 0, nodeNames.get(0), true)));

        ensureYellowAndNoInitializingShards("index");

        final IndexLongFieldRange timestampFieldRange
                = client().admin().cluster().prepareState().get().getState().metadata().index("index").getTimestampRange();
        assertThat(timestampFieldRange, not(sameInstance(IndexLongFieldRange.UNKNOWN)));
        assertThat(timestampFieldRange, not(sameInstance(IndexLongFieldRange.EMPTY)));
        assertTrue(timestampFieldRange.isComplete());
        assertThat(timestampFieldRange.getMin(), equalTo(Instant.parse("2010-01-06T02:03:04.567Z").getMillis()));
        assertThat(timestampFieldRange.getMax(), equalTo(Instant.parse("2010-01-06T02:03:04.567Z").getMillis()));
    }

    public void testTimestampFieldTypeExposedByAllIndicesServices() throws Exception {
        internalCluster().startNodes(between(2, 4));

        final String locale;
        final String date;

        switch (between(1, 3)) {
            case 1:
                locale = "";
                date = "04 Feb 2020 12:01:23Z";
                break;
            case 2:
                locale = "en_GB";
                date = "04 Feb 2020 12:01:23Z";
                break;
            case 3:
                locale = "fr_FR";
                date = "04 févr. 2020 12:01:23Z";
                break;
            default:
                throw new AssertionError("impossible");
        }

        assertAcked(prepareCreate("index")
                .setSettings(Settings.builder()
                        .put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, 1)
                        .put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, 1))
                .setMapping(jsonBuilder().startObject().startObject("_doc").startObject("properties")
                        .startObject(DataStream.TimestampField.FIXED_TIMESTAMP_FIELD)
                        .field("type", "date")
                        .field("format", "dd LLL yyyy HH:mm:ssX")
                        .field("locale", locale)
                        .endObject()
                        .endObject().endObject().endObject()));

        final Index index = client().admin().cluster().prepareState().clear().setIndices("index").setMetadata(true)
                .get().getState().metadata().index("index").getIndex();

        ensureGreen("index");
        if (randomBoolean()) {
            client().prepareIndex("index").setSource(DataStream.TimestampField.FIXED_TIMESTAMP_FIELD, date).get();
        }

        for (final IndicesService indicesService : internalCluster().getInstances(IndicesService.class)) {
            assertNull(indicesService.getTimestampFieldType(index));
        }

        assertAcked(client().execute(FreezeIndexAction.INSTANCE, new FreezeRequest("index")).actionGet());
        ensureGreen("index");
        for (final IndicesService indicesService : internalCluster().getInstances(IndicesService.class)) {
            final PlainActionFuture<DateFieldMapper.DateFieldType> timestampFieldTypeFuture = new PlainActionFuture<>();
            assertBusy(() -> {
                final DateFieldMapper.DateFieldType timestampFieldType = indicesService.getTimestampFieldType(index);
                assertNotNull(timestampFieldType);
                timestampFieldTypeFuture.onResponse(timestampFieldType);
            });
            assertTrue(timestampFieldTypeFuture.isDone());
            assertThat(timestampFieldTypeFuture.get().dateTimeFormatter().locale().toString(), equalTo(locale));
            assertThat(timestampFieldTypeFuture.get().dateTimeFormatter().parseMillis(date), equalTo(1580817683000L));
        }

        assertAcked(client().execute(FreezeIndexAction.INSTANCE, new FreezeRequest("index").setFreeze(false)).actionGet());
        ensureGreen("index");
        for (final IndicesService indicesService : internalCluster().getInstances(IndicesService.class)) {
            assertNull(indicesService.getTimestampFieldType(index));
        }
    }

    public void testRetryPointInTime() throws Exception {
        internalCluster().ensureAtLeastNumDataNodes(1);
        final List<String> dataNodes =
            StreamSupport.stream(internalCluster().clusterService().state().nodes().getDataNodes().spliterator(), false)
                .map(e -> e.value.getName())
                .collect(Collectors.toList());
        final String assignedNode = randomFrom(dataNodes);
        final String indexName = "test";
        assertAcked(
            client().admin().indices()
                .prepareCreate(indexName)
                .setSettings(Settings.builder().put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, between(1, 5))
                    .put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, 0)
                    .put("index.routing.allocation.require._name", assignedNode)
                    .build())
                .setMapping("{\"properties\":{\"created_date\":{\"type\": \"date\", \"format\": \"yyyy-MM-dd\"}}}"));
        int numDocs = randomIntBetween(1, 100);
        for (int i = 0; i < numDocs; i++) {
            client().prepareIndex(indexName).setSource("created_date", "2011-02-02").get();
        }
        assertAcked(client().execute(FreezeIndexAction.INSTANCE, new FreezeRequest(indexName)).actionGet());
        final String pitId = client().execute(OpenPointInTimeAction.INSTANCE,
            new OpenPointInTimeRequest(new String[]{indexName}, IndicesOptions.STRICT_EXPAND_OPEN_FORBID_CLOSED,
                TimeValue.timeValueMinutes(2), null, null)).actionGet().getSearchContextId();
        try {
            SearchResponse resp = client().prepareSearch()
                .setIndices(indexName)
                .setPreference(null)
                .setPointInTime(new PointInTimeBuilder(pitId))
                .get();
            assertNoFailures(resp);
            assertThat(resp.pointInTimeId(), equalTo(pitId));
            assertHitCount(resp, numDocs);
            internalCluster().restartNode(assignedNode);
            ensureGreen(indexName);
            resp = client().prepareSearch()
                .setIndices(indexName)
                .setQuery(new RangeQueryBuilder("created_date").gte("2011-01-01").lte("2011-12-12"))
                .setSearchType(SearchType.QUERY_THEN_FETCH)
                .setPreference(null)
                .setPreFilterShardSize(between(1, 10))
                .setAllowPartialSearchResults(true)
                .setPointInTime(new PointInTimeBuilder(pitId))
                .get();
            assertNoFailures(resp);
            assertThat(resp.pointInTimeId(), equalTo(pitId));
            assertHitCount(resp, numDocs);
        } finally {
            assertAcked(client().execute(FreezeIndexAction.INSTANCE, new FreezeRequest(indexName).setFreeze(false)).actionGet());
            client().execute(ClosePointInTimeAction.INSTANCE, new ClosePointInTimeRequest(pitId)).actionGet();
        }
    }
}
