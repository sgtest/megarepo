/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */


package org.elasticsearch.action.bulk;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.admin.indices.alias.Alias;
import org.elasticsearch.action.admin.indices.datastream.GetDataStreamAction;
import org.elasticsearch.action.admin.indices.get.GetIndexRequest;
import org.elasticsearch.action.admin.indices.get.GetIndexResponse;
import org.elasticsearch.action.admin.indices.mapping.get.GetMappingsResponse;
import org.elasticsearch.action.admin.indices.template.delete.DeleteIndexTemplateV2Action;
import org.elasticsearch.action.admin.indices.template.put.PutIndexTemplateRequest;
import org.elasticsearch.action.admin.indices.template.put.PutIndexTemplateV2Action;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.index.IndexResponse;
import org.elasticsearch.action.ingest.PutPipelineRequest;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.action.support.replication.ReplicationRequest;
import org.elasticsearch.cluster.metadata.DataStream;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.metadata.IndexTemplateV2;
import org.elasticsearch.cluster.metadata.Template;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.ingest.IngestTestPlugin;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.test.ESIntegTestCase;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;

import static org.elasticsearch.action.DocWriteRequest.OpType.CREATE;
import static org.elasticsearch.action.DocWriteResponse.Result.CREATED;
import static org.elasticsearch.action.DocWriteResponse.Result.UPDATED;
import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.elasticsearch.test.StreamsUtils.copyToStringFromClasspath;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;
import static org.hamcrest.Matchers.arrayWithSize;
import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThanOrEqualTo;
import static org.hamcrest.Matchers.hasItemInArray;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.oneOf;

public class BulkIntegrationIT extends ESIntegTestCase {
    @Override
    protected Collection<Class<? extends Plugin>> nodePlugins() {
        return Arrays.asList(IngestTestPlugin.class);
    }

    public void testBulkIndexCreatesMapping() throws Exception {
        String bulkAction = copyToStringFromClasspath("/org/elasticsearch/action/bulk/bulk-log.json");
        BulkRequestBuilder bulkBuilder = client().prepareBulk();
        bulkBuilder.add(bulkAction.getBytes(StandardCharsets.UTF_8), 0, bulkAction.length(), null, XContentType.JSON);
        bulkBuilder.get();
        assertBusy(() -> {
            GetMappingsResponse mappingsResponse = client().admin().indices().prepareGetMappings().get();
            assertTrue(mappingsResponse.getMappings().containsKey("logstash-2014.03.30"));
        });
    }

    /**
     * This tests that the {@link TransportBulkAction} evaluates alias routing values correctly when dealing with
     * an alias pointing to multiple indices, while a write index exits.
     */
    public void testBulkWithWriteIndexAndRouting() {
        Map<String, Integer> twoShardsSettings = Collections.singletonMap(IndexMetadata.SETTING_NUMBER_OF_SHARDS, 2);
        client().admin().indices().prepareCreate("index1")
            .addAlias(new Alias("alias1").indexRouting("0")).setSettings(twoShardsSettings).get();
        client().admin().indices().prepareCreate("index2")
            .addAlias(new Alias("alias1").indexRouting("0").writeIndex(randomFrom(false, null)))
            .setSettings(twoShardsSettings).get();
        client().admin().indices().prepareCreate("index3")
            .addAlias(new Alias("alias1").indexRouting("1").writeIndex(true)).setSettings(twoShardsSettings).get();

        IndexRequest indexRequestWithAlias = new IndexRequest("alias1").id("id");
        if (randomBoolean()) {
            indexRequestWithAlias.routing("1");
        }
        indexRequestWithAlias.source(Collections.singletonMap("foo", "baz"));
        BulkResponse bulkResponse = client().prepareBulk().add(indexRequestWithAlias).get();
        assertThat(bulkResponse.getItems()[0].getResponse().getIndex(), equalTo("index3"));
        assertThat(bulkResponse.getItems()[0].getResponse().getShardId().getId(), equalTo(0));
        assertThat(bulkResponse.getItems()[0].getResponse().getVersion(), equalTo(1L));
        assertThat(bulkResponse.getItems()[0].getResponse().status(), equalTo(RestStatus.CREATED));
        assertThat(client().prepareGet("index3", "id").setRouting("1").get().getSource().get("foo"), equalTo("baz"));

        bulkResponse = client().prepareBulk().add(client().prepareUpdate("alias1", "id").setDoc("foo", "updated")).get();
        assertFalse(bulkResponse.buildFailureMessage(), bulkResponse.hasFailures());
        assertThat(client().prepareGet("index3", "id").setRouting("1").get().getSource().get("foo"), equalTo("updated"));
        bulkResponse = client().prepareBulk().add(client().prepareDelete("alias1", "id")).get();
        assertFalse(bulkResponse.buildFailureMessage(), bulkResponse.hasFailures());
        assertFalse(client().prepareGet("index3", "id").setRouting("1").get().isExists());
    }

    // allowing the auto-generated timestamp to externally be set would allow making the index inconsistent with duplicate docs
    public void testExternallySetAutoGeneratedTimestamp() {
        IndexRequest indexRequest = new IndexRequest("index1").source(Collections.singletonMap("foo", "baz"));
        indexRequest.process(Version.CURRENT, null, null); // sets the timestamp
        if (randomBoolean()) {
            indexRequest.id("test");
        }
        assertThat(expectThrows(IllegalArgumentException.class, () -> client().prepareBulk().add(indexRequest).get()).getMessage(),
            containsString("autoGeneratedTimestamp should not be set externally"));
    }

    public void testBulkWithGlobalDefaults() throws Exception {
        // all requests in the json are missing index and type parameters: "_index" : "test", "_type" : "type1",
        String bulkAction = copyToStringFromClasspath("/org/elasticsearch/action/bulk/simple-bulk-missing-index-type.json");
        {
            BulkRequestBuilder bulkBuilder = client().prepareBulk();
            bulkBuilder.add(bulkAction.getBytes(StandardCharsets.UTF_8), 0, bulkAction.length(), null, XContentType.JSON);
            ActionRequestValidationException ex = expectThrows(ActionRequestValidationException.class, bulkBuilder::get);

            assertThat(ex.validationErrors(), containsInAnyOrder(
                "index is missing",
                "index is missing",
                "index is missing"));
        }

        {
            createSamplePipeline("pipeline");
            BulkRequestBuilder bulkBuilder = client().prepareBulk("test")
                .routing("routing")
                .pipeline("pipeline");

            bulkBuilder.add(bulkAction.getBytes(StandardCharsets.UTF_8), 0, bulkAction.length(), null, XContentType.JSON);
            BulkResponse bulkItemResponses = bulkBuilder.get();
            assertFalse(bulkItemResponses.hasFailures());
        }
    }

    private void createSamplePipeline(String pipelineId) throws IOException, ExecutionException, InterruptedException {
        XContentBuilder pipeline = jsonBuilder()
            .startObject()
                .startArray("processors")
                    .startObject()
                        .startObject("test")
                        .endObject()
                    .endObject()
                .endArray()
            .endObject();

        AcknowledgedResponse acknowledgedResponse = client().admin()
            .cluster()
            .putPipeline(new PutPipelineRequest(pipelineId, BytesReference.bytes(pipeline), XContentType.JSON))
            .get();

        assertTrue(acknowledgedResponse.isAcknowledged());
    }

    /** This test ensures that index deletion makes indexing fail quickly, not wait on the index that has disappeared */
    public void testDeleteIndexWhileIndexing() throws Exception {
        String index = "deleted_while_indexing";
        createIndex(index);
        AtomicBoolean stopped = new AtomicBoolean();
        Thread[] threads = new Thread[between(1, 4)];
        AtomicInteger docID = new AtomicInteger();
        for (int i = 0; i < threads.length; i++) {
            threads[i] = new Thread(() -> {
                while (stopped.get() == false && docID.get() < 5000) {
                    String id = Integer.toString(docID.incrementAndGet());
                    try {
                        IndexResponse response = client().prepareIndex(index).setId(id)
                            .setSource(Map.of("f" + randomIntBetween(1, 10), randomNonNegativeLong()), XContentType.JSON).get();
                        assertThat(response.getResult(), is(oneOf(CREATED, UPDATED)));
                        logger.info("--> index id={} seq_no={}", response.getId(), response.getSeqNo());
                    } catch (ElasticsearchException ignore) {
                        logger.info("--> fail to index id={}", id);
                    }
                }
            });
            threads[i].start();
        }
        ensureGreen(index);
        assertBusy(() -> assertThat(docID.get(), greaterThanOrEqualTo(1)));
        assertAcked(client().admin().indices().prepareDelete(index));
        stopped.set(true);
        for (Thread thread : threads) {
            thread.join(ReplicationRequest.DEFAULT_TIMEOUT.millis() / 2);
            assertFalse(thread.isAlive());
        }
    }

    public void testMixedAutoCreate() {
        Settings settings = Settings.builder().put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, 0).build();

        PutIndexTemplateV2Action.Request createTemplateRequest = new PutIndexTemplateV2Action.Request("logs-foo");
        createTemplateRequest.indexTemplate(
            new IndexTemplateV2(
                List.of("logs-foo*"),
                new Template(settings, null, null),
                null, null, null, null,
                new IndexTemplateV2.DataStreamTemplate("@timestamp"))
        );
        client().execute(PutIndexTemplateV2Action.INSTANCE, createTemplateRequest).actionGet();

        BulkRequest bulkRequest = new BulkRequest();
        bulkRequest.add(new IndexRequest("logs-foobar").opType(CREATE).source("{}", XContentType.JSON));
        bulkRequest.add(new IndexRequest("logs-foobaz").opType(CREATE).source("{}", XContentType.JSON));
        bulkRequest.add(new IndexRequest("logs-barbaz").opType(CREATE).source("{}", XContentType.JSON));
        bulkRequest.add(new IndexRequest("logs-barfoo").opType(CREATE).source("{}", XContentType.JSON));
        BulkResponse bulkResponse = client().bulk(bulkRequest).actionGet();
        assertThat("bulk failures: " + Strings.toString(bulkResponse), bulkResponse.hasFailures(), is(false));

        bulkRequest = new BulkRequest();
        bulkRequest.add(new IndexRequest("logs-foobar").opType(CREATE).source("{}", XContentType.JSON));
        bulkRequest.add(new IndexRequest("logs-foobaz2").opType(CREATE).source("{}", XContentType.JSON));
        bulkRequest.add(new IndexRequest("logs-barbaz").opType(CREATE).source("{}", XContentType.JSON));
        bulkRequest.add(new IndexRequest("logs-barfoo2").opType(CREATE).source("{}", XContentType.JSON));
        bulkResponse = client().bulk(bulkRequest).actionGet();
        assertThat("bulk failures: " + Strings.toString(bulkResponse), bulkResponse.hasFailures(), is(false));

        bulkRequest = new BulkRequest();
        bulkRequest.add(new IndexRequest("logs-foobar").opType(CREATE).source("{}", XContentType.JSON));
        bulkRequest.add(new IndexRequest("logs-foobaz2").opType(CREATE).source("{}", XContentType.JSON));
        bulkRequest.add(new IndexRequest("logs-foobaz3").opType(CREATE).source("{}", XContentType.JSON));
        bulkRequest.add(new IndexRequest("logs-barbaz").opType(CREATE).source("{}", XContentType.JSON));
        bulkRequest.add(new IndexRequest("logs-barfoo2").opType(CREATE).source("{}", XContentType.JSON));
        bulkRequest.add(new IndexRequest("logs-barfoo3").opType(CREATE).source("{}", XContentType.JSON));
        bulkResponse = client().bulk(bulkRequest).actionGet();
        assertThat("bulk failures: " + Strings.toString(bulkResponse), bulkResponse.hasFailures(), is(false));

        GetDataStreamAction.Request getDataStreamRequest = new GetDataStreamAction.Request("*");
        GetDataStreamAction.Response getDataStreamsResponse = client().admin().indices().getDataStreams(getDataStreamRequest).actionGet();
        assertThat(getDataStreamsResponse.getDataStreams(), hasSize(4));
        getDataStreamsResponse.getDataStreams().sort(Comparator.comparing(DataStream::getName));
        assertThat(getDataStreamsResponse.getDataStreams().get(0).getName(), equalTo("logs-foobar"));
        assertThat(getDataStreamsResponse.getDataStreams().get(1).getName(), equalTo("logs-foobaz"));
        assertThat(getDataStreamsResponse.getDataStreams().get(2).getName(), equalTo("logs-foobaz2"));
        assertThat(getDataStreamsResponse.getDataStreams().get(3).getName(), equalTo("logs-foobaz3"));

        GetIndexResponse getIndexResponse = client().admin().indices().getIndex(new GetIndexRequest().indices("logs-bar*")).actionGet();
        assertThat(getIndexResponse.getIndices(), arrayWithSize(4));
        assertThat(getIndexResponse.getIndices(), hasItemInArray("logs-barbaz"));
        assertThat(getIndexResponse.getIndices(), hasItemInArray("logs-barfoo"));
        assertThat(getIndexResponse.getIndices(), hasItemInArray("logs-barfoo2"));
        assertThat(getIndexResponse.getIndices(), hasItemInArray("logs-barfoo3"));

        DeleteIndexTemplateV2Action.Request deleteTemplateRequest = new DeleteIndexTemplateV2Action.Request("*");
        client().execute(DeleteIndexTemplateV2Action.INSTANCE, deleteTemplateRequest).actionGet();
    }

    public void testAutoCreateV1TemplateNoDataStream() {
        Settings settings = Settings.builder().put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, 0).build();

        PutIndexTemplateRequest v1Request = new PutIndexTemplateRequest("logs-foo");
        v1Request.patterns(List.of("logs-foo*"));
        v1Request.settings(settings);
        v1Request.order(Integer.MAX_VALUE); // in order to avoid number_of_replicas being overwritten by random_template
        client().admin().indices().putTemplate(v1Request).actionGet();

        BulkRequest bulkRequest = new BulkRequest();
        bulkRequest.add(new IndexRequest("logs-foobar").opType(CREATE).source("{}", XContentType.JSON));
        BulkResponse bulkResponse = client().bulk(bulkRequest).actionGet();
        assertThat("bulk failures: " + Strings.toString(bulkResponse), bulkResponse.hasFailures(), is(false));

        GetDataStreamAction.Request getDataStreamRequest = new GetDataStreamAction.Request("*");
        GetDataStreamAction.Response getDataStreamsResponse = client().admin().indices().getDataStreams(getDataStreamRequest).actionGet();
        assertThat(getDataStreamsResponse.getDataStreams(), hasSize(0));

        GetIndexResponse getIndexResponse = client().admin().indices().getIndex(new GetIndexRequest().indices("logs-foobar")).actionGet();
        assertThat(getIndexResponse.getIndices(), arrayWithSize(1));
        assertThat(getIndexResponse.getIndices(), hasItemInArray("logs-foobar"));
        assertThat(getIndexResponse.getSettings().get("logs-foobar").get(IndexMetadata.SETTING_NUMBER_OF_REPLICAS), equalTo("0"));
    }
}
