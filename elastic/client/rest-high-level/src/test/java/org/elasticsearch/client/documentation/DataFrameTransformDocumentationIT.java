/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.client.documentation;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.LatchedActionListener;
import org.elasticsearch.client.ESRestHighLevelClientTestCase;
import org.elasticsearch.client.RequestOptions;
import org.elasticsearch.client.RestHighLevelClient;
import org.elasticsearch.client.core.AcknowledgedResponse;
import org.elasticsearch.client.core.IndexerState;
import org.elasticsearch.client.dataframe.DeleteDataFrameTransformRequest;
import org.elasticsearch.client.dataframe.GetDataFrameTransformRequest;
import org.elasticsearch.client.dataframe.GetDataFrameTransformResponse;
import org.elasticsearch.client.dataframe.GetDataFrameTransformStatsRequest;
import org.elasticsearch.client.dataframe.GetDataFrameTransformStatsResponse;
import org.elasticsearch.client.dataframe.PreviewDataFrameTransformRequest;
import org.elasticsearch.client.dataframe.PreviewDataFrameTransformResponse;
import org.elasticsearch.client.dataframe.PutDataFrameTransformRequest;
import org.elasticsearch.client.dataframe.StartDataFrameTransformRequest;
import org.elasticsearch.client.dataframe.StartDataFrameTransformResponse;
import org.elasticsearch.client.dataframe.StopDataFrameTransformRequest;
import org.elasticsearch.client.dataframe.StopDataFrameTransformResponse;
import org.elasticsearch.client.dataframe.transforms.DataFrameIndexerTransformStats;
import org.elasticsearch.client.dataframe.transforms.DataFrameTransformConfig;
import org.elasticsearch.client.dataframe.transforms.DataFrameTransformProgress;
import org.elasticsearch.client.dataframe.transforms.DataFrameTransformStateAndStats;
import org.elasticsearch.client.dataframe.transforms.DataFrameTransformTaskState;
import org.elasticsearch.client.dataframe.transforms.DestConfig;
import org.elasticsearch.client.dataframe.transforms.QueryConfig;
import org.elasticsearch.client.dataframe.transforms.SourceConfig;
import org.elasticsearch.client.dataframe.transforms.pivot.AggregationConfig;
import org.elasticsearch.client.dataframe.transforms.pivot.GroupConfig;
import org.elasticsearch.client.dataframe.transforms.pivot.PivotConfig;
import org.elasticsearch.client.dataframe.transforms.pivot.TermsGroupSource;
import org.elasticsearch.client.indices.CreateIndexRequest;
import org.elasticsearch.client.indices.CreateIndexResponse;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.index.query.MatchAllQueryBuilder;
import org.elasticsearch.search.aggregations.AggregationBuilders;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.junit.After;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.hamcrest.Matchers.hasSize;

public class DataFrameTransformDocumentationIT extends ESRestHighLevelClientTestCase {

    private List<String> transformsToClean = new ArrayList<>();

    @After
    public void cleanUpTransforms() throws IOException {
        for (String transformId : transformsToClean) {
            highLevelClient().dataFrame().stopDataFrameTransform(new StopDataFrameTransformRequest(transformId), RequestOptions.DEFAULT);
        }

        for (String transformId : transformsToClean) {
            highLevelClient().dataFrame().deleteDataFrameTransform(
                    new DeleteDataFrameTransformRequest(transformId), RequestOptions.DEFAULT);
        }

        transformsToClean = new ArrayList<>();
    }

    private void createIndex(String indexName) throws IOException {

        XContentBuilder builder = jsonBuilder();
        builder.startObject()
                .startObject("properties")
                .startObject("timestamp")
                .field("type", "date")
                .endObject()
                .startObject("user_id")
                .field("type", "keyword")
                .endObject()
                .startObject("stars")
                .field("type", "integer")
                .endObject()
                .endObject()
                .endObject();

        CreateIndexRequest request = new CreateIndexRequest(indexName);
        request.mapping(builder);
        CreateIndexResponse response = highLevelClient().indices().create(request, RequestOptions.DEFAULT);
        assertTrue(response.isAcknowledged());
    }

    public void testPutDataFrameTransform() throws IOException, InterruptedException {
        createIndex("source-index");

        RestHighLevelClient client = highLevelClient();

        // tag::put-data-frame-transform-query-config
        QueryConfig queryConfig = new QueryConfig(new MatchAllQueryBuilder());
        // end::put-data-frame-transform-query-config
        // tag::put-data-frame-transform-source-config
        SourceConfig sourceConfig =
            new SourceConfig(new String[]{"source-index"}, queryConfig);
        // end::put-data-frame-transform-source-config
        // tag::put-data-frame-transform-group-config
        GroupConfig groupConfig =
                new GroupConfig(Collections.singletonMap("reviewer", // <1>
                        new TermsGroupSource("user_id")));           // <2>
        // end::put-data-frame-transform-group-config
        // tag::put-data-frame-transform-agg-config
        AggregatorFactories.Builder aggBuilder = new AggregatorFactories.Builder();
        aggBuilder.addAggregator(
                AggregationBuilders.avg("avg_rating").field("stars"));  // <1>
        AggregationConfig aggConfig = new AggregationConfig(aggBuilder);
        // end::put-data-frame-transform-agg-config
        // tag::put-data-frame-transform-pivot-config
        PivotConfig pivotConfig = new PivotConfig(groupConfig, aggConfig);
        // end::put-data-frame-transform-pivot-config
        // tag::put-data-frame-transform-config
        DataFrameTransformConfig transformConfig =
                new DataFrameTransformConfig("reviewer-avg-rating", // <1>
                sourceConfig, // <2>
                new DestConfig("pivot-destination"),  // <3>
                pivotConfig);  // <4>
        // end::put-data-frame-transform-config

        {
            // tag::put-data-frame-transform-request
            PutDataFrameTransformRequest request =
                    new PutDataFrameTransformRequest(transformConfig); // <1>
            // end::put-data-frame-transform-request

            // tag::put-data-frame-transform-execute
            AcknowledgedResponse response =
                    client.dataFrame().putDataFrameTransform(
                            request, RequestOptions.DEFAULT);
            // end::put-data-frame-transform-execute

            assertTrue(response.isAcknowledged());
        }
        {
            DataFrameTransformConfig configWithDifferentId = new DataFrameTransformConfig("reviewer-avg-rating2",
                    transformConfig.getSource(), transformConfig.getDestination(),
                    transformConfig.getPivotConfig());
            PutDataFrameTransformRequest request = new PutDataFrameTransformRequest(configWithDifferentId);

            // tag::put-data-frame-transform-execute-listener
            ActionListener<AcknowledgedResponse> listener =
                    new ActionListener<AcknowledgedResponse>() {
                @Override
                public void onResponse(AcknowledgedResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::put-data-frame-transform-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::put-data-frame-transform-execute-async
            client.dataFrame().putDataFrameTransformAsync(
                    request, RequestOptions.DEFAULT, listener); // <1>
            // end::put-data-frame-transform-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testStartStop() throws IOException, InterruptedException {
        createIndex("source-data");

        RestHighLevelClient client = highLevelClient();

        QueryConfig queryConfig = new QueryConfig(new MatchAllQueryBuilder());
        GroupConfig groupConfig = new GroupConfig(Collections.singletonMap("reviewer", new TermsGroupSource("user_id")));
        AggregatorFactories.Builder aggBuilder = new AggregatorFactories.Builder();
        aggBuilder.addAggregator(AggregationBuilders.avg("avg_rating").field("stars"));
        AggregationConfig aggConfig = new AggregationConfig(aggBuilder);
        PivotConfig pivotConfig = new PivotConfig(groupConfig, aggConfig);

        DataFrameTransformConfig transformConfig = new DataFrameTransformConfig("mega-transform",
                new SourceConfig(new String[]{"source-data"}, queryConfig), new DestConfig("pivot-dest"), pivotConfig);

        client.dataFrame().putDataFrameTransform(new PutDataFrameTransformRequest(transformConfig), RequestOptions.DEFAULT);
        transformsToClean.add(transformConfig.getId());

        {
            // tag::start-data-frame-transform-request
            StartDataFrameTransformRequest request =
                    new StartDataFrameTransformRequest("mega-transform");  // <1>
            // end::start-data-frame-transform-request

            // tag::start-data-frame-transform-request-options
            request.setTimeout(TimeValue.timeValueSeconds(20));  // <1>
            // end::start-data-frame-transform-request-options

            // tag::start-data-frame-transform-execute
            StartDataFrameTransformResponse response =
                    client.dataFrame().startDataFrameTransform(
                            request, RequestOptions.DEFAULT);
            // end::start-data-frame-transform-execute

            assertTrue(response.isStarted());
        }
        {
            // tag::stop-data-frame-transform-request
            StopDataFrameTransformRequest request =
                    new StopDataFrameTransformRequest("mega-transform"); // <1>
            // end::stop-data-frame-transform-request

            // tag::stop-data-frame-transform-request-options
            request.setWaitForCompletion(Boolean.TRUE);  // <1>
            request.setTimeout(TimeValue.timeValueSeconds(30));  // <2>
            // end::stop-data-frame-transform-request-options

            // tag::stop-data-frame-transform-execute
            StopDataFrameTransformResponse response =
                    client.dataFrame().stopDataFrameTransform(
                            request, RequestOptions.DEFAULT);
            // end::stop-data-frame-transform-execute

            assertTrue(response.isStopped());
        }
        {
            // tag::start-data-frame-transform-execute-listener
            ActionListener<StartDataFrameTransformResponse> listener =
                    new ActionListener<StartDataFrameTransformResponse>() {
                        @Override
                        public void onResponse(
                                StartDataFrameTransformResponse response) {
                            // <1>
                        }

                        @Override
                        public void onFailure(Exception e) {
                            // <2>
                        }
                    };
            // end::start-data-frame-transform-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            StartDataFrameTransformRequest request = new StartDataFrameTransformRequest("mega-transform");
            // tag::start-data-frame-transform-execute-async
            client.dataFrame().startDataFrameTransformAsync(
                    request, RequestOptions.DEFAULT, listener); // <1>
            // end::start-data-frame-transform-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
        {
            // tag::stop-data-frame-transform-execute-listener
            ActionListener<StopDataFrameTransformResponse> listener =
                    new ActionListener<StopDataFrameTransformResponse>() {
                        @Override
                        public void onResponse(
                                StopDataFrameTransformResponse response) {
                            // <1>
                        }

                        @Override
                        public void onFailure(Exception e) {
                            // <2>
                        }
                    };
            // end::stop-data-frame-transform-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            StopDataFrameTransformRequest request = new StopDataFrameTransformRequest("mega-transform");
            // tag::stop-data-frame-transform-execute-async
            client.dataFrame().stopDataFrameTransformAsync(
                    request, RequestOptions.DEFAULT, listener); // <1>
            // end::stop-data-frame-transform-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testDeleteDataFrameTransform() throws IOException, InterruptedException {
        createIndex("source-data");

        RestHighLevelClient client = highLevelClient();

        QueryConfig queryConfig = new QueryConfig(new MatchAllQueryBuilder());
        GroupConfig groupConfig = new GroupConfig(Collections.singletonMap("reviewer", new TermsGroupSource("user_id")));
        AggregatorFactories.Builder aggBuilder = new AggregatorFactories.Builder();
        aggBuilder.addAggregator(AggregationBuilders.avg("avg_rating").field("stars"));
        AggregationConfig aggConfig = new AggregationConfig(aggBuilder);
        PivotConfig pivotConfig = new PivotConfig(groupConfig, aggConfig);

        DataFrameTransformConfig transformConfig1 = new DataFrameTransformConfig("mega-transform",
                new SourceConfig(new String[]{"source-data"}, queryConfig), new DestConfig("pivot-dest"), pivotConfig);
        DataFrameTransformConfig transformConfig2 = new DataFrameTransformConfig("mega-transform2",
                new SourceConfig(new String[]{"source-data"}, queryConfig), new DestConfig("pivot-dest2"), pivotConfig);

        client.dataFrame().putDataFrameTransform(new PutDataFrameTransformRequest(transformConfig1), RequestOptions.DEFAULT);
        client.dataFrame().putDataFrameTransform(new PutDataFrameTransformRequest(transformConfig2), RequestOptions.DEFAULT);

        {
            // tag::delete-data-frame-transform-request
            DeleteDataFrameTransformRequest request =
                    new DeleteDataFrameTransformRequest("mega-transform"); // <1>
            // end::delete-data-frame-transform-request

            // tag::delete-data-frame-transform-execute
            AcknowledgedResponse response =
                    client.dataFrame()
                    .deleteDataFrameTransform(request, RequestOptions.DEFAULT);
            // end::delete-data-frame-transform-execute

            assertTrue(response.isAcknowledged());
        }
        {
            // tag::delete-data-frame-transform-execute-listener
            ActionListener<AcknowledgedResponse> listener =
                    new ActionListener<AcknowledgedResponse>() {
                @Override
                public void onResponse(AcknowledgedResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::delete-data-frame-transform-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            DeleteDataFrameTransformRequest request = new DeleteDataFrameTransformRequest("mega-transform2");

            // tag::delete-data-frame-transform-execute-async
            client.dataFrame().deleteDataFrameTransformAsync(
                    request, RequestOptions.DEFAULT, listener);  // <1>
            // end::delete-data-frame-transform-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testPreview() throws IOException, InterruptedException {
        createIndex("source-data");

        RestHighLevelClient client = highLevelClient();

        QueryConfig queryConfig = new QueryConfig(new MatchAllQueryBuilder());
        GroupConfig groupConfig = new GroupConfig(Collections.singletonMap("reviewer", new TermsGroupSource("user_id")));
        AggregatorFactories.Builder aggBuilder = new AggregatorFactories.Builder();
        aggBuilder.addAggregator(AggregationBuilders.avg("avg_rating").field("stars"));
        AggregationConfig aggConfig = new AggregationConfig(aggBuilder);
        PivotConfig pivotConfig = new PivotConfig(groupConfig, aggConfig);

        // tag::preview-data-frame-transform-request
        DataFrameTransformConfig transformConfig =
                new DataFrameTransformConfig(null,  // <1>
                new SourceConfig(new String[]{"source-data"}, queryConfig),
                null,                               // <2>
                pivotConfig);

        PreviewDataFrameTransformRequest request =
                new PreviewDataFrameTransformRequest(transformConfig); // <3>
        // end::preview-data-frame-transform-request

        {
            // tag::preview-data-frame-transform-execute
            PreviewDataFrameTransformResponse response =
                client.dataFrame()
                    .previewDataFrameTransform(request, RequestOptions.DEFAULT);
            // end::preview-data-frame-transform-execute

            assertNotNull(response.getDocs());
        }
        {
            // tag::preview-data-frame-transform-execute-listener
            ActionListener<PreviewDataFrameTransformResponse> listener =
                new ActionListener<PreviewDataFrameTransformResponse>() {
                    @Override
                    public void onResponse(PreviewDataFrameTransformResponse response) {
                        // <1>
                    }

                    @Override
                    public void onFailure(Exception e) {
                        // <2>
                    }
                };
            // end::preview-data-frame-transform-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::preview-data-frame-transform-execute-async
            client.dataFrame().previewDataFrameTransformAsync(
                    request, RequestOptions.DEFAULT, listener);  // <1>
            // end::preview-data-frame-transform-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testGetStats() throws IOException, InterruptedException {
        createIndex("source-data");

        RestHighLevelClient client = highLevelClient();

        QueryConfig queryConfig = new QueryConfig(new MatchAllQueryBuilder());
        GroupConfig groupConfig = new GroupConfig(Collections.singletonMap("reviewer", new TermsGroupSource("user_id")));
        AggregatorFactories.Builder aggBuilder = new AggregatorFactories.Builder();
        aggBuilder.addAggregator(AggregationBuilders.avg("avg_rating").field("stars"));
        AggregationConfig aggConfig = new AggregationConfig(aggBuilder);
        PivotConfig pivotConfig = new PivotConfig(groupConfig, aggConfig);

        String id = "statisitcal-transform";
        DataFrameTransformConfig transformConfig = new DataFrameTransformConfig(id,
                new SourceConfig(new String[]{"source-data"}, queryConfig), new DestConfig("dest"), pivotConfig);
        client.dataFrame().putDataFrameTransform(new PutDataFrameTransformRequest(transformConfig), RequestOptions.DEFAULT);

        // tag::get-data-frame-transform-stats-request
        GetDataFrameTransformStatsRequest request =
                new GetDataFrameTransformStatsRequest(id); // <1>
        // end::get-data-frame-transform-stats-request

        {
            // tag::get-data-frame-transform-stats-execute
            GetDataFrameTransformStatsResponse response =
                client.dataFrame()
                    .getDataFrameTransformStats(request, RequestOptions.DEFAULT);
            // end::get-data-frame-transform-stats-execute

            assertThat(response.getTransformsStateAndStats(), hasSize(1));

            // tag::get-data-frame-transform-stats-response
            DataFrameTransformStateAndStats stateAndStats =
                response.getTransformsStateAndStats().get(0);   // <1>
            DataFrameTransformTaskState taskState =
                stateAndStats.getTransformState().getTaskState(); // <2>
            IndexerState indexerState =
                stateAndStats.getTransformState().getIndexerState();  // <3>
            DataFrameIndexerTransformStats transformStats =
                stateAndStats.getTransformStats();              // <4>
            DataFrameTransformProgress progress =
                stateAndStats.getTransformState().getProgress(); // <5>
            // end::get-data-frame-transform-stats-response

            assertEquals(IndexerState.STOPPED, indexerState);
            assertEquals(DataFrameTransformTaskState.STOPPED, taskState);
            assertNotNull(transformStats);
            assertNull(progress);
        }
        {
            // tag::get-data-frame-transform-stats-execute-listener
            ActionListener<GetDataFrameTransformStatsResponse> listener =
                    new ActionListener<GetDataFrameTransformStatsResponse>() {
                        @Override
                        public void onResponse(
                                GetDataFrameTransformStatsResponse response) {
                            // <1>
                        }

                        @Override
                        public void onFailure(Exception e) {
                            // <2>
                        }
                    };
            // end::get-data-frame-transform-stats-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::get-data-frame-transform-stats-execute-async
            client.dataFrame().getDataFrameTransformStatsAsync(
                    request, RequestOptions.DEFAULT, listener);  // <1>
            // end::get-data-frame-transform-stats-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }


    public void testGetDataFrameTransform() throws IOException, InterruptedException {
        createIndex("source-data");
        
        QueryConfig queryConfig = new QueryConfig(new MatchAllQueryBuilder());
        GroupConfig groupConfig = new GroupConfig(Collections.singletonMap("reviewer", new TermsGroupSource("user_id")));
        AggregatorFactories.Builder aggBuilder = new AggregatorFactories.Builder();
        aggBuilder.addAggregator(AggregationBuilders.avg("avg_rating").field("stars"));
        AggregationConfig aggConfig = new AggregationConfig(aggBuilder);
        PivotConfig pivotConfig = new PivotConfig(groupConfig, aggConfig);

        
        DataFrameTransformConfig putTransformConfig = new DataFrameTransformConfig("mega-transform",
                new SourceConfig(new String[]{"source-data"}, queryConfig),
                new DestConfig("pivot-dest"), pivotConfig);

        RestHighLevelClient client = highLevelClient();
        client.dataFrame().putDataFrameTransform(new PutDataFrameTransformRequest(putTransformConfig), RequestOptions.DEFAULT);
        transformsToClean.add(putTransformConfig.getId());

        {
            // tag::get-data-frame-transform-request
            GetDataFrameTransformRequest request =
                    new GetDataFrameTransformRequest("mega-transform"); // <1>
            // end::get-data-frame-transform-request

            // tag::get-data-frame-transform-request-options
            request.setFrom(0);     // <1>
            request.setSize(100);   // <2>
            // end::get-data-frame-transform-request-options

            // tag::get-data-frame-transform-execute
            GetDataFrameTransformResponse response =
                client.dataFrame()
                    .getDataFrameTransform(request, RequestOptions.DEFAULT);
            // end::get-data-frame-transform-execute

            // tag::get-data-frame-transform-response
            List<DataFrameTransformConfig> transformConfigs =
                    response.getTransformConfigurations();
            // end::get-data-frame-transform-response

            assertEquals(1, transformConfigs.size());
        }
        {
            // tag::get-data-frame-transform-execute-listener
            ActionListener<GetDataFrameTransformResponse> listener =
                new ActionListener<GetDataFrameTransformResponse>() {
                    @Override
                    public void onResponse(GetDataFrameTransformResponse response) {
                        // <1>
                    }

                    @Override
                    public void onFailure(Exception e) {
                        // <2>
                    }
                };
            // end::get-data-frame-transform-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            GetDataFrameTransformRequest request = new GetDataFrameTransformRequest("mega-transform");

            // tag::get-data-frame-transform-execute-async
            client.dataFrame().getDataFrameTransformAsync(
                    request, RequestOptions.DEFAULT, listener);  // <1>
            // end::get-data-frame-transform-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }
}
