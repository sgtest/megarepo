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

package org.elasticsearch.client.documentation;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.LatchedActionListener;
import org.elasticsearch.action.admin.indices.alias.Alias;
import org.elasticsearch.action.admin.indices.alias.IndicesAliasesRequest;
import org.elasticsearch.action.admin.indices.alias.IndicesAliasesRequest.AliasActions;
import org.elasticsearch.action.admin.indices.alias.IndicesAliasesResponse;
import org.elasticsearch.action.admin.indices.alias.get.GetAliasesRequest;
import org.elasticsearch.action.admin.indices.cache.clear.ClearIndicesCacheRequest;
import org.elasticsearch.action.admin.indices.cache.clear.ClearIndicesCacheResponse;
import org.elasticsearch.action.admin.indices.close.CloseIndexRequest;
import org.elasticsearch.action.admin.indices.close.CloseIndexResponse;
import org.elasticsearch.action.admin.indices.create.CreateIndexRequest;
import org.elasticsearch.action.admin.indices.create.CreateIndexResponse;
import org.elasticsearch.action.admin.indices.delete.DeleteIndexRequest;
import org.elasticsearch.action.admin.indices.delete.DeleteIndexResponse;
import org.elasticsearch.action.admin.indices.flush.FlushRequest;
import org.elasticsearch.action.admin.indices.flush.FlushResponse;
import org.elasticsearch.action.admin.indices.forcemerge.ForceMergeRequest;
import org.elasticsearch.action.admin.indices.forcemerge.ForceMergeResponse;
import org.elasticsearch.action.admin.indices.get.GetIndexRequest;
import org.elasticsearch.action.admin.indices.mapping.put.PutMappingRequest;
import org.elasticsearch.action.admin.indices.mapping.put.PutMappingResponse;
import org.elasticsearch.action.admin.indices.open.OpenIndexRequest;
import org.elasticsearch.action.admin.indices.open.OpenIndexResponse;
import org.elasticsearch.action.admin.indices.refresh.RefreshRequest;
import org.elasticsearch.action.admin.indices.refresh.RefreshResponse;
import org.elasticsearch.action.admin.indices.rollover.RolloverRequest;
import org.elasticsearch.action.admin.indices.rollover.RolloverResponse;
import org.elasticsearch.action.admin.indices.settings.put.UpdateSettingsRequest;
import org.elasticsearch.action.admin.indices.settings.put.UpdateSettingsResponse;
import org.elasticsearch.action.admin.indices.shrink.ResizeRequest;
import org.elasticsearch.action.admin.indices.shrink.ResizeResponse;
import org.elasticsearch.action.admin.indices.shrink.ResizeType;
import org.elasticsearch.action.support.ActiveShardCount;
import org.elasticsearch.action.support.DefaultShardOperationFailedException;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.client.ESRestHighLevelClientTestCase;
import org.elasticsearch.client.RestHighLevelClient;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.rest.RestStatus;

import java.io.IOException;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * This class is used to generate the Java Indices API documentation.
 * You need to wrap your code between two tags like:
 * // tag::example
 * // end::example
 *
 * Where example is your tag name.
 *
 * Then in the documentation, you can extract what is between tag and end tags with
 * ["source","java",subs="attributes,callouts,macros"]
 * --------------------------------------------------
 * include-tagged::{doc-tests}/IndicesClientDocumentationIT.java[example]
 * --------------------------------------------------
 *
 * The column width of the code block is 84. If the code contains a line longer
 * than 84, the line will be cut and a horizontal scroll bar will be displayed.
 * (the code indentation of the tag is not included in the width)
 */
public class IndicesClientDocumentationIT extends ESRestHighLevelClientTestCase {

    public void testIndicesExist() throws IOException {
        RestHighLevelClient client = highLevelClient();

        {
            CreateIndexResponse createIndexResponse = client.indices().create(new CreateIndexRequest("twitter"));
            assertTrue(createIndexResponse.isAcknowledged());
        }

        {
            // tag::indices-exists-request
            GetIndexRequest request = new GetIndexRequest();
            request.indices("twitter"); // <1>
            // end::indices-exists-request

            IndicesOptions indicesOptions = IndicesOptions.strictExpand();
            // tag::indices-exists-request-optionals
            request.local(false); // <1>
            request.humanReadable(true); // <2>
            request.includeDefaults(false); // <3>
            request.indicesOptions(indicesOptions); // <4>
            // end::indices-exists-request-optionals

            // tag::indices-exists-response
            boolean exists = client.indices().exists(request);
            // end::indices-exists-response
            assertTrue(exists);
        }
    }

    public void testIndicesExistAsync() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            CreateIndexResponse createIndexResponse = client.indices().create(new CreateIndexRequest("twitter"));
            assertTrue(createIndexResponse.isAcknowledged());
        }

        {
            GetIndexRequest request = new GetIndexRequest();
            request.indices("twitter");

            // tag::indices-exists-execute-listener
            ActionListener<Boolean> listener = new ActionListener<Boolean>() {
                @Override
                public void onResponse(Boolean exists) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::indices-exists-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::indices-exists-async
            client.indices().existsAsync(request, listener); // <1>
            // end::indices-exists-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }
    public void testDeleteIndex() throws IOException {
        RestHighLevelClient client = highLevelClient();

        {
            CreateIndexResponse createIndexResponse = client.indices().create(new CreateIndexRequest("posts"));
            assertTrue(createIndexResponse.isAcknowledged());
        }

        {
            // tag::delete-index-request
            DeleteIndexRequest request = new DeleteIndexRequest("posts"); // <1>
            // end::delete-index-request

            // tag::delete-index-request-timeout
            request.timeout(TimeValue.timeValueMinutes(2)); // <1>
            request.timeout("2m"); // <2>
            // end::delete-index-request-timeout
            // tag::delete-index-request-masterTimeout
            request.masterNodeTimeout(TimeValue.timeValueMinutes(1)); // <1>
            request.masterNodeTimeout("1m"); // <2>
            // end::delete-index-request-masterTimeout
            // tag::delete-index-request-indicesOptions
            request.indicesOptions(IndicesOptions.lenientExpandOpen()); // <1>
            // end::delete-index-request-indicesOptions

            // tag::delete-index-execute
            DeleteIndexResponse deleteIndexResponse = client.indices().delete(request);
            // end::delete-index-execute

            // tag::delete-index-response
            boolean acknowledged = deleteIndexResponse.isAcknowledged(); // <1>
            // end::delete-index-response
            assertTrue(acknowledged);
        }

        {
            // tag::delete-index-notfound
            try {
                DeleteIndexRequest request = new DeleteIndexRequest("does_not_exist");
                client.indices().delete(request);
            } catch (ElasticsearchException exception) {
                if (exception.status() == RestStatus.NOT_FOUND) {
                    // <1>
                }
            }
            // end::delete-index-notfound
        }
    }

    public void testDeleteIndexAsync() throws Exception {
        final RestHighLevelClient client = highLevelClient();

        {
            CreateIndexResponse createIndexResponse = client.indices().create(new CreateIndexRequest("posts"));
            assertTrue(createIndexResponse.isAcknowledged());
        }

        {
            DeleteIndexRequest request = new DeleteIndexRequest("posts");

            // tag::delete-index-execute-listener
            ActionListener<DeleteIndexResponse> listener =
                    new ActionListener<DeleteIndexResponse>() {
                @Override
                public void onResponse(DeleteIndexResponse deleteIndexResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::delete-index-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::delete-index-execute-async
            client.indices().deleteAsync(request, listener); // <1>
            // end::delete-index-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testCreateIndex() throws IOException {
        RestHighLevelClient client = highLevelClient();

        {
            // tag::create-index-request
            CreateIndexRequest request = new CreateIndexRequest("twitter"); // <1>
            // end::create-index-request

            // tag::create-index-request-settings
            request.settings(Settings.builder() // <1>
                .put("index.number_of_shards", 3)
                .put("index.number_of_replicas", 2)
            );
            // end::create-index-request-settings

            {
                // tag::create-index-request-mappings
                request.mapping("tweet", // <1>
                        "{\n" +
                        "  \"tweet\": {\n" +
                        "    \"properties\": {\n" +
                        "      \"message\": {\n" +
                        "        \"type\": \"text\"\n" +
                        "      }\n" +
                        "    }\n" +
                        "  }\n" +
                        "}", // <2>
                        XContentType.JSON);
                // end::create-index-request-mappings
                CreateIndexResponse createIndexResponse = client.indices().create(request);
                assertTrue(createIndexResponse.isAcknowledged());
            }

            {
                request = new CreateIndexRequest("twitter2");
                //tag::create-index-mappings-map
                Map<String, Object> jsonMap = new HashMap<>();
                Map<String, Object> message = new HashMap<>();
                message.put("type", "text");
                Map<String, Object> properties = new HashMap<>();
                properties.put("message", message);
                Map<String, Object> tweet = new HashMap<>();
                tweet.put("properties", properties);
                jsonMap.put("tweet", tweet);
                request.mapping("tweet", jsonMap); // <1>
                //end::create-index-mappings-map
                CreateIndexResponse createIndexResponse = client.indices().create(request);
                assertTrue(createIndexResponse.isAcknowledged());
            }
            {
                request = new CreateIndexRequest("twitter3");
                //tag::create-index-mappings-xcontent
                XContentBuilder builder = XContentFactory.jsonBuilder();
                builder.startObject();
                {
                    builder.startObject("tweet");
                    {
                        builder.startObject("properties");
                        {
                            builder.startObject("message");
                            {
                                builder.field("type", "text");
                            }
                            builder.endObject();
                        }
                        builder.endObject();
                    }
                    builder.endObject();
                }
                builder.endObject();
                request.mapping("tweet", builder); // <1>
                //end::create-index-mappings-xcontent
                CreateIndexResponse createIndexResponse = client.indices().create(request);
                assertTrue(createIndexResponse.isAcknowledged());
            }
            {
                request = new CreateIndexRequest("twitter4");
                //tag::create-index-mappings-shortcut
                request.mapping("tweet", "message", "type=text"); // <1>
                //end::create-index-mappings-shortcut
                CreateIndexResponse createIndexResponse = client.indices().create(request);
                assertTrue(createIndexResponse.isAcknowledged());
            }

            request = new CreateIndexRequest("twitter5");
            // tag::create-index-request-aliases
            request.alias(new Alias("twitter_alias").filter(QueryBuilders.termQuery("user", "kimchy")));  // <1>
            // end::create-index-request-aliases

            // tag::create-index-request-timeout
            request.timeout(TimeValue.timeValueMinutes(2)); // <1>
            request.timeout("2m"); // <2>
            // end::create-index-request-timeout
            // tag::create-index-request-masterTimeout
            request.masterNodeTimeout(TimeValue.timeValueMinutes(1)); // <1>
            request.masterNodeTimeout("1m"); // <2>
            // end::create-index-request-masterTimeout
            // tag::create-index-request-waitForActiveShards
            request.waitForActiveShards(2); // <1>
            request.waitForActiveShards(ActiveShardCount.DEFAULT); // <2>
            // end::create-index-request-waitForActiveShards
            {
                CreateIndexResponse createIndexResponse = client.indices().create(request);
                assertTrue(createIndexResponse.isAcknowledged());
            }

            request = new CreateIndexRequest("twitter6");
            // tag::create-index-whole-source
            request.source("{\n" +
                    "    \"settings\" : {\n" +
                    "        \"number_of_shards\" : 1,\n" +
                    "        \"number_of_replicas\" : 0\n" +
                    "    },\n" +
                    "    \"mappings\" : {\n" +
                    "        \"tweet\" : {\n" +
                    "            \"properties\" : {\n" +
                    "                \"message\" : { \"type\" : \"text\" }\n" +
                    "            }\n" +
                    "        }\n" +
                    "    },\n" +
                    "    \"aliases\" : {\n" +
                    "        \"twitter_alias\" : {}\n" +
                    "    }\n" +
                    "}", XContentType.JSON); // <1>
            // end::create-index-whole-source

            // tag::create-index-execute
            CreateIndexResponse createIndexResponse = client.indices().create(request);
            // end::create-index-execute

            // tag::create-index-response
            boolean acknowledged = createIndexResponse.isAcknowledged(); // <1>
            boolean shardsAcknowledged = createIndexResponse.isShardsAcknowledged(); // <2>
            // end::create-index-response
            assertTrue(acknowledged);
            assertTrue(shardsAcknowledged);
        }
    }

    public void testCreateIndexAsync() throws Exception {
        final RestHighLevelClient client = highLevelClient();

        {
            CreateIndexRequest request = new CreateIndexRequest("twitter");

            // tag::create-index-execute-listener
            ActionListener<CreateIndexResponse> listener =
                    new ActionListener<CreateIndexResponse>() {

                @Override
                public void onResponse(CreateIndexResponse createIndexResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::create-index-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::create-index-execute-async
            client.indices().createAsync(request, listener); // <1>
            // end::create-index-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testPutMapping() throws IOException {
        RestHighLevelClient client = highLevelClient();

        {
            CreateIndexResponse createIndexResponse = client.indices().create(new CreateIndexRequest("twitter"));
            assertTrue(createIndexResponse.isAcknowledged());
        }

        {
            // tag::put-mapping-request
            PutMappingRequest request = new PutMappingRequest("twitter"); // <1>
            request.type("tweet"); // <2>
            // end::put-mapping-request

            {
                // tag::put-mapping-request-source
                request.source(
                    "{\n" +
                    "  \"properties\": {\n" +
                    "    \"message\": {\n" +
                    "      \"type\": \"text\"\n" +
                    "    }\n" +
                    "  }\n" +
                    "}", // <1>
                    XContentType.JSON);
                // end::put-mapping-request-source
                PutMappingResponse putMappingResponse = client.indices().putMapping(request);
                assertTrue(putMappingResponse.isAcknowledged());
            }

            {
                //tag::put-mapping-map
                Map<String, Object> jsonMap = new HashMap<>();
                Map<String, Object> message = new HashMap<>();
                message.put("type", "text");
                Map<String, Object> properties = new HashMap<>();
                properties.put("message", message);
                jsonMap.put("properties", properties);
                request.source(jsonMap); // <1>
                //end::put-mapping-map
                PutMappingResponse putMappingResponse = client.indices().putMapping(request);
                assertTrue(putMappingResponse.isAcknowledged());
            }
            {
                //tag::put-mapping-xcontent
                XContentBuilder builder = XContentFactory.jsonBuilder();
                builder.startObject();
                {
                    builder.startObject("properties");
                    {
                        builder.startObject("message");
                        {
                            builder.field("type", "text");
                        }
                        builder.endObject();
                    }
                    builder.endObject();
                }
                builder.endObject();
                request.source(builder); // <1>
                //end::put-mapping-xcontent
                PutMappingResponse putMappingResponse = client.indices().putMapping(request);
                assertTrue(putMappingResponse.isAcknowledged());
            }
            {
                //tag::put-mapping-shortcut
                request.source("message", "type=text"); // <1>
                //end::put-mapping-shortcut
                PutMappingResponse putMappingResponse = client.indices().putMapping(request);
                assertTrue(putMappingResponse.isAcknowledged());
            }

            // tag::put-mapping-request-timeout
            request.timeout(TimeValue.timeValueMinutes(2)); // <1>
            request.timeout("2m"); // <2>
            // end::put-mapping-request-timeout
            // tag::put-mapping-request-masterTimeout
            request.masterNodeTimeout(TimeValue.timeValueMinutes(1)); // <1>
            request.masterNodeTimeout("1m"); // <2>
            // end::put-mapping-request-masterTimeout

            // tag::put-mapping-execute
            PutMappingResponse putMappingResponse = client.indices().putMapping(request);
            // end::put-mapping-execute

            // tag::put-mapping-response
            boolean acknowledged = putMappingResponse.isAcknowledged(); // <1>
            // end::put-mapping-response
            assertTrue(acknowledged);
        }
    }

    public void testPutMappingAsync() throws Exception {
        final RestHighLevelClient client = highLevelClient();

        {
            CreateIndexResponse createIndexResponse = client.indices().create(new CreateIndexRequest("twitter"));
            assertTrue(createIndexResponse.isAcknowledged());
        }

        {
            PutMappingRequest request = new PutMappingRequest("twitter").type("tweet");

            // tag::put-mapping-execute-listener
            ActionListener<PutMappingResponse> listener =
                    new ActionListener<PutMappingResponse>() {
                @Override
                public void onResponse(PutMappingResponse putMappingResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::put-mapping-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::put-mapping-execute-async
            client.indices().putMappingAsync(request, listener); // <1>
            // end::put-mapping-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testOpenIndex() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            CreateIndexResponse createIndexResponse = client.indices().create(new CreateIndexRequest("index"));
            assertTrue(createIndexResponse.isAcknowledged());
        }

        {
            // tag::open-index-request
            OpenIndexRequest request = new OpenIndexRequest("index"); // <1>
            // end::open-index-request

            // tag::open-index-request-timeout
            request.timeout(TimeValue.timeValueMinutes(2)); // <1>
            request.timeout("2m"); // <2>
            // end::open-index-request-timeout
            // tag::open-index-request-masterTimeout
            request.masterNodeTimeout(TimeValue.timeValueMinutes(1)); // <1>
            request.masterNodeTimeout("1m"); // <2>
            // end::open-index-request-masterTimeout
            // tag::open-index-request-waitForActiveShards
            request.waitForActiveShards(2); // <1>
            request.waitForActiveShards(ActiveShardCount.DEFAULT); // <2>
            // end::open-index-request-waitForActiveShards

            // tag::open-index-request-indicesOptions
            request.indicesOptions(IndicesOptions.strictExpandOpen()); // <1>
            // end::open-index-request-indicesOptions

            // tag::open-index-execute
            OpenIndexResponse openIndexResponse = client.indices().open(request);
            // end::open-index-execute

            // tag::open-index-response
            boolean acknowledged = openIndexResponse.isAcknowledged(); // <1>
            boolean shardsAcked = openIndexResponse.isShardsAcknowledged(); // <2>
            // end::open-index-response
            assertTrue(acknowledged);
            assertTrue(shardsAcked);

            // tag::open-index-execute-listener
            ActionListener<OpenIndexResponse> listener =
                    new ActionListener<OpenIndexResponse>() {
                @Override
                public void onResponse(OpenIndexResponse openIndexResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::open-index-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::open-index-execute-async
            client.indices().openAsync(request, listener); // <1>
            // end::open-index-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }

        {
            // tag::open-index-notfound
            try {
                OpenIndexRequest request = new OpenIndexRequest("does_not_exist");
                client.indices().open(request);
            } catch (ElasticsearchException exception) {
                if (exception.status() == RestStatus.BAD_REQUEST) {
                    // <1>
                }
            }
            // end::open-index-notfound
        }
    }

    public void testRefreshIndex() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            createIndex("index1", Settings.EMPTY);
        }

        {
            // tag::refresh-request
            RefreshRequest request = new RefreshRequest("index1"); // <1>
            RefreshRequest requestMultiple = new RefreshRequest("index1", "index2"); // <2>
            RefreshRequest requestAll = new RefreshRequest(); // <3>
            // end::refresh-request

            // tag::refresh-request-indicesOptions
            request.indicesOptions(IndicesOptions.lenientExpandOpen()); // <1>
            // end::refresh-request-indicesOptions

            // tag::refresh-execute
            RefreshResponse refreshResponse = client.indices().refresh(request);
            // end::refresh-execute

            // tag::refresh-response
            int totalShards = refreshResponse.getTotalShards(); // <1>
            int successfulShards = refreshResponse.getSuccessfulShards(); // <2>
            int failedShards = refreshResponse.getFailedShards(); // <3>
            DefaultShardOperationFailedException[] failures = refreshResponse.getShardFailures(); // <4>
            // end::refresh-response

            // tag::refresh-execute-listener
            ActionListener<RefreshResponse> listener = new ActionListener<RefreshResponse>() {
                @Override
                public void onResponse(RefreshResponse refreshResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::refresh-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::refresh-execute-async
            client.indices().refreshAsync(request, listener); // <1>
            // end::refresh-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }

        {
            // tag::refresh-notfound
            try {
                RefreshRequest request = new RefreshRequest("does_not_exist");
                client.indices().refresh(request);
            } catch (ElasticsearchException exception) {
                if (exception.status() == RestStatus.NOT_FOUND) {
                    // <1>
                }
            }
            // end::refresh-notfound
        }
    }

    public void testFlushIndex() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            createIndex("index1", Settings.EMPTY);
        }

        {
            // tag::flush-request
            FlushRequest request = new FlushRequest("index1"); // <1>
            FlushRequest requestMultiple = new FlushRequest("index1", "index2"); // <2>
            FlushRequest requestAll = new FlushRequest(); // <3>
            // end::flush-request

            // tag::flush-request-indicesOptions
            request.indicesOptions(IndicesOptions.lenientExpandOpen()); // <1>
            // end::flush-request-indicesOptions

            // tag::flush-request-wait
            request.waitIfOngoing(true); // <1>
            // end::flush-request-wait

            // tag::flush-request-force
            request.force(true); // <1>
            // end::flush-request-force

            // tag::flush-execute
            FlushResponse flushResponse = client.indices().flush(request);
            // end::flush-execute

            // tag::flush-response
            int totalShards = flushResponse.getTotalShards(); // <1>
            int successfulShards = flushResponse.getSuccessfulShards(); // <2>
            int failedShards = flushResponse.getFailedShards(); // <3>
            DefaultShardOperationFailedException[] failures = flushResponse.getShardFailures(); // <4>
            // end::flush-response

            // tag::flush-execute-listener
            ActionListener<FlushResponse> listener = new ActionListener<FlushResponse>() {
                @Override
                public void onResponse(FlushResponse refreshResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::flush-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::flush-execute-async
            client.indices().flushAsync(request, listener); // <1>
            // end::flush-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }

        {
            // tag::flush-notfound
            try {
                FlushRequest request = new FlushRequest("does_not_exist");
                client.indices().flush(request);
            } catch (ElasticsearchException exception) {
                if (exception.status() == RestStatus.NOT_FOUND) {
                    // <1>
                }
            }
            // end::flush-notfound
        }
    }

    public void testForceMergeIndex() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            createIndex("index", Settings.EMPTY);
        }

        {
            // tag::force-merge-request
            ForceMergeRequest request = new ForceMergeRequest("index1"); // <1>
            ForceMergeRequest requestMultiple = new ForceMergeRequest("index1", "index2"); // <2>
            ForceMergeRequest requestAll = new ForceMergeRequest(); // <3>
            // end::force-merge-request

            // tag::force-merge-request-indicesOptions
            request.indicesOptions(IndicesOptions.lenientExpandOpen()); // <1>
            // end::force-merge-request-indicesOptions

            // tag::force-merge-request-segments-num
            request.maxNumSegments(1); // <1>
            // end::force-merge-request-segments-num

            // tag::force-merge-request-only-expunge-deletes
            request.onlyExpungeDeletes(true); // <1>
            // end::force-merge-request-only-expunge-deletes

            // tag::force-merge-request-flush
            request.flush(true); // <1>
            // end::force-merge-request-flush

            // tag::force-merge-execute
            ForceMergeResponse forceMergeResponse = client.indices().forceMerge(request);
            // end::force-merge-execute

            // tag::force-merge-response
            int totalShards = forceMergeResponse.getTotalShards(); // <1>
            int successfulShards = forceMergeResponse.getSuccessfulShards(); // <2>
            int failedShards = forceMergeResponse.getFailedShards(); // <3>
            DefaultShardOperationFailedException[] failures = forceMergeResponse.getShardFailures(); // <4>
            // end::force-merge-response

            // tag::force-merge-execute-listener
            ActionListener<ForceMergeResponse> listener = new ActionListener<ForceMergeResponse>() {
                @Override
                public void onResponse(ForceMergeResponse forceMergeResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::force-merge-execute-listener

            // tag::force-merge-execute-async
            client.indices().forceMergeAsync(request, listener); // <1>
            // end::force-merge-execute-async
        }
        {
            // tag::force-merge-notfound
            try {
                ForceMergeRequest request = new ForceMergeRequest("does_not_exist");
                client.indices().forceMerge(request);
            } catch (ElasticsearchException exception) {
                if (exception.status() == RestStatus.NOT_FOUND) {
                    // <1>
                }
            }
            // end::force-merge-notfound
        }
    }

    public void testClearCache() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            createIndex("index1", Settings.EMPTY);
        }

        {
            // tag::clear-cache-request
            ClearIndicesCacheRequest request = new ClearIndicesCacheRequest("index1"); // <1>
            ClearIndicesCacheRequest requestMultiple = new ClearIndicesCacheRequest("index1", "index2"); // <2>
            ClearIndicesCacheRequest requestAll = new ClearIndicesCacheRequest(); // <3>
            // end::clear-cache-request

            // tag::clear-cache-request-indicesOptions
            request.indicesOptions(IndicesOptions.lenientExpandOpen()); // <1>
            // end::clear-cache-request-indicesOptions

            // tag::clear-cache-request-query
            request.queryCache(true); // <1>
            // end::clear-cache-request-query

            // tag::clear-cache-request-request
            request.requestCache(true); // <1>
            // end::clear-cache-request-request

            // tag::clear-cache-request-fielddata
            request.fieldDataCache(true); // <1>
            // end::clear-cache-request-fielddata

            // tag::clear-cache-request-fields
            request.fields("field1", "field2", "field3"); // <1>
            // end::clear-cache-request-fields

            // tag::clear-cache-execute
            ClearIndicesCacheResponse clearCacheResponse = client.indices().clearCache(request);
            // end::clear-cache-execute

            // tag::clear-cache-response
            int totalShards = clearCacheResponse.getTotalShards(); // <1>
            int successfulShards = clearCacheResponse.getSuccessfulShards(); // <2>
            int failedShards = clearCacheResponse.getFailedShards(); // <3>
            DefaultShardOperationFailedException[] failures = clearCacheResponse.getShardFailures(); // <4>
            // end::clear-cache-response

            // tag::clear-cache-execute-listener
            ActionListener<ClearIndicesCacheResponse> listener = new ActionListener<ClearIndicesCacheResponse>() {
                @Override
                public void onResponse(ClearIndicesCacheResponse clearCacheResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::clear-cache-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::clear-cache-execute-async
            client.indices().clearCacheAsync(request, listener); // <1>
            // end::clear-cache-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }

        {
            // tag::clear-cache-notfound
            try {
                ClearIndicesCacheRequest request = new ClearIndicesCacheRequest("does_not_exist");
                client.indices().clearCache(request);
            } catch (ElasticsearchException exception) {
                if (exception.status() == RestStatus.NOT_FOUND) {
                    // <1>
                }
            }
            // end::clear-cache-notfound
        }
    }

    public void testCloseIndex() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            CreateIndexResponse createIndexResponse = client.indices().create(new CreateIndexRequest("index"));
            assertTrue(createIndexResponse.isAcknowledged());
        }

        {
            // tag::close-index-request
            CloseIndexRequest request = new CloseIndexRequest("index"); // <1>
            // end::close-index-request

            // tag::close-index-request-timeout
            request.timeout(TimeValue.timeValueMinutes(2)); // <1>
            request.timeout("2m"); // <2>
            // end::close-index-request-timeout
            // tag::close-index-request-masterTimeout
            request.masterNodeTimeout(TimeValue.timeValueMinutes(1)); // <1>
            request.masterNodeTimeout("1m"); // <2>
            // end::close-index-request-masterTimeout

            // tag::close-index-request-indicesOptions
            request.indicesOptions(IndicesOptions.lenientExpandOpen()); // <1>
            // end::close-index-request-indicesOptions

            // tag::close-index-execute
            CloseIndexResponse closeIndexResponse = client.indices().close(request);
            // end::close-index-execute

            // tag::close-index-response
            boolean acknowledged = closeIndexResponse.isAcknowledged(); // <1>
            // end::close-index-response
            assertTrue(acknowledged);

            // tag::close-index-execute-listener
            ActionListener<CloseIndexResponse> listener =
                    new ActionListener<CloseIndexResponse>() {
                @Override
                public void onResponse(CloseIndexResponse closeIndexResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::close-index-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::close-index-execute-async
            client.indices().closeAsync(request, listener); // <1>
            // end::close-index-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testExistsAlias() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            CreateIndexResponse createIndexResponse = client.indices().create(new CreateIndexRequest("index")
                    .alias(new Alias("alias")));
            assertTrue(createIndexResponse.isAcknowledged());
        }

        {
            // tag::exists-alias-request
            GetAliasesRequest request = new GetAliasesRequest();
            GetAliasesRequest requestWithAlias = new GetAliasesRequest("alias1");
            GetAliasesRequest requestWithAliases =
                    new GetAliasesRequest(new String[]{"alias1", "alias2"});
            // end::exists-alias-request

            // tag::exists-alias-request-alias
            request.aliases("alias"); // <1>
            // end::exists-alias-request-alias
            // tag::exists-alias-request-indices
            request.indices("index"); // <1>
            // end::exists-alias-request-indices

            // tag::exists-alias-request-indicesOptions
            request.indicesOptions(IndicesOptions.lenientExpandOpen()); // <1>
            // end::exists-alias-request-indicesOptions

            // tag::exists-alias-request-local
            request.local(true); // <1>
            // end::exists-alias-request-local

            // tag::exists-alias-execute
            boolean exists = client.indices().existsAlias(request);
            // end::exists-alias-execute
            assertTrue(exists);

            // tag::exists-alias-listener
            ActionListener<Boolean> listener = new ActionListener<Boolean>() {
                @Override
                public void onResponse(Boolean exists) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::exists-alias-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::exists-alias-execute-async
            client.indices().existsAliasAsync(request, listener); // <1>
            // end::exists-alias-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testUpdateAliases() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            CreateIndexResponse createIndexResponse = client.indices().create(new CreateIndexRequest("index1"));
            assertTrue(createIndexResponse.isAcknowledged());
            createIndexResponse = client.indices().create(new CreateIndexRequest("index2"));
            assertTrue(createIndexResponse.isAcknowledged());
            createIndexResponse = client.indices().create(new CreateIndexRequest("index3"));
            assertTrue(createIndexResponse.isAcknowledged());
            createIndexResponse = client.indices().create(new CreateIndexRequest("index4"));
            assertTrue(createIndexResponse.isAcknowledged());
        }

        {
            // tag::update-aliases-request
            IndicesAliasesRequest request = new IndicesAliasesRequest(); // <1>
            AliasActions aliasAction =
                    new AliasActions(AliasActions.Type.ADD)
                    .index("index1")
                    .alias("alias1"); // <2>
            request.addAliasAction(aliasAction); // <3>
            // end::update-aliases-request

            // tag::update-aliases-request2
            AliasActions addIndexAction =
                    new AliasActions(AliasActions.Type.ADD)
                    .index("index1")
                    .alias("alias1")
                    .filter("{\"term\":{\"year\":2016}}"); // <1>
            AliasActions addIndicesAction =
                    new AliasActions(AliasActions.Type.ADD)
                    .indices("index1", "index2")
                    .alias("alias2")
                    .routing("1"); // <2>
            AliasActions removeAction =
                    new AliasActions(AliasActions.Type.REMOVE)
                    .index("index3")
                    .alias("alias3"); // <3>
            AliasActions removeIndexAction =
                    new AliasActions(AliasActions.Type.REMOVE_INDEX)
                    .index("index4"); // <4>
            // end::update-aliases-request2

            // tag::update-aliases-request-timeout
            request.timeout(TimeValue.timeValueMinutes(2)); // <1>
            request.timeout("2m"); // <2>
            // end::update-aliases-request-timeout
            // tag::update-aliases-request-masterTimeout
            request.masterNodeTimeout(TimeValue.timeValueMinutes(1)); // <1>
            request.masterNodeTimeout("1m"); // <2>
            // end::update-aliases-request-masterTimeout

            // tag::update-aliases-execute
            IndicesAliasesResponse indicesAliasesResponse =
                    client.indices().updateAliases(request);
            // end::update-aliases-execute

            // tag::update-aliases-response
            boolean acknowledged = indicesAliasesResponse.isAcknowledged(); // <1>
            // end::update-aliases-response
            assertTrue(acknowledged);
        }

        {
            IndicesAliasesRequest request = new IndicesAliasesRequest();
            AliasActions aliasAction = new AliasActions(AliasActions.Type.ADD).index("index1").alias("async");
            request.addAliasAction(aliasAction);

            // tag::update-aliases-execute-listener
            ActionListener<IndicesAliasesResponse> listener =
                    new ActionListener<IndicesAliasesResponse>() {
                @Override
                public void onResponse(IndicesAliasesResponse indicesAliasesResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::update-aliases-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::update-aliases-execute-async
            client.indices().updateAliasesAsync(request, listener); // <1>
            // end::update-aliases-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testShrinkIndex() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            Map<String, Object> nodes = getAsMap("_nodes");
            @SuppressWarnings("unchecked")
            String firstNode = ((Map<String, Object>) nodes.get("nodes")).keySet().iterator().next();
            createIndex("source_index", Settings.builder().put("index.number_of_shards", 4).put("index.number_of_replicas", 0).build());
            updateIndexSettings("source_index", Settings.builder().put("index.routing.allocation.require._name", firstNode)
                    .put("index.blocks.write", true));
        }

        // tag::shrink-index-request
        ResizeRequest request = new ResizeRequest("target_index","source_index"); // <1>
        // end::shrink-index-request

        // tag::shrink-index-request-timeout
        request.timeout(TimeValue.timeValueMinutes(2)); // <1>
        request.timeout("2m"); // <2>
        // end::shrink-index-request-timeout
        // tag::shrink-index-request-masterTimeout
        request.masterNodeTimeout(TimeValue.timeValueMinutes(1)); // <1>
        request.masterNodeTimeout("1m"); // <2>
        // end::shrink-index-request-masterTimeout
        // tag::shrink-index-request-waitForActiveShards
        request.setWaitForActiveShards(2); // <1>
        request.setWaitForActiveShards(ActiveShardCount.DEFAULT); // <2>
        // end::shrink-index-request-waitForActiveShards
        // tag::shrink-index-request-settings
        request.getTargetIndexRequest().settings(Settings.builder()
                .put("index.number_of_shards", 2)); // <1>
        // end::shrink-index-request-settings
        // tag::shrink-index-request-aliases
        request.getTargetIndexRequest().alias(new Alias("target_alias")); // <1>
        // end::shrink-index-request-aliases

        // tag::shrink-index-execute
        ResizeResponse resizeResponse = client.indices().shrink(request);
        // end::shrink-index-execute

        // tag::shrink-index-response
        boolean acknowledged = resizeResponse.isAcknowledged(); // <1>
        boolean shardsAcked = resizeResponse.isShardsAcknowledged(); // <2>
        // end::shrink-index-response
        assertTrue(acknowledged);
        assertTrue(shardsAcked);

        // tag::shrink-index-execute-listener
        ActionListener<ResizeResponse> listener = new ActionListener<ResizeResponse>() {
            @Override
            public void onResponse(ResizeResponse resizeResponse) {
                // <1>
            }

            @Override
            public void onFailure(Exception e) {
                // <2>
            }
        };
        // end::shrink-index-execute-listener

        // Replace the empty listener by a blocking listener in test
        final CountDownLatch latch = new CountDownLatch(1);
        listener = new LatchedActionListener<>(listener, latch);

        // tag::shrink-index-execute-async
        client.indices().shrinkAsync(request, listener); // <1>
        // end::shrink-index-execute-async

        assertTrue(latch.await(30L, TimeUnit.SECONDS));
    }

    public void testSplitIndex() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            createIndex("source_index", Settings.builder().put("index.number_of_shards", 2).put("index.number_of_replicas", 0)
                    .put("index.number_of_routing_shards", 4).build());
            updateIndexSettings("source_index", Settings.builder().put("index.blocks.write", true));
        }

        // tag::split-index-request
        ResizeRequest request = new ResizeRequest("target_index","source_index"); // <1>
        request.setResizeType(ResizeType.SPLIT); // <2>
        // end::split-index-request

        // tag::split-index-request-timeout
        request.timeout(TimeValue.timeValueMinutes(2)); // <1>
        request.timeout("2m"); // <2>
        // end::split-index-request-timeout
        // tag::split-index-request-masterTimeout
        request.masterNodeTimeout(TimeValue.timeValueMinutes(1)); // <1>
        request.masterNodeTimeout("1m"); // <2>
        // end::split-index-request-masterTimeout
        // tag::split-index-request-waitForActiveShards
        request.setWaitForActiveShards(2); // <1>
        request.setWaitForActiveShards(ActiveShardCount.DEFAULT); // <2>
        // end::split-index-request-waitForActiveShards
        // tag::split-index-request-settings
        request.getTargetIndexRequest().settings(Settings.builder()
                .put("index.number_of_shards", 4)); // <1>
        // end::split-index-request-settings
        // tag::split-index-request-aliases
        request.getTargetIndexRequest().alias(new Alias("target_alias")); // <1>
        // end::split-index-request-aliases

        // tag::split-index-execute
        ResizeResponse resizeResponse = client.indices().split(request);
        // end::split-index-execute

        // tag::split-index-response
        boolean acknowledged = resizeResponse.isAcknowledged(); // <1>
        boolean shardsAcked = resizeResponse.isShardsAcknowledged(); // <2>
        // end::split-index-response
        assertTrue(acknowledged);
        assertTrue(shardsAcked);

        // tag::split-index-execute-listener
        ActionListener<ResizeResponse> listener = new ActionListener<ResizeResponse>() {
            @Override
            public void onResponse(ResizeResponse resizeResponse) {
                // <1>
            }

            @Override
            public void onFailure(Exception e) {
                // <2>
            }
        };
        // end::split-index-execute-listener

        // Replace the empty listener by a blocking listener in test
        final CountDownLatch latch = new CountDownLatch(1);
        listener = new LatchedActionListener<>(listener, latch);

        // tag::split-index-execute-async
        client.indices().splitAsync(request,listener); // <1>
        // end::split-index-execute-async

        assertTrue(latch.await(30L, TimeUnit.SECONDS));
    }

    public void testRolloverIndex() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            client.indices().create(new CreateIndexRequest("index-1").alias(new Alias("alias")));
        }

        // tag::rollover-request
        RolloverRequest request = new RolloverRequest("alias", "index-2"); // <1>
        request.addMaxIndexAgeCondition(new TimeValue(7, TimeUnit.DAYS)); // <2>
        request.addMaxIndexDocsCondition(1000); // <3>
        request.addMaxIndexSizeCondition(new ByteSizeValue(5, ByteSizeUnit.GB)); // <4>
        // end::rollover-request

        // tag::rollover-request-timeout
        request.timeout(TimeValue.timeValueMinutes(2)); // <1>
        request.timeout("2m"); // <2>
        // end::rollover-request-timeout
        // tag::rollover-request-masterTimeout
        request.masterNodeTimeout(TimeValue.timeValueMinutes(1)); // <1>
        request.masterNodeTimeout("1m"); // <2>
        // end::rollover-request-masterTimeout
        // tag::rollover-request-dryRun
        request.dryRun(true); // <1>
        // end::rollover-request-dryRun
        // tag::rollover-request-waitForActiveShards
        request.getCreateIndexRequest().waitForActiveShards(2); // <1>
        request.getCreateIndexRequest().waitForActiveShards(ActiveShardCount.DEFAULT); // <2>
        // end::rollover-request-waitForActiveShards
        // tag::rollover-request-settings
        request.getCreateIndexRequest().settings(Settings.builder()
                .put("index.number_of_shards", 4)); // <1>
        // end::rollover-request-settings
        // tag::rollover-request-mapping
        request.getCreateIndexRequest().mapping("type", "field", "type=keyword"); // <1>
        // end::rollover-request-mapping
        // tag::rollover-request-alias
        request.getCreateIndexRequest().alias(new Alias("another_alias")); // <1>
        // end::rollover-request-alias

        // tag::rollover-execute
        RolloverResponse rolloverResponse = client.indices().rollover(request);
        // end::rollover-execute

        // tag::rollover-response
        boolean acknowledged = rolloverResponse.isAcknowledged(); // <1>
        boolean shardsAcked = rolloverResponse.isShardsAcknowledged(); // <2>
        String oldIndex = rolloverResponse.getOldIndex(); // <3>
        String newIndex = rolloverResponse.getNewIndex(); // <4>
        boolean isRolledOver = rolloverResponse.isRolledOver(); // <5>
        boolean isDryRun = rolloverResponse.isDryRun(); // <6>
        Map<String, Boolean> conditionStatus = rolloverResponse.getConditionStatus();// <7>
        // end::rollover-response
        assertFalse(acknowledged);
        assertFalse(shardsAcked);
        assertEquals("index-1", oldIndex);
        assertEquals("index-2", newIndex);
        assertFalse(isRolledOver);
        assertTrue(isDryRun);
        assertEquals(3, conditionStatus.size());

        // tag::rollover-execute-listener
        ActionListener<RolloverResponse> listener = new ActionListener<RolloverResponse>() {
            @Override
            public void onResponse(RolloverResponse rolloverResponse) {
                // <1>
            }

            @Override
            public void onFailure(Exception e) {
                // <2>
            }
        };
        // end::rollover-execute-listener

        // Replace the empty listener by a blocking listener in test
        final CountDownLatch latch = new CountDownLatch(1);
        listener = new LatchedActionListener<>(listener, latch);

        // tag::rollover-execute-async
        client.indices().rolloverAsync(request,listener); // <1>
        // end::rollover-execute-async

        assertTrue(latch.await(30L, TimeUnit.SECONDS));
    }

    public void testIndexPutSettings() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            CreateIndexResponse createIndexResponse = client.indices().create(new CreateIndexRequest("index"));
            assertTrue(createIndexResponse.isAcknowledged());
        }

        // tag::put-settings-request
        UpdateSettingsRequest request = new UpdateSettingsRequest("index1"); // <1>
        UpdateSettingsRequest requestMultiple =
                new UpdateSettingsRequest("index1", "index2"); // <2>
        UpdateSettingsRequest requestAll = new UpdateSettingsRequest(); // <3>
        // end::put-settings-request

        // tag::put-settings-create-settings
        String settingKey = "index.number_of_replicas";
        int settingValue = 0;
        Settings settings =
                Settings.builder()
                .put(settingKey, settingValue)
                .build(); // <1>
        // end::put-settings-create-settings
        // tag::put-settings-request-index-settings
        request.settings(settings);
        // end::put-settings-request-index-settings

        {
            // tag::put-settings-settings-builder
            Settings.Builder settingsBuilder =
                    Settings.builder()
                    .put(settingKey, settingValue);
            request.settings(settingsBuilder); // <1>
            // end::put-settings-settings-builder
        }
        {
            // tag::put-settings-settings-map
            Map<String, Object> map = new HashMap<>();
            map.put(settingKey, settingValue);
            request.settings(map); // <1>
            // end::put-settings-settings-map
        }
        {
            // tag::put-settings-settings-source
            request.settings(
                    "{\"index.number_of_replicas\": \"2\"}"
                    , XContentType.JSON); // <1>
            // end::put-settings-settings-source
        }

        // tag::put-settings-request-preserveExisting
        request.setPreserveExisting(false); // <1>
        // end::put-settings-request-preserveExisting
        // tag::put-settings-request-timeout
        request.timeout(TimeValue.timeValueMinutes(2)); // <1>
        request.timeout("2m"); // <2>
        // end::put-settings-request-timeout
        // tag::put-settings-request-masterTimeout
        request.masterNodeTimeout(TimeValue.timeValueMinutes(1)); // <1>
        request.masterNodeTimeout("1m"); // <2>
        // end::put-settings-request-masterTimeout
        // tag::put-settings-request-indicesOptions
        request.indicesOptions(IndicesOptions.lenientExpandOpen()); // <1>
        // end::put-settings-request-indicesOptions

        // tag::put-settings-execute
        UpdateSettingsResponse updateSettingsResponse =
                client.indices().putSettings(request);
        // end::put-settings-execute

        // tag::put-settings-response
        boolean acknowledged = updateSettingsResponse.isAcknowledged(); // <1>
        // end::put-settings-response
        assertTrue(acknowledged);

        // tag::put-settings-execute-listener
        ActionListener<UpdateSettingsResponse> listener =
                new ActionListener<UpdateSettingsResponse>() {

            @Override
            public void onResponse(UpdateSettingsResponse updateSettingsResponse) {
                // <1>
            }

            @Override
            public void onFailure(Exception e) {
                // <2>
            }
        };
        // end::put-settings-execute-listener

        // Replace the empty listener by a blocking listener in test
        final CountDownLatch latch = new CountDownLatch(1);
        listener = new LatchedActionListener<>(listener, latch);

        // tag::put-settings-execute-async
        client.indices().putSettingsAsync(request,listener); // <1>
        // end::put-settings-execute-async

        assertTrue(latch.await(30L, TimeUnit.SECONDS));
    }

}
