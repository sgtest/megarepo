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

package org.elasticsearch.client.analytics;

import org.elasticsearch.action.bulk.BulkRequest;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.support.WriteRequest.RefreshPolicy;
import org.elasticsearch.client.ESRestHighLevelClientTestCase;
import org.elasticsearch.client.RequestOptions;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.search.sort.FieldSortBuilder;
import org.elasticsearch.search.sort.SortOrder;

import java.io.IOException;

import static java.util.Collections.singletonList;
import static java.util.Collections.singletonMap;
import static org.hamcrest.Matchers.aMapWithSize;
import static org.hamcrest.Matchers.closeTo;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasEntry;
import static org.hamcrest.Matchers.hasSize;

public class AnalyticsAggsIT extends ESRestHighLevelClientTestCase {
    public void testStringStats() throws IOException {
        BulkRequest bulk = new BulkRequest("test").setRefreshPolicy(RefreshPolicy.IMMEDIATE);
        bulk.add(new IndexRequest().source(XContentType.JSON, "message", "trying out elasticsearch"));
        bulk.add(new IndexRequest().source(XContentType.JSON, "message", "more words"));
        highLevelClient().bulk(bulk, RequestOptions.DEFAULT);
        SearchRequest search = new SearchRequest("test");
        search.source().aggregation(new StringStatsAggregationBuilder("test").field("message.keyword").showDistribution(true));
        SearchResponse response = highLevelClient().search(search, RequestOptions.DEFAULT);
        ParsedStringStats stats = response.getAggregations().get("test");
        assertThat(stats.getCount(), equalTo(2L));
        assertThat(stats.getMinLength(), equalTo(10));
        assertThat(stats.getMaxLength(), equalTo(24));
        assertThat(stats.getAvgLength(), equalTo(17.0));
        assertThat(stats.getEntropy(), closeTo(4, .1));
        assertThat(stats.getDistribution(), aMapWithSize(18));
        assertThat(stats.getDistribution(), hasEntry(equalTo("o"), closeTo(.09, .005)));
        assertThat(stats.getDistribution(), hasEntry(equalTo("r"), closeTo(.12, .005)));
        assertThat(stats.getDistribution(), hasEntry(equalTo("t"), closeTo(.09, .005)));
    }

    public void testBasic() throws IOException {
        BulkRequest bulk = new BulkRequest("test").setRefreshPolicy(RefreshPolicy.IMMEDIATE);
        bulk.add(new IndexRequest().source(XContentType.JSON, "s", 1, "v", 2));
        bulk.add(new IndexRequest().source(XContentType.JSON, "s", 2, "v", 3));
        highLevelClient().bulk(bulk, RequestOptions.DEFAULT);
        SearchRequest search = new SearchRequest("test");
        search.source().aggregation(new TopMetricsAggregationBuilder(
                "test", new FieldSortBuilder("s").order(SortOrder.DESC), "v"));
        SearchResponse response = highLevelClient().search(search, RequestOptions.DEFAULT);
        ParsedTopMetrics top = response.getAggregations().get("test");
        assertThat(top.getTopMetrics(), hasSize(1));
        ParsedTopMetrics.TopMetrics metric = top.getTopMetrics().get(0);
        assertThat(metric.getSort(), equalTo(singletonList(2)));
        assertThat(metric.getMetrics(), equalTo(singletonMap("v", 3.0)));
    }
}
