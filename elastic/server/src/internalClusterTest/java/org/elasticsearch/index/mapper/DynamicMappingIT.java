/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.index.mapper;

import org.elasticsearch.action.DocWriteResponse;
import org.elasticsearch.action.admin.indices.mapping.get.GetMappingsResponse;
import org.elasticsearch.action.bulk.BulkRequest;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.index.IndexRequestBuilder;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateUpdateTask;
import org.elasticsearch.cluster.metadata.MappingMetadata;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Randomness;
import org.elasticsearch.common.geo.GeoPoint;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.index.query.GeoBoundingBoxQueryBuilder;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.test.ESIntegTestCase;
import org.elasticsearch.test.InternalSettingsPlugin;
import org.hamcrest.Matchers;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicReference;

import static org.elasticsearch.index.mapper.MapperService.INDEX_MAPPING_TOTAL_FIELDS_LIMIT_SETTING;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertSearchHits;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.instanceOf;

public class DynamicMappingIT extends ESIntegTestCase {

    @Override
    protected Collection<Class<? extends Plugin>> nodePlugins() {
        return Collections.singleton(InternalSettingsPlugin.class);
    }

    public void testConflictingDynamicMappings() {
        // we don't use indexRandom because the order of requests is important here
        createIndex("index");
        client().prepareIndex("index").setId("1").setSource("foo", 3).get();
        try {
            client().prepareIndex("index").setId("2").setSource("foo", "bar").get();
            fail("Indexing request should have failed!");
        } catch (MapperParsingException e) {
            // general case, the parsing code complains that it can't parse "bar" as a "long"
            assertThat(e.getMessage(),
                    Matchers.containsString("failed to parse field [foo] of type [long]"));
        } catch (IllegalArgumentException e) {
            // rare case: the node that processes the index request doesn't have the mappings
            // yet and sends a mapping update to the master node to map "bar" as "text". This
            // fails as it had been already mapped as a long by the previous index request.
            assertThat(e.getMessage(),
                    Matchers.containsString("mapper [foo] cannot be changed from type [long] to [text]"));
        }
    }

    public void testConflictingDynamicMappingsBulk() {
        // we don't use indexRandom because the order of requests is important here
        createIndex("index");
        client().prepareIndex("index").setId("1").setSource("foo", 3).get();
        BulkResponse bulkResponse = client().prepareBulk().add(client().prepareIndex("index").setId("1").setSource("foo", 3)).get();
        assertFalse(bulkResponse.hasFailures());
        bulkResponse = client().prepareBulk().add(client().prepareIndex("index").setId("2").setSource("foo", "bar")).get();
        assertTrue(bulkResponse.hasFailures());
    }

    private static void assertMappingsHaveField(GetMappingsResponse mappings, String index, String field) throws IOException {
        MappingMetadata indexMappings = mappings.getMappings().get("index");
        assertNotNull(indexMappings);
        Map<String, Object> typeMappingsMap = indexMappings.getSourceAsMap();
        Map<String, Object> properties = (Map<String, Object>) typeMappingsMap.get("properties");
        assertTrue("Could not find [" + field + "] in " + typeMappingsMap.toString(), properties.containsKey(field));
    }

    public void testConcurrentDynamicUpdates() throws Throwable {
        createIndex("index");
        final Thread[] indexThreads = new Thread[32];
        final CountDownLatch startLatch = new CountDownLatch(1);
        final AtomicReference<Throwable> error = new AtomicReference<>();
        for (int i = 0; i < indexThreads.length; ++i) {
            final String id = Integer.toString(i);
            indexThreads[i] = new Thread(new Runnable() {
                @Override
                public void run() {
                    try {
                        startLatch.await();
                        assertEquals(DocWriteResponse.Result.CREATED, client().prepareIndex("index").setId(id)
                            .setSource("field" + id, "bar").get().getResult());
                    } catch (Exception e) {
                        error.compareAndSet(null, e);
                    }
                }
            });
            indexThreads[i].start();
        }
        startLatch.countDown();
        for (Thread thread : indexThreads) {
            thread.join();
        }
        if (error.get() != null) {
            throw error.get();
        }
        Thread.sleep(2000);
        GetMappingsResponse mappings = client().admin().indices().prepareGetMappings("index").get();
        for (int i = 0; i < indexThreads.length; ++i) {
            assertMappingsHaveField(mappings, "index", "field" + i);
        }
        for (int i = 0; i < indexThreads.length; ++i) {
            assertTrue(client().prepareGet("index", Integer.toString(i)).get().isExists());
        }
    }

    public void testPreflightCheckAvoidsMaster() throws InterruptedException {
        createIndex("index", Settings.builder().put(INDEX_MAPPING_TOTAL_FIELDS_LIMIT_SETTING.getKey(), 2).build());
        ensureGreen("index");
        client().prepareIndex("index").setId("1").setSource("field1", "value1").get();

        final CountDownLatch masterBlockedLatch = new CountDownLatch(1);
        final CountDownLatch indexingCompletedLatch = new CountDownLatch(1);

        internalCluster().getInstance(ClusterService.class, internalCluster().getMasterName()).submitStateUpdateTask("block-state-updates",
            new ClusterStateUpdateTask() {
            @Override
            public ClusterState execute(ClusterState currentState) throws Exception {
                masterBlockedLatch.countDown();
                indexingCompletedLatch.await();
                return currentState;
            }

            @Override
            public void onFailure(String source, Exception e) {
                throw new AssertionError("unexpected", e);
            }
        });

        masterBlockedLatch.await();
        final IndexRequestBuilder indexRequestBuilder = client().prepareIndex("index").setId("2").setSource("field2", "value2");
        try {
            assertThat(
                expectThrows(IllegalArgumentException.class, () -> indexRequestBuilder.get(TimeValue.timeValueSeconds(10))).getMessage(),
                Matchers.containsString("Limit of total fields [2] has been exceeded"));
        } finally {
            indexingCompletedLatch.countDown();
        }
    }

    public void testMappingVersionAfterDynamicMappingUpdate() throws Exception {
        createIndex("test");
        final ClusterService clusterService = internalCluster().clusterService();
        final long previousVersion = clusterService.state().metadata().index("test").getMappingVersion();
        client().prepareIndex("test").setId("1").setSource("field", "text").get();
        assertBusy(() -> assertThat(clusterService.state().metadata().index("test").getMappingVersion(), equalTo(1 + previousVersion)));
    }

    public void testBulkRequestWithDynamicTemplates() throws Exception {
        final XContentBuilder mappings = XContentFactory.jsonBuilder();
        mappings.startObject();
        {
            mappings.startArray("dynamic_templates");
            {
                mappings.startObject();
                mappings.startObject("location");
                {
                    if (randomBoolean()) {
                        mappings.field("match", "location");
                    }
                    if (randomBoolean()) {
                        mappings.field("match_mapping_type", "string");
                    }
                    mappings.startObject("mapping");
                    {
                        mappings.field("type", "geo_point");
                    }
                    mappings.endObject();
                }
                mappings.endObject();
                mappings.endObject();
            }
            mappings.endArray();
        }
        mappings.endObject();
        assertAcked(client().admin().indices().prepareCreate("test").setMapping(mappings));
        List<IndexRequest> requests = new ArrayList<>();
        requests.add(new IndexRequest("test").id("1").source("location", "41.12,-71.34")
            .setDynamicTemplates(Map.of("location", "location")));
        requests.add(new IndexRequest("test").id("2").source(
            XContentFactory.jsonBuilder()
                .startObject()
                .startObject("location").field("lat", 41.12).field("lon", -71.34).endObject()
                .endObject())
            .setDynamicTemplates(Map.of("location", "location")));
        requests.add(new IndexRequest("test").id("3").source("address.location", "41.12,-71.34")
            .setDynamicTemplates(Map.of("address.location", "location")));
        requests.add(new IndexRequest("test").id("4").source("location", new double[]{-71.34, 41.12})
            .setDynamicTemplates(Map.of("location", "location")));
        requests.add(new IndexRequest("test").id("5").source("array_of_numbers", new double[]{-71.34, 41.12}));

        Randomness.shuffle(requests);
        BulkRequest bulkRequest = new BulkRequest().setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);
        requests.forEach(bulkRequest::add);
        final BulkResponse bulkResponse = client().bulk(bulkRequest).actionGet();
        assertFalse(bulkResponse.hasFailures());

        SearchResponse searchResponse = client().prepareSearch("test")
            .setQuery(new GeoBoundingBoxQueryBuilder("location").setCorners(new GeoPoint(42, -72), new GeoPoint(40, -74)))
            .get();
        assertSearchHits(searchResponse, "1", "2", "4");
        searchResponse = client().prepareSearch("test")
            .setQuery(new GeoBoundingBoxQueryBuilder("address.location").setCorners(new GeoPoint(42, -72), new GeoPoint(40, -74)))
            .get();
        assertSearchHits(searchResponse, "3");
    }

    public void testBulkRequestWithNotFoundDynamicTemplate() throws Exception {
        assertAcked(client().admin().indices().prepareCreate("test"));
        final XContentBuilder mappings = XContentFactory.jsonBuilder();
        mappings.startObject();
        {
            mappings.startArray("dynamic_templates");
            {
                if (randomBoolean()) {
                    mappings.startObject();
                    mappings.startObject("location");
                    {
                        if (randomBoolean()) {
                            mappings.field("match", "location");
                        }
                        if (randomBoolean()) {
                            mappings.field("match_mapping_type", "string");
                        }
                        mappings.startObject("mapping");
                        {
                            mappings.field("type", "geo_point");
                        }
                        mappings.endObject();
                    }
                    mappings.endObject();
                    mappings.endObject();
                }
            }
            mappings.endArray();
        }
        mappings.endObject();

        BulkRequest bulkRequest = new BulkRequest().setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);
        bulkRequest.add(
            new IndexRequest("test").id("1").source(
                XContentFactory.jsonBuilder()
                    .startObject()
                    .field("my_location", "41.12,-71.34")
                    .endObject())
                .setDynamicTemplates(Map.of("my_location", "foo_bar")),
            new IndexRequest("test").id("2").source(
                XContentFactory.jsonBuilder()
                    .startObject()
                    .field("address.location", "41.12,-71.34")
                    .endObject())
                .setDynamicTemplates(Map.of("address.location", "bar_foo"))
        );
        final BulkResponse bulkItemResponses = client().bulk(bulkRequest).actionGet();
        assertTrue(bulkItemResponses.hasFailures());
        assertThat(bulkItemResponses.getItems()[0].getFailure().getCause(), instanceOf(MapperParsingException.class));
        assertThat(bulkItemResponses.getItems()[0].getFailureMessage(),
            containsString("Can't find dynamic template for dynamic template name [foo_bar] of field [my_location]"));
        assertThat(bulkItemResponses.getItems()[1].getFailure().getCause(), instanceOf(MapperParsingException.class));
        assertThat(bulkItemResponses.getItems()[1].getFailureMessage(),
            containsString("Can't find dynamic template for dynamic template name [bar_foo] of field [address.location]"));
    }
}
