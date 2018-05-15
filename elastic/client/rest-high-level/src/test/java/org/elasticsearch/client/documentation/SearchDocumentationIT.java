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

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.LatchedActionListener;
import org.elasticsearch.action.admin.indices.create.CreateIndexRequest;
import org.elasticsearch.action.admin.indices.create.CreateIndexResponse;
import org.elasticsearch.action.bulk.BulkRequest;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.action.fieldcaps.FieldCapabilities;
import org.elasticsearch.action.fieldcaps.FieldCapabilitiesRequest;
import org.elasticsearch.action.fieldcaps.FieldCapabilitiesResponse;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.index.IndexResponse;
import org.elasticsearch.action.search.ClearScrollRequest;
import org.elasticsearch.action.search.ClearScrollResponse;
import org.elasticsearch.action.search.MultiSearchRequest;
import org.elasticsearch.action.search.MultiSearchResponse;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.search.SearchScrollRequest;
import org.elasticsearch.action.search.ShardSearchFailure;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.client.ESRestHighLevelClientTestCase;
import org.elasticsearch.client.RestHighLevelClient;
import org.elasticsearch.common.text.Text;
import org.elasticsearch.common.unit.Fuzziness;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.query.MatchQueryBuilder;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.index.rankeval.EvalQueryQuality;
import org.elasticsearch.index.rankeval.EvaluationMetric;
import org.elasticsearch.index.rankeval.MetricDetail;
import org.elasticsearch.index.rankeval.PrecisionAtK;
import org.elasticsearch.index.rankeval.RankEvalRequest;
import org.elasticsearch.index.rankeval.RankEvalResponse;
import org.elasticsearch.index.rankeval.RankEvalSpec;
import org.elasticsearch.index.rankeval.RatedDocument;
import org.elasticsearch.index.rankeval.RatedRequest;
import org.elasticsearch.index.rankeval.RatedSearchHit;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.search.Scroll;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.SearchHits;
import org.elasticsearch.search.aggregations.Aggregation;
import org.elasticsearch.search.aggregations.AggregationBuilders;
import org.elasticsearch.search.aggregations.Aggregations;
import org.elasticsearch.search.aggregations.bucket.range.Range;
import org.elasticsearch.search.aggregations.bucket.terms.Terms;
import org.elasticsearch.search.aggregations.bucket.terms.Terms.Bucket;
import org.elasticsearch.search.aggregations.bucket.terms.TermsAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.avg.Avg;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.search.fetch.subphase.highlight.HighlightBuilder;
import org.elasticsearch.search.fetch.subphase.highlight.HighlightField;
import org.elasticsearch.search.profile.ProfileResult;
import org.elasticsearch.search.profile.ProfileShardResult;
import org.elasticsearch.search.profile.aggregation.AggregationProfileShardResult;
import org.elasticsearch.search.profile.query.CollectorResult;
import org.elasticsearch.search.profile.query.QueryProfileShardResult;
import org.elasticsearch.search.sort.FieldSortBuilder;
import org.elasticsearch.search.sort.ScoreSortBuilder;
import org.elasticsearch.search.sort.SortOrder;
import org.elasticsearch.search.suggest.Suggest;
import org.elasticsearch.search.suggest.SuggestBuilder;
import org.elasticsearch.search.suggest.SuggestBuilders;
import org.elasticsearch.search.suggest.SuggestionBuilder;
import org.elasticsearch.search.suggest.term.TermSuggestion;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

import static org.elasticsearch.index.query.QueryBuilders.matchQuery;
import static org.hamcrest.Matchers.contains;
import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;

/**
 * Documentation for search APIs in the high level java client.
 * Code wrapped in {@code tag} and {@code end} tags is included in the docs.
 */
public class SearchDocumentationIT extends ESRestHighLevelClientTestCase {

    @SuppressWarnings({"unused", "unchecked"})
    public void testSearch() throws Exception {
        indexSearchTestData();
        RestHighLevelClient client = highLevelClient();
        {
            // tag::search-request-basic
            SearchRequest searchRequest = new SearchRequest(); // <1>
            SearchSourceBuilder searchSourceBuilder = new SearchSourceBuilder(); // <2>
            searchSourceBuilder.query(QueryBuilders.matchAllQuery()); // <3>
            searchRequest.source(searchSourceBuilder); // <4>
            // end::search-request-basic
        }
        {
            // tag::search-request-indices-types
            SearchRequest searchRequest = new SearchRequest("posts"); // <1>
            searchRequest.types("doc"); // <2>
            // end::search-request-indices-types
            // tag::search-request-routing
            searchRequest.routing("routing"); // <1>
            // end::search-request-routing
            // tag::search-request-indicesOptions
            searchRequest.indicesOptions(IndicesOptions.lenientExpandOpen()); // <1>
            // end::search-request-indicesOptions
            // tag::search-request-preference
            searchRequest.preference("_local"); // <1>
            // end::search-request-preference
            assertNotNull(client.search(searchRequest));
        }
        {
            // tag::search-source-basics
            SearchSourceBuilder sourceBuilder = new SearchSourceBuilder(); // <1>
            sourceBuilder.query(QueryBuilders.termQuery("user", "kimchy")); // <2>
            sourceBuilder.from(0); // <3>
            sourceBuilder.size(5); // <4>
            sourceBuilder.timeout(new TimeValue(60, TimeUnit.SECONDS)); // <5>
            // end::search-source-basics

            // tag::search-source-sorting
            sourceBuilder.sort(new ScoreSortBuilder().order(SortOrder.DESC)); // <1>
            sourceBuilder.sort(new FieldSortBuilder("_id").order(SortOrder.ASC));  // <2>
            // end::search-source-sorting

            // tag::search-source-filtering-off
            sourceBuilder.fetchSource(false);
            // end::search-source-filtering-off
            // tag::search-source-filtering-includes
            String[] includeFields = new String[] {"title", "user", "innerObject.*"};
            String[] excludeFields = new String[] {"_type"};
            sourceBuilder.fetchSource(includeFields, excludeFields);
            // end::search-source-filtering-includes
            sourceBuilder.fetchSource(true);

            // tag::search-source-setter
            SearchRequest searchRequest = new SearchRequest();
            searchRequest.indices("posts");
            searchRequest.source(sourceBuilder);
            // end::search-source-setter

            // tag::search-execute
            SearchResponse searchResponse = client.search(searchRequest);
            // end::search-execute

            // tag::search-execute-listener
            ActionListener<SearchResponse> listener = new ActionListener<SearchResponse>() {
                @Override
                public void onResponse(SearchResponse searchResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::search-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::search-execute-async
            client.searchAsync(searchRequest, listener); // <1>
            // end::search-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));

            // tag::search-response-1
            RestStatus status = searchResponse.status();
            TimeValue took = searchResponse.getTook();
            Boolean terminatedEarly = searchResponse.isTerminatedEarly();
            boolean timedOut = searchResponse.isTimedOut();
            // end::search-response-1

            // tag::search-response-2
            int totalShards = searchResponse.getTotalShards();
            int successfulShards = searchResponse.getSuccessfulShards();
            int failedShards = searchResponse.getFailedShards();
            for (ShardSearchFailure failure : searchResponse.getShardFailures()) {
                // failures should be handled here
            }
            // end::search-response-2
            assertNotNull(searchResponse);

            // tag::search-hits-get
            SearchHits hits = searchResponse.getHits();
            // end::search-hits-get
            // tag::search-hits-info
            long totalHits = hits.getTotalHits();
            float maxScore = hits.getMaxScore();
            // end::search-hits-info
            // tag::search-hits-singleHit
            SearchHit[] searchHits = hits.getHits();
            for (SearchHit hit : searchHits) {
                // do something with the SearchHit
            }
            // end::search-hits-singleHit
            for (SearchHit hit : searchHits) {
                // tag::search-hits-singleHit-properties
                String index = hit.getIndex();
                String type = hit.getType();
                String id = hit.getId();
                float score = hit.getScore();
                // end::search-hits-singleHit-properties
                // tag::search-hits-singleHit-source
                String sourceAsString = hit.getSourceAsString();
                Map<String, Object> sourceAsMap = hit.getSourceAsMap();
                String documentTitle = (String) sourceAsMap.get("title");
                List<Object> users = (List<Object>) sourceAsMap.get("user");
                Map<String, Object> innerObject =
                        (Map<String, Object>) sourceAsMap.get("innerObject");
                // end::search-hits-singleHit-source
            }
            assertEquals(3, totalHits);
            assertNotNull(hits.getHits()[0].getSourceAsString());
            assertNotNull(hits.getHits()[0].getSourceAsMap().get("title"));
            assertNotNull(hits.getHits()[0].getSourceAsMap().get("user"));
            assertNotNull(hits.getHits()[0].getSourceAsMap().get("innerObject"));
        }
    }

    @SuppressWarnings("unused")
    public void testBuildingSearchQueries() {
        RestHighLevelClient client = highLevelClient();
        {
            // tag::search-query-builder-ctor
            MatchQueryBuilder matchQueryBuilder = new MatchQueryBuilder("user", "kimchy"); // <1>
            // end::search-query-builder-ctor
            // tag::search-query-builder-options
            matchQueryBuilder.fuzziness(Fuzziness.AUTO); // <1>
            matchQueryBuilder.prefixLength(3); // <2>
            matchQueryBuilder.maxExpansions(10); // <3>
            // end::search-query-builder-options
        }
        {
            // tag::search-query-builders
            QueryBuilder matchQueryBuilder = QueryBuilders.matchQuery("user", "kimchy")
                                                            .fuzziness(Fuzziness.AUTO)
                                                            .prefixLength(3)
                                                            .maxExpansions(10);
            // end::search-query-builders
            SearchSourceBuilder searchSourceBuilder = new SearchSourceBuilder();
            // tag::search-query-setter
            searchSourceBuilder.query(matchQueryBuilder);
            // end::search-query-setter
        }
    }

    @SuppressWarnings({ "unused" })
    public void testSearchRequestAggregations() throws IOException {
        RestHighLevelClient client = highLevelClient();
        {
            BulkRequest request = new BulkRequest();
            request.add(new IndexRequest("posts", "doc", "1")
                    .source(XContentType.JSON, "company", "Elastic", "age", 20));
            request.add(new IndexRequest("posts", "doc", "2")
                    .source(XContentType.JSON, "company", "Elastic", "age", 30));
            request.add(new IndexRequest("posts", "doc", "3")
                    .source(XContentType.JSON, "company", "Elastic", "age", 40));
            request.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);
            BulkResponse bulkResponse = client.bulk(request);
            assertSame(RestStatus.OK, bulkResponse.status());
            assertFalse(bulkResponse.hasFailures());
        }
        {
            SearchRequest searchRequest = new SearchRequest();
            // tag::search-request-aggregations
            SearchSourceBuilder searchSourceBuilder = new SearchSourceBuilder();
            TermsAggregationBuilder aggregation = AggregationBuilders.terms("by_company")
                    .field("company.keyword");
            aggregation.subAggregation(AggregationBuilders.avg("average_age")
                    .field("age"));
            searchSourceBuilder.aggregation(aggregation);
            // end::search-request-aggregations
            searchSourceBuilder.query(QueryBuilders.matchAllQuery());
            searchRequest.source(searchSourceBuilder);
            SearchResponse searchResponse = client.search(searchRequest);
            {
                // tag::search-request-aggregations-get
                Aggregations aggregations = searchResponse.getAggregations();
                Terms byCompanyAggregation = aggregations.get("by_company"); // <1>
                Bucket elasticBucket = byCompanyAggregation.getBucketByKey("Elastic"); // <2>
                Avg averageAge = elasticBucket.getAggregations().get("average_age"); // <3>
                double avg = averageAge.getValue();
                // end::search-request-aggregations-get

                try {
                    // tag::search-request-aggregations-get-wrongCast
                    Range range = aggregations.get("by_company"); // <1>
                    // end::search-request-aggregations-get-wrongCast
                } catch (ClassCastException ex) {
                    assertEquals("org.elasticsearch.search.aggregations.bucket.terms.ParsedStringTerms"
                            + " cannot be cast to org.elasticsearch.search.aggregations.bucket.range.Range", ex.getMessage());
                }
                assertEquals(3, elasticBucket.getDocCount());
                assertEquals(30, avg, 0.0);
            }
            Aggregations aggregations = searchResponse.getAggregations();
            {
                // tag::search-request-aggregations-asMap
                Map<String, Aggregation> aggregationMap = aggregations.getAsMap();
                Terms companyAggregation = (Terms) aggregationMap.get("by_company");
                // end::search-request-aggregations-asMap
            }
            {
                // tag::search-request-aggregations-asList
                List<Aggregation> aggregationList = aggregations.asList();
                // end::search-request-aggregations-asList
            }
            {
                // tag::search-request-aggregations-iterator
                for (Aggregation agg : aggregations) {
                    String type = agg.getType();
                    if (type.equals(TermsAggregationBuilder.NAME)) {
                        Bucket elasticBucket = ((Terms) agg).getBucketByKey("Elastic");
                        long numberOfDocs = elasticBucket.getDocCount();
                    }
                }
                // end::search-request-aggregations-iterator
            }
        }
    }

    @SuppressWarnings({"unused", "rawtypes"})
    public void testSearchRequestSuggestions() throws IOException {
        RestHighLevelClient client = highLevelClient();
        {
            BulkRequest request = new BulkRequest();
            request.add(new IndexRequest("posts", "doc", "1").source(XContentType.JSON, "user", "kimchy"));
            request.add(new IndexRequest("posts", "doc", "2").source(XContentType.JSON, "user", "javanna"));
            request.add(new IndexRequest("posts", "doc", "3").source(XContentType.JSON, "user", "tlrx"));
            request.add(new IndexRequest("posts", "doc", "4").source(XContentType.JSON, "user", "cbuescher"));
            request.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);
            BulkResponse bulkResponse = client.bulk(request);
            assertSame(RestStatus.OK, bulkResponse.status());
            assertFalse(bulkResponse.hasFailures());
        }
        {
            SearchRequest searchRequest = new SearchRequest();
            // tag::search-request-suggestion
            SearchSourceBuilder searchSourceBuilder = new SearchSourceBuilder();
            SuggestionBuilder termSuggestionBuilder =
                SuggestBuilders.termSuggestion("user").text("kmichy"); // <1>
            SuggestBuilder suggestBuilder = new SuggestBuilder();
            suggestBuilder.addSuggestion("suggest_user", termSuggestionBuilder); // <2>
            searchSourceBuilder.suggest(suggestBuilder);
            // end::search-request-suggestion
            searchRequest.source(searchSourceBuilder);
            SearchResponse searchResponse = client.search(searchRequest);
            {
                // tag::search-request-suggestion-get
                Suggest suggest = searchResponse.getSuggest(); // <1>
                TermSuggestion termSuggestion = suggest.getSuggestion("suggest_user"); // <2>
                for (TermSuggestion.Entry entry : termSuggestion.getEntries()) { // <3>
                    for (TermSuggestion.Entry.Option option : entry) { // <4>
                        String suggestText = option.getText().string();
                    }
                }
                // end::search-request-suggestion-get
                assertEquals(1, termSuggestion.getEntries().size());
                assertEquals(1, termSuggestion.getEntries().get(0).getOptions().size());
                assertEquals("kimchy", termSuggestion.getEntries().get(0).getOptions().get(0).getText().string());
            }
        }
    }

    public void testSearchRequestHighlighting() throws IOException {
        RestHighLevelClient client = highLevelClient();
        {
            BulkRequest request = new BulkRequest();
            request.add(new IndexRequest("posts", "doc", "1")
                    .source(XContentType.JSON, "title", "In which order are my Elasticsearch queries executed?", "user",
                            Arrays.asList("kimchy", "luca"), "innerObject", Collections.singletonMap("key", "value")));
            request.add(new IndexRequest("posts", "doc", "2")
                    .source(XContentType.JSON, "title", "Current status and upcoming changes in Elasticsearch", "user",
                            Arrays.asList("kimchy", "christoph"), "innerObject", Collections.singletonMap("key", "value")));
            request.add(new IndexRequest("posts", "doc", "3")
                    .source(XContentType.JSON, "title", "The Future of Federated Search in Elasticsearch", "user",
                            Arrays.asList("kimchy", "tanguy"), "innerObject", Collections.singletonMap("key", "value")));
            request.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);
            BulkResponse bulkResponse = client.bulk(request);
            assertSame(RestStatus.OK, bulkResponse.status());
            assertFalse(bulkResponse.hasFailures());
        }
        {
            SearchRequest searchRequest = new SearchRequest();
            // tag::search-request-highlighting
            SearchSourceBuilder searchSourceBuilder = new SearchSourceBuilder();
            HighlightBuilder highlightBuilder = new HighlightBuilder(); // <1>
            HighlightBuilder.Field highlightTitle =
                    new HighlightBuilder.Field("title"); // <2>
            highlightTitle.highlighterType("unified");  // <3>
            highlightBuilder.field(highlightTitle);  // <4>
            HighlightBuilder.Field highlightUser = new HighlightBuilder.Field("user");
            highlightBuilder.field(highlightUser);
            searchSourceBuilder.highlighter(highlightBuilder);
            // end::search-request-highlighting
            searchSourceBuilder.query(QueryBuilders.boolQuery()
                    .should(matchQuery("title", "Elasticsearch"))
                    .should(matchQuery("user", "kimchy")));
            searchRequest.source(searchSourceBuilder);
            SearchResponse searchResponse = client.search(searchRequest);
            {
                // tag::search-request-highlighting-get
                SearchHits hits = searchResponse.getHits();
                for (SearchHit hit : hits.getHits()) {
                    Map<String, HighlightField> highlightFields = hit.getHighlightFields();
                    HighlightField highlight = highlightFields.get("title"); // <1>
                    Text[] fragments = highlight.fragments();  // <2>
                    String fragmentString = fragments[0].string();
                }
                // end::search-request-highlighting-get
                hits = searchResponse.getHits();
                for (SearchHit hit : hits.getHits()) {
                    Map<String, HighlightField> highlightFields = hit.getHighlightFields();
                    HighlightField highlight = highlightFields.get("title");
                    Text[] fragments = highlight.fragments();
                    assertEquals(1, fragments.length);
                    assertThat(fragments[0].string(), containsString("<em>Elasticsearch</em>"));
                    highlight = highlightFields.get("user");
                    fragments = highlight.fragments();
                    assertEquals(1, fragments.length);
                    assertThat(fragments[0].string(), containsString("<em>kimchy</em>"));
                }
            }

        }
    }

    @SuppressWarnings("unused")
    public void testSearchRequestProfiling() throws IOException {
        RestHighLevelClient client = highLevelClient();
        {
            IndexRequest request = new IndexRequest("posts", "doc", "1")
                    .source(XContentType.JSON, "tags", "elasticsearch", "comments", 123);
            request.setRefreshPolicy(WriteRequest.RefreshPolicy.WAIT_UNTIL);
            IndexResponse indexResponse = client.index(request);
            assertSame(RestStatus.CREATED, indexResponse.status());
        }
        {
            SearchRequest searchRequest = new SearchRequest();
            // tag::search-request-profiling
            SearchSourceBuilder searchSourceBuilder = new SearchSourceBuilder();
            searchSourceBuilder.profile(true);
            // end::search-request-profiling
            searchSourceBuilder.query(QueryBuilders.termQuery("tags", "elasticsearch"));
            searchSourceBuilder.aggregation(AggregationBuilders.histogram("by_comments").field("comments").interval(100));
            searchRequest.source(searchSourceBuilder);

            SearchResponse searchResponse = client.search(searchRequest);
            // tag::search-request-profiling-get
            Map<String, ProfileShardResult> profilingResults =
                    searchResponse.getProfileResults(); // <1>
            for (Map.Entry<String, ProfileShardResult> profilingResult : profilingResults.entrySet()) { // <2>
                String key = profilingResult.getKey(); // <3>
                ProfileShardResult profileShardResult = profilingResult.getValue(); // <4>
            }
            // end::search-request-profiling-get

            ProfileShardResult profileShardResult = profilingResults.values().iterator().next();
            assertNotNull(profileShardResult);

            // tag::search-request-profiling-queries
            List<QueryProfileShardResult> queryProfileShardResults =
                    profileShardResult.getQueryProfileResults(); // <1>
            for (QueryProfileShardResult queryProfileResult : queryProfileShardResults) { // <2>

            }
            // end::search-request-profiling-queries
            assertThat(queryProfileShardResults.size(), equalTo(1));

            for (QueryProfileShardResult queryProfileResult : queryProfileShardResults) {
                // tag::search-request-profiling-queries-results
                for (ProfileResult profileResult : queryProfileResult.getQueryResults()) { // <1>
                    String queryName = profileResult.getQueryName(); // <2>
                    long queryTimeInMillis = profileResult.getTime(); // <3>
                    List<ProfileResult> profiledChildren = profileResult.getProfiledChildren(); // <4>
                }
                // end::search-request-profiling-queries-results

                // tag::search-request-profiling-queries-collectors
                CollectorResult collectorResult = queryProfileResult.getCollectorResult();  // <1>
                String collectorName = collectorResult.getName();  // <2>
                Long collectorTimeInMillis = collectorResult.getTime(); // <3>
                List<CollectorResult> profiledChildren = collectorResult.getProfiledChildren(); // <4>
                // end::search-request-profiling-queries-collectors
            }

            // tag::search-request-profiling-aggs
            AggregationProfileShardResult aggsProfileResults =
                    profileShardResult.getAggregationProfileResults(); // <1>
            for (ProfileResult profileResult : aggsProfileResults.getProfileResults()) { // <2>
                String aggName = profileResult.getQueryName(); // <3>
                long aggTimeInMillis = profileResult.getTime(); // <4>
                List<ProfileResult> profiledChildren = profileResult.getProfiledChildren(); // <5>
            }
            // end::search-request-profiling-aggs
            assertThat(aggsProfileResults.getProfileResults().size(), equalTo(1));
        }
    }

    public void testScroll() throws Exception {
        RestHighLevelClient client = highLevelClient();
        {
            BulkRequest request = new BulkRequest();
            request.add(new IndexRequest("posts", "doc", "1")
                    .source(XContentType.JSON, "title", "In which order are my Elasticsearch queries executed?"));
            request.add(new IndexRequest("posts", "doc", "2")
                    .source(XContentType.JSON, "title", "Current status and upcoming changes in Elasticsearch"));
            request.add(new IndexRequest("posts", "doc", "3")
                    .source(XContentType.JSON, "title", "The Future of Federated Search in Elasticsearch"));
            request.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);
            BulkResponse bulkResponse = client.bulk(request);
            assertSame(RestStatus.OK, bulkResponse.status());
            assertFalse(bulkResponse.hasFailures());
        }
        {
            int size = 1;
            // tag::search-scroll-init
            SearchRequest searchRequest = new SearchRequest("posts");
            SearchSourceBuilder searchSourceBuilder = new SearchSourceBuilder();
            searchSourceBuilder.query(matchQuery("title", "Elasticsearch"));
            searchSourceBuilder.size(size); // <1>
            searchRequest.source(searchSourceBuilder);
            searchRequest.scroll(TimeValue.timeValueMinutes(1L)); // <2>
            SearchResponse searchResponse = client.search(searchRequest);
            String scrollId = searchResponse.getScrollId(); // <3>
            SearchHits hits = searchResponse.getHits();  // <4>
            // end::search-scroll-init
            assertEquals(3, hits.getTotalHits());
            assertEquals(1, hits.getHits().length);
            assertNotNull(scrollId);

            // tag::search-scroll2
            SearchScrollRequest scrollRequest = new SearchScrollRequest(scrollId); // <1>
            scrollRequest.scroll(TimeValue.timeValueSeconds(30));
            SearchResponse searchScrollResponse = client.searchScroll(scrollRequest);
            scrollId = searchScrollResponse.getScrollId();  // <2>
            hits = searchScrollResponse.getHits(); // <3>
            assertEquals(3, hits.getTotalHits());
            assertEquals(1, hits.getHits().length);
            assertNotNull(scrollId);
            // end::search-scroll2

            ClearScrollRequest clearScrollRequest = new ClearScrollRequest();
            clearScrollRequest.addScrollId(scrollId);
            ClearScrollResponse clearScrollResponse = client.clearScroll(clearScrollRequest);
            assertTrue(clearScrollResponse.isSucceeded());
        }
        {
            SearchRequest searchRequest = new SearchRequest();
            searchRequest.scroll("60s");

            SearchResponse initialSearchResponse = client.search(searchRequest);
            String scrollId = initialSearchResponse.getScrollId();

            SearchScrollRequest scrollRequest = new SearchScrollRequest();
            scrollRequest.scrollId(scrollId);

            // tag::scroll-request-arguments
            scrollRequest.scroll(TimeValue.timeValueSeconds(60L)); // <1>
            scrollRequest.scroll("60s"); // <2>
            // end::scroll-request-arguments

            // tag::search-scroll-execute-sync
            SearchResponse searchResponse = client.searchScroll(scrollRequest);
            // end::search-scroll-execute-sync

            assertEquals(0, searchResponse.getFailedShards());
            assertEquals(3L, searchResponse.getHits().getTotalHits());

            // tag::search-scroll-execute-listener
            ActionListener<SearchResponse> scrollListener =
                    new ActionListener<SearchResponse>() {
                @Override
                public void onResponse(SearchResponse searchResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::search-scroll-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            scrollListener = new LatchedActionListener<>(scrollListener, latch);

            // tag::search-scroll-execute-async
            client.searchScrollAsync(scrollRequest, scrollListener); // <1>
            // end::search-scroll-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));

            // tag::clear-scroll-request
            ClearScrollRequest request = new ClearScrollRequest(); // <1>
            request.addScrollId(scrollId); // <2>
            // end::clear-scroll-request

            // tag::clear-scroll-add-scroll-id
            request.addScrollId(scrollId);
            // end::clear-scroll-add-scroll-id

            List<String> scrollIds = Collections.singletonList(scrollId);

            // tag::clear-scroll-add-scroll-ids
            request.setScrollIds(scrollIds);
            // end::clear-scroll-add-scroll-ids

            // tag::clear-scroll-execute
            ClearScrollResponse response = client.clearScroll(request);
            // end::clear-scroll-execute

            // tag::clear-scroll-response
            boolean success = response.isSucceeded(); // <1>
            int released = response.getNumFreed(); // <2>
            // end::clear-scroll-response
            assertTrue(success);
            assertThat(released, greaterThan(0));

            // tag::clear-scroll-execute-listener
            ActionListener<ClearScrollResponse> listener =
                    new ActionListener<ClearScrollResponse>() {
                @Override
                public void onResponse(ClearScrollResponse clearScrollResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::clear-scroll-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch clearScrollLatch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, clearScrollLatch);

            // tag::clear-scroll-execute-async
            client.clearScrollAsync(request, listener); // <1>
            // end::clear-scroll-execute-async

            assertTrue(clearScrollLatch.await(30L, TimeUnit.SECONDS));
        }
        {
            // tag::search-scroll-example
            final Scroll scroll = new Scroll(TimeValue.timeValueMinutes(1L));
            SearchRequest searchRequest = new SearchRequest("posts");
            searchRequest.scroll(scroll);
            SearchSourceBuilder searchSourceBuilder = new SearchSourceBuilder();
            searchSourceBuilder.query(matchQuery("title", "Elasticsearch"));
            searchRequest.source(searchSourceBuilder);

            SearchResponse searchResponse = client.search(searchRequest); // <1>
            String scrollId = searchResponse.getScrollId();
            SearchHit[] searchHits = searchResponse.getHits().getHits();

            while (searchHits != null && searchHits.length > 0) { // <2>
                SearchScrollRequest scrollRequest = new SearchScrollRequest(scrollId); // <3>
                scrollRequest.scroll(scroll);
                searchResponse = client.searchScroll(scrollRequest);
                scrollId = searchResponse.getScrollId();
                searchHits = searchResponse.getHits().getHits();
                // <4>
            }

            ClearScrollRequest clearScrollRequest = new ClearScrollRequest(); // <5>
            clearScrollRequest.addScrollId(scrollId);
            ClearScrollResponse clearScrollResponse = client.clearScroll(clearScrollRequest);
            boolean succeeded = clearScrollResponse.isSucceeded();
            // end::search-scroll-example
            assertTrue(succeeded);
        }
    }

    public void testFieldCaps() throws Exception {
        indexSearchTestData();
        RestHighLevelClient client = highLevelClient();
        // tag::field-caps-request
        FieldCapabilitiesRequest request = new FieldCapabilitiesRequest()
            .fields("user")
            .indices("posts", "authors", "contributors");
        // end::field-caps-request

        // tag::field-caps-request-indicesOptions
        request.indicesOptions(IndicesOptions.lenientExpandOpen()); // <1>
        // end::field-caps-request-indicesOptions

        // tag::field-caps-execute
        FieldCapabilitiesResponse response = client.fieldCaps(request);
        // end::field-caps-execute

        // tag::field-caps-response
        assertThat(response.get().keySet(), contains("user"));
        Map<String, FieldCapabilities> userResponse = response.getField("user");

        assertThat(userResponse.keySet(), containsInAnyOrder("keyword", "text")); // <1>
        FieldCapabilities textCapabilities = userResponse.get("keyword");

        assertTrue(textCapabilities.isSearchable());
        assertFalse(textCapabilities.isAggregatable());

        assertArrayEquals(textCapabilities.indices(), // <2>
                          new String[]{"authors", "contributors"});
        assertNull(textCapabilities.nonSearchableIndices()); // <3>
        assertArrayEquals(textCapabilities.nonAggregatableIndices(), // <4>
                          new String[]{"authors"});
        // end::field-caps-response

        // tag::field-caps-execute-listener
        ActionListener<FieldCapabilitiesResponse> listener = new ActionListener<FieldCapabilitiesResponse>() {
            @Override
            public void onResponse(FieldCapabilitiesResponse response) {
                // <1>
            }

            @Override
            public void onFailure(Exception e) {
                // <2>
            }
        };
        // end::field-caps-execute-listener

        // Replace the empty listener by a blocking listener for tests.
        CountDownLatch latch = new CountDownLatch(1);
        listener = new LatchedActionListener<>(listener, latch);

        // tag::field-caps-execute-async
        client.fieldCapsAsync(request, listener); // <1>
        // end::field-caps-execute-async

        assertTrue(latch.await(30L, TimeUnit.SECONDS));
    }

    public void testRankEval() throws Exception {
        indexSearchTestData();
        RestHighLevelClient client = highLevelClient();
        {
            // tag::rank-eval-request-basic
            EvaluationMetric metric = new PrecisionAtK();                 // <1>
            List<RatedDocument> ratedDocs = new ArrayList<>();
            ratedDocs.add(new RatedDocument("posts", "1", 1));            // <2>
            SearchSourceBuilder searchQuery = new SearchSourceBuilder();
            searchQuery.query(QueryBuilders.matchQuery("user", "kimchy"));// <3>
            RatedRequest ratedRequest =                                   // <4>
                    new RatedRequest("kimchy_query", ratedDocs, searchQuery);
            List<RatedRequest> ratedRequests = Arrays.asList(ratedRequest);
            RankEvalSpec specification =
                    new RankEvalSpec(ratedRequests, metric);              // <5>
            RankEvalRequest request =                                     // <6>
                    new RankEvalRequest(specification, new String[] { "posts" });
            // end::rank-eval-request-basic

            // tag::rank-eval-execute
            RankEvalResponse response = client.rankEval(request);
            // end::rank-eval-execute

            // tag::rank-eval-response
            double evaluationResult = response.getEvaluationResult();   // <1>
            assertEquals(1.0 / 3.0, evaluationResult, 0.0);
            Map<String, EvalQueryQuality> partialResults =
                    response.getPartialResults();
            EvalQueryQuality evalQuality =
                    partialResults.get("kimchy_query");                 // <2>
            assertEquals("kimchy_query", evalQuality.getId());
            double qualityLevel = evalQuality.getQualityLevel();        // <3>
            assertEquals(1.0 / 3.0, qualityLevel, 0.0);
            List<RatedSearchHit> hitsAndRatings = evalQuality.getHitsAndRatings();
            RatedSearchHit ratedSearchHit = hitsAndRatings.get(2);
            assertEquals("3", ratedSearchHit.getSearchHit().getId());   // <4>
            assertFalse(ratedSearchHit.getRating().isPresent());        // <5>
            MetricDetail metricDetails = evalQuality.getMetricDetails();
            String metricName = metricDetails.getMetricName();
            assertEquals(PrecisionAtK.NAME, metricName);                // <6>
            PrecisionAtK.Detail detail = (PrecisionAtK.Detail) metricDetails;
            assertEquals(1, detail.getRelevantRetrieved());             // <7>
            assertEquals(3, detail.getRetrieved());
            // end::rank-eval-response

            // tag::rank-eval-execute-listener
            ActionListener<RankEvalResponse> listener = new ActionListener<RankEvalResponse>() {
                @Override
                public void onResponse(RankEvalResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::rank-eval-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::rank-eval-execute-async
            client.rankEvalAsync(request, listener); // <1>
            // end::rank-eval-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testMultiSearch() throws Exception {
        indexSearchTestData();
        RestHighLevelClient client = highLevelClient();
        {
            // tag::multi-search-request-basic
            MultiSearchRequest request = new MultiSearchRequest();    // <1>
            SearchRequest firstSearchRequest = new SearchRequest();   // <2>
            SearchSourceBuilder searchSourceBuilder = new SearchSourceBuilder();
            searchSourceBuilder.query(QueryBuilders.matchQuery("user", "kimchy"));
            firstSearchRequest.source(searchSourceBuilder);
            request.add(firstSearchRequest);                          // <3>
            SearchRequest secondSearchRequest = new SearchRequest();  // <4>
            searchSourceBuilder = new SearchSourceBuilder();
            searchSourceBuilder.query(QueryBuilders.matchQuery("user", "luca"));
            secondSearchRequest.source(searchSourceBuilder);
            request.add(secondSearchRequest);
            // end::multi-search-request-basic
            // tag::multi-search-execute
            MultiSearchResponse response = client.multiSearch(request);
            // end::multi-search-execute
            // tag::multi-search-response
            MultiSearchResponse.Item firstResponse = response.getResponses()[0];   // <1>
            assertNull(firstResponse.getFailure());                                // <2>
            SearchResponse searchResponse = firstResponse.getResponse();           // <3>
            assertEquals(4, searchResponse.getHits().getTotalHits());
            MultiSearchResponse.Item secondResponse = response.getResponses()[1];  // <4>
            assertNull(secondResponse.getFailure());
            searchResponse = secondResponse.getResponse();
            assertEquals(1, searchResponse.getHits().getTotalHits());
            // end::multi-search-response

            // tag::multi-search-execute-listener
            ActionListener<MultiSearchResponse> listener = new ActionListener<MultiSearchResponse>() {
                @Override
                public void onResponse(MultiSearchResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::multi-search-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::multi-search-execute-async
            client.multiSearchAsync(request, listener); // <1>
            // end::multi-search-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
        {
            // tag::multi-search-request-index
            MultiSearchRequest request = new MultiSearchRequest();
            request.add(new SearchRequest("posts")  // <1>
                    .types("doc"));                 // <2>
            // end::multi-search-request-index
            MultiSearchResponse response = client.multiSearch(request);
            MultiSearchResponse.Item firstResponse = response.getResponses()[0];
            assertNull(firstResponse.getFailure());
            SearchResponse searchResponse = firstResponse.getResponse();
            assertEquals(3, searchResponse.getHits().getTotalHits());
        }
    }

    private void indexSearchTestData() throws IOException {
        CreateIndexRequest authorsRequest = new CreateIndexRequest("authors")
            .mapping("doc", "user", "type=keyword,doc_values=false");
        CreateIndexResponse authorsResponse = highLevelClient().indices().create(authorsRequest);
        assertTrue(authorsResponse.isAcknowledged());

        CreateIndexRequest reviewersRequest = new CreateIndexRequest("contributors")
            .mapping("doc", "user", "type=keyword");
        CreateIndexResponse reviewersResponse = highLevelClient().indices().create(reviewersRequest);
        assertTrue(reviewersResponse.isAcknowledged());

        BulkRequest bulkRequest = new BulkRequest();
        bulkRequest.add(new IndexRequest("posts", "doc", "1")
                .source(XContentType.JSON, "title", "In which order are my Elasticsearch queries executed?", "user",
                        Arrays.asList("kimchy", "luca"), "innerObject", Collections.singletonMap("key", "value")));
        bulkRequest.add(new IndexRequest("posts", "doc", "2")
                .source(XContentType.JSON, "title", "Current status and upcoming changes in Elasticsearch", "user",
                        Arrays.asList("kimchy", "christoph"), "innerObject", Collections.singletonMap("key", "value")));
        bulkRequest.add(new IndexRequest("posts", "doc", "3")
                .source(XContentType.JSON, "title", "The Future of Federated Search in Elasticsearch", "user",
                        Arrays.asList("kimchy", "tanguy"), "innerObject", Collections.singletonMap("key", "value")));

        bulkRequest.add(new IndexRequest("authors", "doc", "1")
            .source(XContentType.JSON, "user", "kimchy"));
        bulkRequest.add(new IndexRequest("contributors", "doc", "1")
            .source(XContentType.JSON, "user", "tanguy"));


        bulkRequest.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);
        BulkResponse bulkResponse = highLevelClient().bulk(bulkRequest);
        assertSame(RestStatus.OK, bulkResponse.status());
        assertFalse(bulkResponse.hasFailures());
    }
}
