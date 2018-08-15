/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.job.persistence;

import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.indices.create.CreateIndexRequest;
import org.elasticsearch.action.admin.indices.mapping.get.GetMappingsResponse;
import org.elasticsearch.action.get.GetResponse;
import org.elasticsearch.action.search.MultiSearchRequest;
import org.elasticsearch.action.search.MultiSearchResponse;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.AckedClusterStateUpdateTask;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.AliasMetaData;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.metadata.MappingMetaData;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.collect.ImmutableOpenMap;
import org.elasticsearch.common.document.DocumentField;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.text.Text;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.index.Index;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.SearchHits;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.ml.action.util.QueryPage;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.persistence.AnomalyDetectorsIndex;
import org.elasticsearch.xpack.core.ml.job.persistence.AnomalyDetectorsIndexFields;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSnapshot;
import org.elasticsearch.xpack.core.ml.job.results.AnomalyRecord;
import org.elasticsearch.xpack.core.ml.job.results.Bucket;
import org.elasticsearch.xpack.core.ml.job.results.CategoryDefinition;
import org.elasticsearch.xpack.core.ml.job.results.Influencer;
import org.elasticsearch.xpack.core.ml.job.results.Result;
import org.elasticsearch.xpack.ml.job.persistence.InfluencersQueryBuilder.InfluencersQuery;
import org.mockito.ArgumentCaptor;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Date;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.Consumer;

import static org.elasticsearch.xpack.core.ml.job.config.JobTests.buildJobBuilder;
import static org.hamcrest.Matchers.equalTo;
import static org.mockito.Matchers.any;
import static org.mockito.Matchers.eq;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.times;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

public class JobResultsProviderTests extends ESTestCase {
    private static final String CLUSTER_NAME = "myCluster";

    @SuppressWarnings("unchecked")
    public void testCreateJobResultsIndex() {
        String resultsIndexName = AnomalyDetectorsIndexFields.RESULTS_INDEX_PREFIX + AnomalyDetectorsIndexFields.RESULTS_INDEX_DEFAULT;
        QueryBuilder jobFilter = QueryBuilders.termQuery("job_id", "foo");

        MockClientBuilder clientBuilder = new MockClientBuilder(CLUSTER_NAME);
        ArgumentCaptor<CreateIndexRequest> captor = ArgumentCaptor.forClass(CreateIndexRequest.class);
        clientBuilder.createIndexRequest(captor, resultsIndexName);
        clientBuilder.prepareAlias(resultsIndexName, AnomalyDetectorsIndex.jobResultsAliasedName("foo"), jobFilter);
        clientBuilder.prepareAlias(resultsIndexName, AnomalyDetectorsIndex.resultsWriteAlias("foo"));

        Job.Builder job = buildJobBuilder("foo");
        JobResultsProvider provider = createProvider(clientBuilder.build());
        AtomicReference<Boolean> resultHolder = new AtomicReference<>();

        ClusterState cs = ClusterState.builder(new ClusterName("_name"))
                .metaData(MetaData.builder().indices(ImmutableOpenMap.of()))
                .build();

        ClusterService clusterService = mock(ClusterService.class);

        doAnswer(invocationOnMock -> {
            AckedClusterStateUpdateTask<Boolean> task = (AckedClusterStateUpdateTask<Boolean>) invocationOnMock.getArguments()[1];
            task.execute(cs);
            return null;
        }).when(clusterService).submitStateUpdateTask(eq("put-job-foo"), any(AckedClusterStateUpdateTask.class));

        provider.createJobResultIndex(job.build(), cs, new ActionListener<Boolean>() {
            @Override
            public void onResponse(Boolean aBoolean) {
                CreateIndexRequest request = captor.getValue();
                assertNotNull(request);
                assertEquals(resultsIndexName, request.index());
                clientBuilder.verifyIndexCreated(resultsIndexName);
                resultHolder.set(aBoolean);
            }

            @Override
            public void onFailure(Exception e) {
                fail(e.toString());
            }
        });

        assertNotNull(resultHolder.get());
        assertTrue(resultHolder.get());
    }

    @SuppressWarnings("unchecked")
    public void testCreateJobWithExistingIndex() {
        QueryBuilder jobFilter = QueryBuilders.termQuery("job_id", "foo");
        MockClientBuilder clientBuilder = new MockClientBuilder(CLUSTER_NAME);
        clientBuilder.prepareAlias(AnomalyDetectorsIndex.jobResultsAliasedName("foo"),
                AnomalyDetectorsIndex.jobResultsAliasedName("foo123"), jobFilter);
        clientBuilder.preparePutMapping(mock(AcknowledgedResponse.class), Result.TYPE.getPreferredName());

        GetMappingsResponse getMappingsResponse = mock(GetMappingsResponse.class);
        ImmutableOpenMap<String, MappingMetaData> typeMappings = ImmutableOpenMap.<String, MappingMetaData>of();

        ImmutableOpenMap<String, ImmutableOpenMap<String, MappingMetaData>> mappings =
                ImmutableOpenMap.<String, ImmutableOpenMap<String, MappingMetaData>>builder()
                        .fPut(AnomalyDetectorsIndex.jobResultsAliasedName("foo"), typeMappings).build();
        when(getMappingsResponse.mappings()).thenReturn(mappings);
        clientBuilder.prepareGetMapping(getMappingsResponse);

        Job.Builder job = buildJobBuilder("foo123");
        job.setResultsIndexName("foo");
        JobResultsProvider provider = createProvider(clientBuilder.build());

        Index index = mock(Index.class);
        when(index.getName()).thenReturn(AnomalyDetectorsIndex.jobResultsAliasedName("foo"));
        IndexMetaData indexMetaData = mock(IndexMetaData.class);
        when(indexMetaData.getIndex()).thenReturn(index);

        ImmutableOpenMap<String, AliasMetaData> aliases = ImmutableOpenMap.of();
        when(indexMetaData.getAliases()).thenReturn(aliases);

        ImmutableOpenMap<String, IndexMetaData> indexMap = ImmutableOpenMap.<String, IndexMetaData>builder()
                .fPut(AnomalyDetectorsIndex.jobResultsAliasedName("foo"), indexMetaData).build();

        ClusterState cs2 = ClusterState.builder(new ClusterName("_name"))
                .metaData(MetaData.builder().indices(indexMap)).build();

        ClusterService clusterService = mock(ClusterService.class);

        doAnswer(invocationOnMock -> {
            AckedClusterStateUpdateTask<Boolean> task = (AckedClusterStateUpdateTask<Boolean>) invocationOnMock.getArguments()[1];
            task.execute(cs2);
            return null;
        }).when(clusterService).submitStateUpdateTask(eq("put-job-foo123"), any(AckedClusterStateUpdateTask.class));

        doAnswer(invocationOnMock -> {
            AckedClusterStateUpdateTask<Boolean> task = (AckedClusterStateUpdateTask<Boolean>) invocationOnMock.getArguments()[1];
            task.execute(cs2);
            return null;
        }).when(clusterService).submitStateUpdateTask(eq("index-aliases"), any(AckedClusterStateUpdateTask.class));

        provider.createJobResultIndex(job.build(), cs2, new ActionListener<Boolean>() {
            @Override
            public void onResponse(Boolean aBoolean) {
                assertTrue(aBoolean);
                verify(clientBuilder.build().admin().indices(), times(1)).preparePutMapping(any());
            }

            @Override
            public void onFailure(Exception e) {
                fail(e.toString());
            }
        });
    }

    @SuppressWarnings("unchecked")
    public void testCreateJobRelatedIndicies_createsAliasBecauseIndexNameIsSet() {
        String indexName = AnomalyDetectorsIndexFields.RESULTS_INDEX_PREFIX + "custom-bar";
        String readAliasName = AnomalyDetectorsIndex.jobResultsAliasedName("foo");
        String writeAliasName = AnomalyDetectorsIndex.resultsWriteAlias("foo");
        QueryBuilder jobFilter = QueryBuilders.termQuery("job_id", "foo");

        MockClientBuilder clientBuilder = new MockClientBuilder(CLUSTER_NAME);
        ArgumentCaptor<CreateIndexRequest> captor = ArgumentCaptor.forClass(CreateIndexRequest.class);
        clientBuilder.createIndexRequest(captor, indexName);
        clientBuilder.prepareAlias(indexName, readAliasName, jobFilter);
        clientBuilder.prepareAlias(indexName, writeAliasName);
        clientBuilder.preparePutMapping(mock(AcknowledgedResponse.class), Result.TYPE.getPreferredName());

        Job.Builder job = buildJobBuilder("foo");
        job.setResultsIndexName("bar");
        Client client = clientBuilder.build();
        JobResultsProvider provider = createProvider(client);

        ImmutableOpenMap<String, IndexMetaData> indexMap = ImmutableOpenMap.<String, IndexMetaData>builder().build();

        ClusterState cs = ClusterState.builder(new ClusterName("_name"))
                .metaData(MetaData.builder().indices(indexMap)).build();

        ClusterService clusterService = mock(ClusterService.class);

        doAnswer(invocationOnMock -> {
            AckedClusterStateUpdateTask<Boolean> task = (AckedClusterStateUpdateTask<Boolean>) invocationOnMock.getArguments()[1];
            task.execute(cs);
            return null;
        }).when(clusterService).submitStateUpdateTask(eq("put-job-foo"), any(AckedClusterStateUpdateTask.class));

        provider.createJobResultIndex(job.build(), cs, new ActionListener<Boolean>() {
            @Override
            public void onResponse(Boolean aBoolean) {
                verify(client.admin().indices(), times(1)).prepareAliases();
                verify(client.admin().indices().prepareAliases(), times(1)).addAlias(indexName, readAliasName, jobFilter);
                verify(client.admin().indices().prepareAliases(), times(1)).addAlias(indexName, writeAliasName);
            }

            @Override
            public void onFailure(Exception e) {
                fail(e.toString());
            }
        });
    }

    public void testBuckets_OneBucketNoInterim() throws IOException {
        String jobId = "TestJobIdentification";
        Date now = new Date();
        List<Map<String, Object>> source = new ArrayList<>();

        Map<String, Object> map = new HashMap<>();
        map.put("job_id", "foo");
        map.put("timestamp", now.getTime());
        map.put("bucket_span", 22);
        source.add(map);

        QueryBuilder[] queryBuilderHolder = new QueryBuilder[1];
        SearchResponse response = createSearchResponse(source);
        int from = 0;
        int size = 10;
        Client client = getMockedClient(queryBuilder -> queryBuilderHolder[0] = queryBuilder, response);
        JobResultsProvider provider = createProvider(client);

        BucketsQueryBuilder bq = new BucketsQueryBuilder().from(from).size(size).anomalyScoreThreshold(1.0);

        @SuppressWarnings({"unchecked"})
        QueryPage<Bucket>[] holder = new QueryPage[1];
        provider.buckets(jobId, bq, r -> holder[0] = r, e -> {throw new RuntimeException(e);}, client);
        QueryPage<Bucket> buckets = holder[0];
        assertEquals(1L, buckets.count());
        QueryBuilder query = queryBuilderHolder[0];
        String queryString = query.toString();
        assertTrue(
                queryString.matches("(?s).*anomaly_score[^}]*from. : 1\\.0.*must_not[^}]*term[^}]*is_interim.*value. : true" +
                        ".*"));
    }

    public void testBuckets_OneBucketInterim() throws IOException {
        String jobId = "TestJobIdentification";
        Date now = new Date();
        List<Map<String, Object>> source = new ArrayList<>();

        Map<String, Object> map = new HashMap<>();
        map.put("job_id", "foo");
        map.put("timestamp", now.getTime());
        map.put("bucket_span", 22);
        source.add(map);

        QueryBuilder[] queryBuilderHolder = new QueryBuilder[1];
        SearchResponse response = createSearchResponse(source);
        int from = 99;
        int size = 17;

        Client client = getMockedClient(queryBuilder -> queryBuilderHolder[0] = queryBuilder, response);
        JobResultsProvider provider = createProvider(client);

        BucketsQueryBuilder bq = new BucketsQueryBuilder().from(from).size(size).anomalyScoreThreshold(5.1)
                .includeInterim(true);

        @SuppressWarnings({"unchecked"})
        QueryPage<Bucket>[] holder = new QueryPage[1];
        provider.buckets(jobId, bq, r -> holder[0] = r, e -> {throw new RuntimeException(e);}, client);
        QueryPage<Bucket> buckets = holder[0];
        assertEquals(1L, buckets.count());
        QueryBuilder query = queryBuilderHolder[0];
        String queryString = query.toString();
        assertTrue(queryString.matches("(?s).*anomaly_score[^}]*from. : 5\\.1.*"));
        assertFalse(queryString.matches("(?s).*is_interim.*"));
    }

    public void testBuckets_UsingBuilder() throws IOException {
        String jobId = "TestJobIdentification";
        Date now = new Date();
        List<Map<String, Object>> source = new ArrayList<>();

        Map<String, Object> map = new HashMap<>();
        map.put("job_id", "foo");
        map.put("timestamp", now.getTime());
        map.put("bucket_span", 22);
        source.add(map);

        QueryBuilder[] queryBuilderHolder = new QueryBuilder[1];
        SearchResponse response = createSearchResponse(source);
        int from = 99;
        int size = 17;

        Client client = getMockedClient(queryBuilder -> queryBuilderHolder[0] = queryBuilder, response);
        JobResultsProvider provider = createProvider(client);

        BucketsQueryBuilder bq = new BucketsQueryBuilder();
        bq.from(from);
        bq.size(size);
        bq.anomalyScoreThreshold(5.1);
        bq.includeInterim(true);

        @SuppressWarnings({"unchecked"})
        QueryPage<Bucket>[] holder = new QueryPage[1];
        provider.buckets(jobId, bq, r -> holder[0] = r, e -> {throw new RuntimeException(e);}, client);
        QueryPage<Bucket> buckets = holder[0];
        assertEquals(1L, buckets.count());
        QueryBuilder query = queryBuilderHolder[0];
        String queryString = query.toString();
        assertTrue(queryString.matches("(?s).*anomaly_score[^}]*from. : 5\\.1.*"));
        assertFalse(queryString.matches("(?s).*is_interim.*"));
    }

    public void testBucket_NoBucketNoExpand() throws IOException {
        String jobId = "TestJobIdentification";
        Long timestamp = 98765432123456789L;
        List<Map<String, Object>> source = new ArrayList<>();

        SearchResponse response = createSearchResponse(source);

        Client client = getMockedClient(queryBuilder -> {}, response);
        JobResultsProvider provider = createProvider(client);

        BucketsQueryBuilder bq = new BucketsQueryBuilder();
        bq.timestamp(Long.toString(timestamp));
        Exception[] holder = new Exception[1];
        provider.buckets(jobId, bq, q -> {}, e -> holder[0] = e, client);
        assertEquals(ResourceNotFoundException.class, holder[0].getClass());
    }

    public void testBucket_OneBucketNoExpand() throws IOException {
        String jobId = "TestJobIdentification";
        Date now = new Date();
        List<Map<String, Object>> source = new ArrayList<>();

        Map<String, Object> map = new HashMap<>();
        map.put("job_id", "foo");
        map.put("timestamp", now.getTime());
        map.put("bucket_span", 22);
        source.add(map);

        SearchResponse response = createSearchResponse(source);
        Client client = getMockedClient(queryBuilder -> {}, response);
        JobResultsProvider provider = createProvider(client);

        BucketsQueryBuilder bq = new BucketsQueryBuilder();
        bq.timestamp(Long.toString(now.getTime()));

        @SuppressWarnings({"unchecked"})
        QueryPage<Bucket>[] bucketHolder = new QueryPage[1];
        provider.buckets(jobId, bq, q -> bucketHolder[0] = q, e -> {}, client);
        assertThat(bucketHolder[0].count(), equalTo(1L));
        Bucket b = bucketHolder[0].results().get(0);
        assertEquals(now, b.getTimestamp());
    }

    public void testRecords() throws IOException {
        String jobId = "TestJobIdentification";
        Date now = new Date();
        List<Map<String, Object>> source = new ArrayList<>();

        Map<String, Object> recordMap1 = new HashMap<>();
        recordMap1.put("job_id", "foo");
        recordMap1.put("typical", 22.4);
        recordMap1.put("actual", 33.3);
        recordMap1.put("timestamp", now.getTime());
        recordMap1.put("function", "irritable");
        recordMap1.put("bucket_span", 22);
        Map<String, Object> recordMap2 = new HashMap<>();
        recordMap2.put("job_id", "foo");
        recordMap2.put("typical", 1122.4);
        recordMap2.put("actual", 933.3);
        recordMap2.put("timestamp", now.getTime());
        recordMap2.put("function", "irrascible");
        recordMap2.put("bucket_span", 22);
        source.add(recordMap1);
        source.add(recordMap2);

        int from = 14;
        int size = 2;
        String sortfield = "minefield";
        SearchResponse response = createSearchResponse(source);
        Client client = getMockedClient(qb -> {}, response);
        JobResultsProvider provider = createProvider(client);

        RecordsQueryBuilder rqb = new RecordsQueryBuilder().from(from).size(size).epochStart(String.valueOf(now.getTime()))
                .epochEnd(String.valueOf(now.getTime())).includeInterim(true).sortField(sortfield)
                .recordScore(2.2);

        @SuppressWarnings({"unchecked"})
        QueryPage<AnomalyRecord>[] holder = new QueryPage[1];
        provider.records(jobId, rqb, page -> holder[0] = page, RuntimeException::new, client);
        QueryPage<AnomalyRecord> recordPage = holder[0];
        assertEquals(2L, recordPage.count());
        List<AnomalyRecord> records = recordPage.results();
        assertEquals(22.4, records.get(0).getTypical().get(0), 0.000001);
        assertEquals(33.3, records.get(0).getActual().get(0), 0.000001);
        assertEquals("irritable", records.get(0).getFunction());
        assertEquals(1122.4, records.get(1).getTypical().get(0), 0.000001);
        assertEquals(933.3, records.get(1).getActual().get(0), 0.000001);
        assertEquals("irrascible", records.get(1).getFunction());
    }

    public void testRecords_UsingBuilder() throws IOException {
        String jobId = "TestJobIdentification";
        Date now = new Date();
        List<Map<String, Object>> source = new ArrayList<>();

        Map<String, Object> recordMap1 = new HashMap<>();
        recordMap1.put("job_id", "foo");
        recordMap1.put("typical", 22.4);
        recordMap1.put("actual", 33.3);
        recordMap1.put("timestamp", now.getTime());
        recordMap1.put("function", "irritable");
        recordMap1.put("bucket_span", 22);
        Map<String, Object> recordMap2 = new HashMap<>();
        recordMap2.put("job_id", "foo");
        recordMap2.put("typical", 1122.4);
        recordMap2.put("actual", 933.3);
        recordMap2.put("timestamp", now.getTime());
        recordMap2.put("function", "irrascible");
        recordMap2.put("bucket_span", 22);
        source.add(recordMap1);
        source.add(recordMap2);

        int from = 14;
        int size = 2;
        String sortfield = "minefield";
        SearchResponse response = createSearchResponse(source);

        Client client = getMockedClient(qb -> {}, response);
        JobResultsProvider provider = createProvider(client);

        RecordsQueryBuilder rqb = new RecordsQueryBuilder();
        rqb.from(from);
        rqb.size(size);
        rqb.epochStart(String.valueOf(now.getTime()));
        rqb.epochEnd(String.valueOf(now.getTime()));
        rqb.includeInterim(true);
        rqb.sortField(sortfield);
        rqb.recordScore(2.2);

        @SuppressWarnings({"unchecked"})
        QueryPage<AnomalyRecord>[] holder = new QueryPage[1];
        provider.records(jobId, rqb, page -> holder[0] = page, RuntimeException::new, client);
        QueryPage<AnomalyRecord> recordPage = holder[0];
        assertEquals(2L, recordPage.count());
        List<AnomalyRecord> records = recordPage.results();
        assertEquals(22.4, records.get(0).getTypical().get(0), 0.000001);
        assertEquals(33.3, records.get(0).getActual().get(0), 0.000001);
        assertEquals("irritable", records.get(0).getFunction());
        assertEquals(1122.4, records.get(1).getTypical().get(0), 0.000001);
        assertEquals(933.3, records.get(1).getActual().get(0), 0.000001);
        assertEquals("irrascible", records.get(1).getFunction());
    }

    public void testBucketRecords() throws IOException {
        String jobId = "TestJobIdentification";
        Date now = new Date();
        Bucket bucket = mock(Bucket.class);
        when(bucket.getTimestamp()).thenReturn(now);

        List<Map<String, Object>> source = new ArrayList<>();
        Map<String, Object> recordMap1 = new HashMap<>();
        recordMap1.put("job_id", "foo");
        recordMap1.put("typical", 22.4);
        recordMap1.put("actual", 33.3);
        recordMap1.put("timestamp", now.getTime());
        recordMap1.put("function", "irritable");
        recordMap1.put("bucket_span", 22);
        Map<String, Object> recordMap2 = new HashMap<>();
        recordMap2.put("job_id", "foo");
        recordMap2.put("typical", 1122.4);
        recordMap2.put("actual", 933.3);
        recordMap2.put("timestamp", now.getTime());
        recordMap2.put("function", "irrascible");
        recordMap2.put("bucket_span", 22);
        source.add(recordMap1);
        source.add(recordMap2);

        int from = 14;
        int size = 2;
        String sortfield = "minefield";
        SearchResponse response = createSearchResponse(source);
        Client client = getMockedClient(qb -> {}, response);
        JobResultsProvider provider = createProvider(client);

        @SuppressWarnings({"unchecked"})
        QueryPage<AnomalyRecord>[] holder = new QueryPage[1];
        provider.bucketRecords(jobId, bucket, from, size, true, sortfield, true, page -> holder[0] = page, RuntimeException::new,
                client);
        QueryPage<AnomalyRecord> recordPage = holder[0];
        assertEquals(2L, recordPage.count());
        List<AnomalyRecord> records = recordPage.results();

        assertEquals(22.4, records.get(0).getTypical().get(0), 0.000001);
        assertEquals(33.3, records.get(0).getActual().get(0), 0.000001);
        assertEquals("irritable", records.get(0).getFunction());
        assertEquals(1122.4, records.get(1).getTypical().get(0), 0.000001);
        assertEquals(933.3, records.get(1).getActual().get(0), 0.000001);
        assertEquals("irrascible", records.get(1).getFunction());
    }

    public void testexpandBucket() throws IOException {
        String jobId = "TestJobIdentification";
        Date now = new Date();
        Bucket bucket = new Bucket("foo", now, 22);

        List<Map<String, Object>> source = new ArrayList<>();
        for (int i = 0; i < 400; i++) {
            Map<String, Object> recordMap = new HashMap<>();
            recordMap.put("job_id", "foo");
            recordMap.put("typical", 22.4 + i);
            recordMap.put("actual", 33.3 + i);
            recordMap.put("timestamp", now.getTime());
            recordMap.put("function", "irritable");
            recordMap.put("bucket_span", 22);
            source.add(recordMap);
        }

        SearchResponse response = createSearchResponse(source);
        Client client = getMockedClient(qb -> {}, response);
        JobResultsProvider provider = createProvider(client);

        Integer[] holder = new Integer[1];
        provider.expandBucket(jobId, false, bucket, records -> holder[0] = records, RuntimeException::new, client);
        int records = holder[0];
        assertEquals(400L, records);
    }

    public void testCategoryDefinitions() throws IOException {
        String jobId = "TestJobIdentification";
        String terms = "the terms and conditions are not valid here";
        List<Map<String, Object>> source = new ArrayList<>();

        Map<String, Object> map = new HashMap<>();
        map.put("job_id", "foo");
        map.put("category_id", String.valueOf(map.hashCode()));
        map.put("terms", terms);

        source.add(map);

        SearchResponse response = createSearchResponse(source);
        int from = 0;
        int size = 10;
        Client client = getMockedClient(q -> {}, response);

        JobResultsProvider provider = createProvider(client);
        @SuppressWarnings({"unchecked"})
        QueryPage<CategoryDefinition>[] holder = new QueryPage[1];
        provider.categoryDefinitions(jobId, null, false, from, size, r -> holder[0] = r,
                e -> {throw new RuntimeException(e);}, client);
        QueryPage<CategoryDefinition> categoryDefinitions = holder[0];
        assertEquals(1L, categoryDefinitions.count());
        assertEquals(terms, categoryDefinitions.results().get(0).getTerms());
    }

    public void testCategoryDefinition() throws IOException {
        String jobId = "TestJobIdentification";
        String terms = "the terms and conditions are not valid here";

        Map<String, Object> source = new HashMap<>();
        long categoryId = source.hashCode();
        source.put("job_id", "foo");
        source.put("category_id", categoryId);
        source.put("terms", terms);

        SearchResponse response = createSearchResponse(Collections.singletonList(source));
        Client client = getMockedClient(q -> {}, response);
        JobResultsProvider provider = createProvider(client);
        @SuppressWarnings({"unchecked"})
        QueryPage<CategoryDefinition>[] holder = new QueryPage[1];
        provider.categoryDefinitions(jobId, categoryId, false, null, null,
                r -> holder[0] = r, e -> {throw new RuntimeException(e);}, client);
        QueryPage<CategoryDefinition> categoryDefinitions = holder[0];
        assertEquals(1L, categoryDefinitions.count());
        assertEquals(terms, categoryDefinitions.results().get(0).getTerms());
    }

    public void testInfluencers_NoInterim() throws IOException {
        String jobId = "TestJobIdentificationForInfluencers";
        Date now = new Date();
        List<Map<String, Object>> source = new ArrayList<>();

        Map<String, Object> influencerMap1 = new HashMap<>();
        influencerMap1.put("job_id", "foo");
        influencerMap1.put("probability", 0.555);
        influencerMap1.put("influencer_field_name", "Builder");
        influencerMap1.put("timestamp", now.getTime());
        influencerMap1.put("influencer_field_value", "Bob");
        influencerMap1.put("initial_influencer_score", 22.2);
        influencerMap1.put("influencer_score", 22.6);
        influencerMap1.put("bucket_span", 123);
        Map<String, Object> recordMap2 = new HashMap<>();
        recordMap2.put("job_id", "foo");
        recordMap2.put("probability", 0.99);
        recordMap2.put("influencer_field_name", "Builder");
        recordMap2.put("timestamp", now.getTime());
        recordMap2.put("influencer_field_value", "James");
        recordMap2.put("initial_influencer_score", 5.0);
        recordMap2.put("influencer_score", 5.0);
        recordMap2.put("bucket_span", 123);
        source.add(influencerMap1);
        source.add(recordMap2);

        int from = 4;
        int size = 3;
        QueryBuilder[] qbHolder = new QueryBuilder[1];
        SearchResponse response = createSearchResponse(source);
        Client client = getMockedClient(q -> qbHolder[0] = q, response);
        JobResultsProvider provider = createProvider(client);

        @SuppressWarnings({"unchecked"})
        QueryPage<Influencer>[] holder = new QueryPage[1];
        InfluencersQuery query = new InfluencersQueryBuilder().from(from).size(size).includeInterim(false).build();
        provider.influencers(jobId, query, page -> holder[0] = page, RuntimeException::new, client);
        QueryPage<Influencer> page = holder[0];
        assertEquals(2L, page.count());

        String queryString = qbHolder[0].toString();
        assertTrue(queryString.matches("(?s).*must_not[^}]*term[^}]*is_interim.*value. : true.*"));

        List<Influencer> records = page.results();
        assertEquals("foo", records.get(0).getJobId());
        assertEquals("Bob", records.get(0).getInfluencerFieldValue());
        assertEquals("Builder", records.get(0).getInfluencerFieldName());
        assertEquals(now, records.get(0).getTimestamp());
        assertEquals(0.555, records.get(0).getProbability(), 0.00001);
        assertEquals(22.6, records.get(0).getInfluencerScore(), 0.00001);
        assertEquals(22.2, records.get(0).getInitialInfluencerScore(), 0.00001);

        assertEquals("James", records.get(1).getInfluencerFieldValue());
        assertEquals("Builder", records.get(1).getInfluencerFieldName());
        assertEquals(now, records.get(1).getTimestamp());
        assertEquals(0.99, records.get(1).getProbability(), 0.00001);
        assertEquals(5.0, records.get(1).getInfluencerScore(), 0.00001);
        assertEquals(5.0, records.get(1).getInitialInfluencerScore(), 0.00001);
    }

    public void testInfluencers_WithInterim() throws IOException {
        String jobId = "TestJobIdentificationForInfluencers";
        Date now = new Date();
        List<Map<String, Object>> source = new ArrayList<>();

        Map<String, Object> influencerMap1 = new HashMap<>();
        influencerMap1.put("job_id", "foo");
        influencerMap1.put("probability", 0.555);
        influencerMap1.put("influencer_field_name", "Builder");
        influencerMap1.put("timestamp", now.getTime());
        influencerMap1.put("influencer_field_value", "Bob");
        influencerMap1.put("initial_influencer_score", 22.2);
        influencerMap1.put("influencer_score", 22.6);
        influencerMap1.put("bucket_span", 123);
        Map<String, Object> influencerMap2 = new HashMap<>();
        influencerMap2.put("job_id", "foo");
        influencerMap2.put("probability", 0.99);
        influencerMap2.put("influencer_field_name", "Builder");
        influencerMap2.put("timestamp", now.getTime());
        influencerMap2.put("influencer_field_value", "James");
        influencerMap2.put("initial_influencer_score", 5.0);
        influencerMap2.put("influencer_score", 5.0);
        influencerMap2.put("bucket_span", 123);
        source.add(influencerMap1);
        source.add(influencerMap2);

        int from = 4;
        int size = 3;
        QueryBuilder[] qbHolder = new QueryBuilder[1];
        SearchResponse response = createSearchResponse(source);
        Client client = getMockedClient(q -> qbHolder[0] = q, response);
        JobResultsProvider provider = createProvider(client);

        @SuppressWarnings({"unchecked"})
        QueryPage<Influencer>[] holder = new QueryPage[1];
        InfluencersQuery query = new InfluencersQueryBuilder().from(from).size(size).start("0").end("0").sortField("sort")
                .sortDescending(true).influencerScoreThreshold(0.0).includeInterim(true).build();
        provider.influencers(jobId, query, page -> holder[0] = page, RuntimeException::new, client);
        QueryPage<Influencer> page = holder[0];
        assertEquals(2L, page.count());

        String queryString = qbHolder[0].toString();
        assertFalse(queryString.matches("(?s).*isInterim.*"));

        List<Influencer> records = page.results();
        assertEquals("Bob", records.get(0).getInfluencerFieldValue());
        assertEquals("Builder", records.get(0).getInfluencerFieldName());
        assertEquals(now, records.get(0).getTimestamp());
        assertEquals(0.555, records.get(0).getProbability(), 0.00001);
        assertEquals(22.6, records.get(0).getInfluencerScore(), 0.00001);
        assertEquals(22.2, records.get(0).getInitialInfluencerScore(), 0.00001);

        assertEquals("James", records.get(1).getInfluencerFieldValue());
        assertEquals("Builder", records.get(1).getInfluencerFieldName());
        assertEquals(now, records.get(1).getTimestamp());
        assertEquals(0.99, records.get(1).getProbability(), 0.00001);
        assertEquals(5.0, records.get(1).getInfluencerScore(), 0.00001);
        assertEquals(5.0, records.get(1).getInitialInfluencerScore(), 0.00001);
    }

    public void testModelSnapshots() throws IOException {
        String jobId = "TestJobIdentificationForInfluencers";
        Date now = new Date();
        List<Map<String, Object>> source = new ArrayList<>();

        Map<String, Object> recordMap1 = new HashMap<>();
        recordMap1.put("job_id", "foo");
        recordMap1.put("description", "snapshot1");
        recordMap1.put("timestamp", now.getTime());
        recordMap1.put("snapshot_doc_count", 5);
        recordMap1.put("latest_record_time_stamp", now.getTime());
        recordMap1.put("latest_result_time_stamp", now.getTime());
        Map<String, Object> recordMap2 = new HashMap<>();
        recordMap2.put("job_id", "foo");
        recordMap2.put("description", "snapshot2");
        recordMap2.put("timestamp", now.getTime());
        recordMap2.put("snapshot_doc_count", 6);
        recordMap2.put("latest_record_time_stamp", now.getTime());
        recordMap2.put("latest_result_time_stamp", now.getTime());
        source.add(recordMap1);
        source.add(recordMap2);

        int from = 4;
        int size = 3;
        SearchResponse response = createSearchResponse(source);
        Client client = getMockedClient(qb -> {}, response);
        JobResultsProvider provider = createProvider(client);

        @SuppressWarnings({"unchecked"})
        QueryPage<ModelSnapshot>[] holder = new QueryPage[1];
        provider.modelSnapshots(jobId, from, size, r -> holder[0] = r, RuntimeException::new);
        QueryPage<ModelSnapshot> page = holder[0];
        assertEquals(2L, page.count());
        List<ModelSnapshot> snapshots = page.results();

        assertEquals("foo", snapshots.get(0).getJobId());
        assertEquals(now, snapshots.get(0).getTimestamp());
        assertEquals(now, snapshots.get(0).getLatestRecordTimeStamp());
        assertEquals(now, snapshots.get(0).getLatestResultTimeStamp());
        assertEquals("snapshot1", snapshots.get(0).getDescription());
        assertEquals(5, snapshots.get(0).getSnapshotDocCount());

        assertEquals(now, snapshots.get(1).getTimestamp());
        assertEquals(now, snapshots.get(1).getLatestRecordTimeStamp());
        assertEquals(now, snapshots.get(1).getLatestResultTimeStamp());
        assertEquals("snapshot2", snapshots.get(1).getDescription());
        assertEquals(6, snapshots.get(1).getSnapshotDocCount());
    }

    public void testViolatedFieldCountLimit() throws Exception {
        Map<String, Object> mapping = new HashMap<>();
        for (int i = 0; i < 10; i++) {
            mapping.put("field" + i, Collections.singletonMap("type", "string"));
        }

        IndexMetaData.Builder indexMetaData1 = new IndexMetaData.Builder("index1")
                .settings(Settings.builder()
                        .put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
                        .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
                        .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0))
                .putMapping(new MappingMetaData("type1", Collections.singletonMap("properties", mapping)));
        MetaData metaData = MetaData.builder()
                .put(indexMetaData1)
                .build();
        boolean result = JobResultsProvider.violatedFieldCountLimit("index1", 0, 10,
                ClusterState.builder(new ClusterName("_name")).metaData(metaData).build());
        assertFalse(result);

        result = JobResultsProvider.violatedFieldCountLimit("index1", 1, 10,
                ClusterState.builder(new ClusterName("_name")).metaData(metaData).build());
        assertTrue(result);

        IndexMetaData.Builder indexMetaData2 = new IndexMetaData.Builder("index1")
                .settings(Settings.builder()
                        .put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
                        .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
                        .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0))
                .putMapping(new MappingMetaData("type1", Collections.singletonMap("properties", mapping)))
                .putMapping(new MappingMetaData("type2", Collections.singletonMap("properties", mapping)));
        metaData = MetaData.builder()
                .put(indexMetaData2)
                .build();
        result = JobResultsProvider.violatedFieldCountLimit("index1", 0, 19,
                ClusterState.builder(new ClusterName("_name")).metaData(metaData).build());
        assertTrue(result);
    }

    public void testCountFields() {
        Map<String, Object> mapping = new HashMap<>();
        mapping.put("field1", Collections.singletonMap("type", "string"));
        mapping.put("field2", Collections.singletonMap("type", "string"));
        mapping.put("field3", Collections.singletonMap("type", "string"));
        assertEquals(3, JobResultsProvider.countFields(Collections.singletonMap("properties", mapping)));

        Map<String, Object> objectProperties = new HashMap<>();
        objectProperties.put("field4", Collections.singletonMap("type", "string"));
        objectProperties.put("field5", Collections.singletonMap("type", "string"));
        objectProperties.put("field6", Collections.singletonMap("type", "string"));
        Map<String, Object> objectField = new HashMap<>();
        objectField.put("type", "object");
        objectField.put("properties", objectProperties);

        mapping.put("field4", objectField);
        assertEquals(7, JobResultsProvider.countFields(Collections.singletonMap("properties", mapping)));
    }

    private Bucket createBucketAtEpochTime(long epoch) {
        return new Bucket("foo", new Date(epoch), 123);
    }

    private JobResultsProvider createProvider(Client client) {
        return new JobResultsProvider(client, Settings.EMPTY);
    }

    private static GetResponse createGetResponse(boolean exists, Map<String, Object> source) throws IOException {
        GetResponse getResponse = mock(GetResponse.class);
        when(getResponse.isExists()).thenReturn(exists);
        when(getResponse.getSourceAsBytesRef()).thenReturn(BytesReference.bytes(XContentFactory.jsonBuilder().map(source)));
        return getResponse;
    }

    private static SearchResponse createSearchResponse(List<Map<String, Object>> source) throws IOException {
        SearchResponse response = mock(SearchResponse.class);
        List<SearchHit> list = new ArrayList<>();

        for (Map<String, Object> map : source) {
            Map<String, Object> _source = new HashMap<>(map);

            Map<String, DocumentField> fields = new HashMap<>();
            fields.put("field_1", new DocumentField("field_1", Collections.singletonList("foo")));
            fields.put("field_2", new DocumentField("field_2", Collections.singletonList("foo")));

            SearchHit hit = new SearchHit(123, String.valueOf(map.hashCode()), new Text("foo"), fields)
                    .sourceRef(BytesReference.bytes(XContentFactory.jsonBuilder().map(_source)));

            list.add(hit);
        }
        SearchHits hits = new SearchHits(list.toArray(new SearchHit[0]), source.size(), 1);
        when(response.getHits()).thenReturn(hits);

        return response;
    }

    private Client getMockedClient(Consumer<QueryBuilder> queryBuilderConsumer, SearchResponse response) {
        Client client = mock(Client.class);
        ThreadPool threadPool = mock(ThreadPool.class);
        when(client.threadPool()).thenReturn(threadPool);
        when(threadPool.getThreadContext()).thenReturn(new ThreadContext(Settings.EMPTY));
        doAnswer(invocationOnMock -> {
            MultiSearchRequest multiSearchRequest = (MultiSearchRequest) invocationOnMock.getArguments()[0];
            queryBuilderConsumer.accept(multiSearchRequest.requests().get(0).source().query());
            @SuppressWarnings("unchecked")
            ActionListener<MultiSearchResponse> actionListener = (ActionListener<MultiSearchResponse>) invocationOnMock.getArguments()[1];
            MultiSearchResponse mresponse = new MultiSearchResponse(
                    new MultiSearchResponse.Item[]{new MultiSearchResponse.Item(response, null)},
                    randomNonNegativeLong());
            actionListener.onResponse(mresponse);
            return null;
        }).when(client).multiSearch(any(), any());
        doAnswer(invocationOnMock -> {
            SearchRequest searchRequest = (SearchRequest) invocationOnMock.getArguments()[0];
            queryBuilderConsumer.accept(searchRequest.source().query());
            @SuppressWarnings("unchecked")
            ActionListener<SearchResponse> actionListener = (ActionListener<SearchResponse>) invocationOnMock.getArguments()[1];
            actionListener.onResponse(response);
            return null;
        }).when(client).search(any(), any());
        return client;
    }
}
