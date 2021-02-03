/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.search;

import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.search.CollectionTerminatedException;
import org.apache.lucene.search.ScoreMode;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.cluster.node.stats.NodeStats;
import org.elasticsearch.action.admin.cluster.node.stats.NodesStatsResponse;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.index.IndexResponse;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.breaker.CircuitBreaker;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.concurrent.AtomicArray;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.query.RangeQueryBuilder;
import org.elasticsearch.index.shard.IndexShard;
import org.elasticsearch.indices.IndicesService;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.plugins.SearchPlugin;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.search.DocValueFormat;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.aggregations.AbstractAggregationBuilder;
import org.elasticsearch.search.aggregations.AggregationBuilder;
import org.elasticsearch.search.aggregations.Aggregations;
import org.elasticsearch.search.aggregations.Aggregator;
import org.elasticsearch.search.aggregations.AggregatorBase;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.elasticsearch.search.aggregations.AggregatorFactory;
import org.elasticsearch.search.aggregations.CardinalityUpperBound;
import org.elasticsearch.search.aggregations.InternalAggregation;
import org.elasticsearch.search.aggregations.LeafBucketCollector;
import org.elasticsearch.search.aggregations.bucket.terms.LongTerms;
import org.elasticsearch.search.aggregations.bucket.terms.TermsAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.InternalMax;
import org.elasticsearch.search.aggregations.support.AggregationContext;
import org.elasticsearch.search.aggregations.support.ValueType;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.search.fetch.FetchSubPhase;
import org.elasticsearch.search.fetch.FetchSubPhaseProcessor;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.test.ESIntegTestCase;

import java.io.IOException;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;

import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;

public class TransportSearchIT extends ESIntegTestCase {
    public static class TestPlugin extends Plugin implements SearchPlugin {
        @Override
        public List<AggregationSpec> getAggregations() {
            return Collections.singletonList(
                new AggregationSpec(TestAggregationBuilder.NAME, TestAggregationBuilder::new, TestAggregationBuilder.PARSER)
                    .addResultReader(InternalMax::new)
            );
        }

        @Override
        public List<FetchSubPhase> getFetchSubPhases(FetchPhaseConstructionContext context) {
            /**
             * Set up a fetch sub phase that throws an exception on indices whose name that start with "boom".
             */
            return Collections.singletonList(fetchContext -> new FetchSubPhaseProcessor() {
                @Override
                public void setNextReader(LeafReaderContext readerContext) {
                }

                @Override
                public void process(FetchSubPhase.HitContext hitContext) {
                    if (fetchContext.getIndexName().startsWith("boom")) {
                        throw new RuntimeException("boom");
                    }
                }
            });
        }
    }

    @Override
    protected Settings nodeSettings(int nodeOrdinal) {
        return Settings.builder()
            .put(super.nodeSettings(nodeOrdinal))
            .put("indices.breaker.request.type", "memory")
            .build();
    }

    @Override
    protected Collection<Class<? extends Plugin>> nodePlugins() {
        return Collections.singletonList(TestPlugin.class);
    }

    public void testLocalClusterAlias() {
        long nowInMillis = randomLongBetween(0, Long.MAX_VALUE);
        IndexRequest indexRequest = new IndexRequest("test");
        indexRequest.id("1");
        indexRequest.source("field", "value");
        indexRequest.setRefreshPolicy(WriteRequest.RefreshPolicy.WAIT_UNTIL);
        IndexResponse indexResponse = client().index(indexRequest).actionGet();
        assertEquals(RestStatus.CREATED, indexResponse.status());
        TaskId parentTaskId = new TaskId("node", randomNonNegativeLong());

        {
            SearchRequest searchRequest = SearchRequest.subSearchRequest(parentTaskId, new SearchRequest(), Strings.EMPTY_ARRAY,
                "local", nowInMillis, randomBoolean());
            SearchResponse searchResponse = client().search(searchRequest).actionGet();
            assertEquals(1, searchResponse.getHits().getTotalHits().value);
            SearchHit[] hits = searchResponse.getHits().getHits();
            assertEquals(1, hits.length);
            SearchHit hit = hits[0];
            assertEquals("local", hit.getClusterAlias());
            assertEquals("test", hit.getIndex());
            assertEquals("1", hit.getId());
        }
        {
            SearchRequest searchRequest = SearchRequest.subSearchRequest(parentTaskId, new SearchRequest(), Strings.EMPTY_ARRAY,
                "", nowInMillis, randomBoolean());
            SearchResponse searchResponse = client().search(searchRequest).actionGet();
            assertEquals(1, searchResponse.getHits().getTotalHits().value);
            SearchHit[] hits = searchResponse.getHits().getHits();
            assertEquals(1, hits.length);
            SearchHit hit = hits[0];
            assertEquals("", hit.getClusterAlias());
            assertEquals("test", hit.getIndex());
            assertEquals("1", hit.getId());
        }
    }

    public void testAbsoluteStartMillis() {
        TaskId parentTaskId = new TaskId("node", randomNonNegativeLong());
        {
            IndexRequest indexRequest = new IndexRequest("test-1970.01.01");
            indexRequest.id("1");
            indexRequest.source("date", "1970-01-01");
            indexRequest.setRefreshPolicy(WriteRequest.RefreshPolicy.WAIT_UNTIL);
            IndexResponse indexResponse = client().index(indexRequest).actionGet();
            assertEquals(RestStatus.CREATED, indexResponse.status());
        }
        {
            IndexRequest indexRequest = new IndexRequest("test-1982.01.01");
            indexRequest.id("1");
            indexRequest.source("date", "1982-01-01");
            indexRequest.setRefreshPolicy(WriteRequest.RefreshPolicy.WAIT_UNTIL);
            IndexResponse indexResponse = client().index(indexRequest).actionGet();
            assertEquals(RestStatus.CREATED, indexResponse.status());
        }
        {
            SearchRequest searchRequest = new SearchRequest();
            SearchResponse searchResponse = client().search(searchRequest).actionGet();
            assertEquals(2, searchResponse.getHits().getTotalHits().value);
        }
        {
            SearchRequest searchRequest = new SearchRequest("<test-{now/d}>");
            searchRequest.indicesOptions(IndicesOptions.fromOptions(true, true, true, true));
            SearchResponse searchResponse = client().search(searchRequest).actionGet();
            assertEquals(0, searchResponse.getTotalShards());
        }
        {
            SearchRequest searchRequest = SearchRequest.subSearchRequest(parentTaskId, new SearchRequest(),
                Strings.EMPTY_ARRAY, "", 0, randomBoolean());
            SearchResponse searchResponse = client().search(searchRequest).actionGet();
            assertEquals(2, searchResponse.getHits().getTotalHits().value);
        }
        {
            SearchRequest searchRequest = SearchRequest.subSearchRequest(parentTaskId, new SearchRequest(),
                Strings.EMPTY_ARRAY, "", 0, randomBoolean());
            searchRequest.indices("<test-{now/d}>");
            SearchResponse searchResponse = client().search(searchRequest).actionGet();
            assertEquals(1, searchResponse.getHits().getTotalHits().value);
            assertEquals("test-1970.01.01", searchResponse.getHits().getHits()[0].getIndex());
        }
        {
            SearchRequest searchRequest = SearchRequest.subSearchRequest(parentTaskId, new SearchRequest(),
                Strings.EMPTY_ARRAY, "", 0, randomBoolean());
            SearchSourceBuilder sourceBuilder = new SearchSourceBuilder();
            RangeQueryBuilder rangeQuery = new RangeQueryBuilder("date");
            rangeQuery.gte("1970-01-01");
            rangeQuery.lt("1982-01-01");
            sourceBuilder.query(rangeQuery);
            searchRequest.source(sourceBuilder);
            SearchResponse searchResponse = client().search(searchRequest).actionGet();
            assertEquals(1, searchResponse.getHits().getTotalHits().value);
            assertEquals("test-1970.01.01", searchResponse.getHits().getHits()[0].getIndex());
        }
    }

    public void testFinalReduce()  {
        long nowInMillis = randomLongBetween(0, Long.MAX_VALUE);
        TaskId taskId = new TaskId("node", randomNonNegativeLong());
        {
            IndexRequest indexRequest = new IndexRequest("test");
            indexRequest.id("1");
            indexRequest.source("price", 10);
            IndexResponse indexResponse = client().index(indexRequest).actionGet();
            assertEquals(RestStatus.CREATED, indexResponse.status());
        }
        {
            IndexRequest indexRequest = new IndexRequest("test");
            indexRequest.id("2");
            indexRequest.source("price", 100);
            IndexResponse indexResponse = client().index(indexRequest).actionGet();
            assertEquals(RestStatus.CREATED, indexResponse.status());
        }
        client().admin().indices().prepareRefresh("test").get();

        SearchRequest originalRequest = new SearchRequest();
        SearchSourceBuilder source = new SearchSourceBuilder();
        source.size(0);
        originalRequest.source(source);
        TermsAggregationBuilder terms = new TermsAggregationBuilder("terms").userValueTypeHint(ValueType.NUMERIC);
        terms.field("price");
        terms.size(1);
        source.aggregation(terms);

        {
            SearchRequest searchRequest = randomBoolean() ? originalRequest : SearchRequest.subSearchRequest(taskId, originalRequest,
                Strings.EMPTY_ARRAY, "remote", nowInMillis, true);
            SearchResponse searchResponse = client().search(searchRequest).actionGet();
            assertEquals(2, searchResponse.getHits().getTotalHits().value);
            Aggregations aggregations = searchResponse.getAggregations();
            LongTerms longTerms = aggregations.get("terms");
            assertEquals(1, longTerms.getBuckets().size());
        }
        {
            SearchRequest searchRequest = SearchRequest.subSearchRequest(taskId, originalRequest,
                Strings.EMPTY_ARRAY, "remote", nowInMillis, false);
            SearchResponse searchResponse = client().search(searchRequest).actionGet();
            assertEquals(2, searchResponse.getHits().getTotalHits().value);
            Aggregations aggregations = searchResponse.getAggregations();
            LongTerms longTerms = aggregations.get("terms");
            assertEquals(2, longTerms.getBuckets().size());
        }
    }

    public void testShardCountLimit() throws Exception {
        try {
            final int numPrimaries1 = randomIntBetween(2, 10);
            final int numPrimaries2 = randomIntBetween(1, 10);
            assertAcked(prepareCreate("test1")
                    .setSettings(Settings.builder().put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, numPrimaries1)));
            assertAcked(prepareCreate("test2")
                    .setSettings(Settings.builder().put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, numPrimaries2)));

            // no exception
            client().prepareSearch("test1").get();

            assertAcked(client().admin().cluster().prepareUpdateSettings()
                    .setTransientSettings(Collections.singletonMap(
                            TransportSearchAction.SHARD_COUNT_LIMIT_SETTING.getKey(), numPrimaries1 - 1)));

            IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
                    () -> client().prepareSearch("test1").get());
            assertThat(e.getMessage(), containsString("Trying to query " + numPrimaries1
                    + " shards, which is over the limit of " + (numPrimaries1 - 1)));

            assertAcked(client().admin().cluster().prepareUpdateSettings()
                    .setTransientSettings(Collections.singletonMap(
                            TransportSearchAction.SHARD_COUNT_LIMIT_SETTING.getKey(), numPrimaries1)));

            // no exception
            client().prepareSearch("test1").get();

            e = expectThrows(IllegalArgumentException.class,
                    () -> client().prepareSearch("test1", "test2").get());
            assertThat(e.getMessage(), containsString("Trying to query " + (numPrimaries1 + numPrimaries2)
                    + " shards, which is over the limit of " + numPrimaries1));

        } finally {
            assertAcked(client().admin().cluster().prepareUpdateSettings()
                    .setTransientSettings(Collections.singletonMap(
                            TransportSearchAction.SHARD_COUNT_LIMIT_SETTING.getKey(), null)));
        }
    }

    public void testSearchIdle() throws Exception {
        int numOfReplicas = randomIntBetween(0, 1);
        internalCluster().ensureAtLeastNumDataNodes(numOfReplicas + 1);
        final Settings.Builder settings = Settings.builder()
            .put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, randomIntBetween(1, 5))
            .put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, numOfReplicas)
            .put(IndexSettings.INDEX_SEARCH_IDLE_AFTER.getKey(), TimeValue.timeValueMillis(randomIntBetween(50, 500)));
        assertAcked(prepareCreate("test").setSettings(settings)
            .setMapping("{\"properties\":{\"created_date\":{\"type\": \"date\", \"format\": \"yyyy-MM-dd\"}}}"));
        ensureGreen("test");
        assertBusy(() -> {
            for (String node : internalCluster().nodesInclude("test")) {
                final IndicesService indicesService = internalCluster().getInstance(IndicesService.class, node);
                for (IndexShard indexShard : indicesService.indexServiceSafe(resolveIndex("test"))) {
                    assertTrue(indexShard.isSearchIdle());
                }
            }
        });
        client().prepareIndex("test").setId("1").setSource("created_date", "2020-01-01").get();
        client().prepareIndex("test").setId("2").setSource("created_date", "2020-01-02").get();
        client().prepareIndex("test").setId("3").setSource("created_date", "2020-01-03").get();
        assertBusy(() -> {
            SearchResponse resp = client().prepareSearch("test")
                .setQuery(new RangeQueryBuilder("created_date").gte("2020-01-02").lte("2020-01-03"))
                .setPreFilterShardSize(randomIntBetween(1, 3)).get();
            assertThat(resp.getHits().getTotalHits().value, equalTo(2L));
        });
    }

    public void testCircuitBreakerReduceFail() throws Exception {
        int numShards = randomIntBetween(1, 10);
        indexSomeDocs("test", numShards, numShards*3);

        {
            final AtomicArray<Boolean> responses = new AtomicArray<>(10);
            final CountDownLatch latch = new CountDownLatch(10);
            for (int i = 0; i < 10; i++) {
                int batchReduceSize = randomIntBetween(2, Math.max(numShards + 1, 3));
                SearchRequest request = client().prepareSearch("test")
                    .addAggregation(new TestAggregationBuilder("test"))
                    .setBatchedReduceSize(batchReduceSize)
                    .request();
                final int index = i;
                client().search(request, new ActionListener<>() {
                    @Override
                    public void onResponse(SearchResponse response) {
                        responses.set(index, true);
                        latch.countDown();
                    }

                    @Override
                    public void onFailure(Exception e) {
                        responses.set(index, false);
                        latch.countDown();
                    }
                });
            }
            latch.await();
            assertThat(responses.asList().size(), equalTo(10));
            for (boolean resp : responses.asList()) {
                assertTrue(resp);
            }
            assertBusy(() -> assertThat(requestBreakerUsed(), equalTo(0L)));
        }

        try {
            Settings settings = Settings.builder()
                .put("indices.breaker.request.limit", "1b")
                .build();
            assertAcked(client().admin().cluster().prepareUpdateSettings().setTransientSettings(settings));
            final Client client = client();
            assertBusy(() -> {
                Exception exc = expectThrows(Exception.class, () -> client.prepareSearch("test")
                    .addAggregation(new TestAggregationBuilder("test"))
                    .get());
                assertThat(exc.getCause().getMessage(), containsString("<reduce_aggs>"));
            });

            final AtomicArray<Exception> exceptions = new AtomicArray<>(10);
            final CountDownLatch latch = new CountDownLatch(10);
            for (int i = 0; i < 10; i++) {
                int batchReduceSize = randomIntBetween(2, Math.max(numShards + 1, 3));
                SearchRequest request = client().prepareSearch("test")
                    .addAggregation(new TestAggregationBuilder("test"))
                    .setBatchedReduceSize(batchReduceSize)
                    .request();
                final int index = i;
                client().search(request, new ActionListener<>() {
                    @Override
                    public void onResponse(SearchResponse response) {
                        latch.countDown();
                    }

                    @Override
                    public void onFailure(Exception exc) {
                        exceptions.set(index, exc);
                        latch.countDown();
                    }
                });
            }
            latch.await();
            assertThat(exceptions.asList().size(), equalTo(10));
            for (Exception exc : exceptions.asList()) {
                assertThat(exc.getCause().getMessage(), containsString("<reduce_aggs>"));
            }
            assertBusy(() -> assertThat(requestBreakerUsed(), equalTo(0L)));
        } finally {
            Settings settings = Settings.builder()
                .putNull("indices.breaker.request.limit")
                .build();
            assertAcked(client().admin().cluster().prepareUpdateSettings().setTransientSettings(settings));
        }
    }

    public void testCircuitBreakerFetchFail() throws Exception {
        int numShards = randomIntBetween(1, 10);
        int numDocs = numShards*10;
        indexSomeDocs("boom", numShards, numDocs);

        final AtomicArray<Exception> exceptions = new AtomicArray<>(10);
        final CountDownLatch latch = new CountDownLatch(10);
        for (int i = 0; i < 10; i++) {
            int batchReduceSize = randomIntBetween(2, Math.max(numShards + 1, 3));
            SearchRequest request = client().prepareSearch("boom")
                .setBatchedReduceSize(batchReduceSize)
                .setAllowPartialSearchResults(false)
                .request();
            final int index = i;
            client().search(request, new ActionListener<>() {
                @Override
                public void onResponse(SearchResponse response) {
                    latch.countDown();
                }

                @Override
                public void onFailure(Exception exc) {
                    exceptions.set(index, exc);
                    latch.countDown();
                }
            });
        }
        latch.await();
        assertThat(exceptions.asList().size(), equalTo(10));
        for (Exception exc : exceptions.asList()) {
            assertThat(exc.getCause().getMessage(), containsString("boom"));
        }
        assertBusy(() -> assertThat(requestBreakerUsed(), equalTo(0L)));
    }

    private void indexSomeDocs(String indexName, int numberOfShards, int numberOfDocs) {
        createIndex(indexName, Settings.builder().put("index.number_of_shards", numberOfShards).build());

        for (int i = 0; i < numberOfDocs; i++) {
            IndexResponse indexResponse  = client().prepareIndex(indexName)
                .setSource("number", randomInt())
                .get();
            assertEquals(RestStatus.CREATED, indexResponse.status());
        }
        client().admin().indices().prepareRefresh(indexName).get();
    }

    private long requestBreakerUsed() {
        NodesStatsResponse stats = client().admin().cluster().prepareNodesStats().setBreaker(true).get();
        long estimated = 0;
        for (NodeStats nodeStats : stats.getNodes()) {
            estimated += nodeStats.getBreaker().getStats(CircuitBreaker.REQUEST).getEstimated();
        }
        return estimated;
    }

    /**
     * A test aggregation that doesn't consume circuit breaker memory when running on shards.
     * It is used to test the behavior of the circuit breaker when reducing multiple aggregations
     * together (coordinator node).
     */
    private static class TestAggregationBuilder extends AbstractAggregationBuilder<TestAggregationBuilder> {
        static final String NAME = "test";

        private static final ObjectParser<TestAggregationBuilder, String> PARSER =
            ObjectParser.fromBuilder(NAME, TestAggregationBuilder::new);

        TestAggregationBuilder(String name) {
            super(name);
        }

        TestAggregationBuilder(StreamInput input) throws IOException {
            super(input);
        }


        @Override
        protected void doWriteTo(StreamOutput out) throws IOException {
            // noop
        }

        @Override
        protected AggregatorFactory doBuild(AggregationContext context,
                                            AggregatorFactory parent,
                                            AggregatorFactories.Builder subFactoriesBuilder) throws IOException {
            return new AggregatorFactory(name, context, parent, subFactoriesBuilder, metadata) {
                @Override
                protected Aggregator createInternal(Aggregator parent, CardinalityUpperBound cardinality, Map<String, Object> metadata)
                    throws IOException {
                    return new TestAggregator(name, parent);
                }
            };
        }

        @Override
        protected XContentBuilder internalXContent(XContentBuilder builder, Params params) throws IOException {
            return builder;
        }

        @Override
        protected AggregationBuilder shallowCopy(AggregatorFactories.Builder factoriesBuilder, Map<String, Object> metadata) {
            return new TestAggregationBuilder(name);
        }

        @Override
        public BucketCardinality bucketCardinality() {
            return BucketCardinality.NONE;
        }

        @Override
        public String getType() {
            return "test";
        }

        @Override
        public long bytesToPreallocate() {
            return 0;
        }
    }

    /**
     * A test aggregator that extends {@link Aggregator} instead of {@link AggregatorBase}
     * to avoid tripping the circuit breaker when executing on a shard.
     */
    private static class TestAggregator extends Aggregator {
        private final String name;
        private final Aggregator parent;

        private TestAggregator(String name, Aggregator parent) {
            this.name = name;
            this.parent = parent;
        }


        @Override
        public String name() {
            return name;
        }

        @Override
        public Aggregator parent() {
            return parent;
        }

        @Override
        public Aggregator subAggregator(String name) {
            return null;
        }

        @Override
        public InternalAggregation[] buildAggregations(long[] owningBucketOrds) throws IOException {
            return new InternalAggregation[] {
                buildEmptyAggregation()
            };
        }

        @Override
        public InternalAggregation buildEmptyAggregation() {
            return new InternalMax(name(), Double.NaN, DocValueFormat.RAW, null);
        }

        @Override
        public void close() {}

        @Override
        public LeafBucketCollector getLeafCollector(LeafReaderContext ctx) throws IOException {
            throw new CollectionTerminatedException();
        }

        @Override
        public ScoreMode scoreMode() {
            return ScoreMode.COMPLETE_NO_SCORES;
        }

        @Override
        public void preCollection() throws IOException {}

        @Override
        public Aggregator[] subAggregators() {
            throw new UnsupportedOperationException();
        }
    }
}
