/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.dataframe.integration;

import org.elasticsearch.client.RequestOptions;
import org.elasticsearch.client.core.IndexerState;
import org.elasticsearch.client.dataframe.transforms.DataFrameTransformConfig;
import org.elasticsearch.client.dataframe.transforms.DataFrameTransformStateAndStats;
import org.elasticsearch.client.dataframe.transforms.pivot.SingleGroupSource;
import org.elasticsearch.client.dataframe.transforms.pivot.TermsGroupSource;
import org.elasticsearch.search.aggregations.AggregationBuilders;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.elasticsearch.search.aggregations.bucket.histogram.DateHistogramInterval;
import org.junit.After;

import java.io.IOException;
import java.util.HashMap;
import java.util.Map;

import static org.hamcrest.Matchers.equalTo;

public class DataFrameTransformIT extends DataFrameIntegTestCase {

    @After
    public void cleanTransforms() throws IOException {
        cleanUp();
    }

    @AwaitsFix( bugUrl = "https://github.com/elastic/elasticsearch/issues/42344")
    public void testDataFrameTransformCrud() throws Exception {
        createReviewsIndex();

        Map<String, SingleGroupSource> groups = new HashMap<>();
        groups.put("by-day", createDateHistogramGroupSourceWithCalendarInterval("timestamp", DateHistogramInterval.DAY, null, null));
        groups.put("by-user", TermsGroupSource.builder().setField("user_id").build());
        groups.put("by-business", TermsGroupSource.builder().setField("business_id").build());

        AggregatorFactories.Builder aggs = AggregatorFactories.builder()
            .addAggregator(AggregationBuilders.avg("review_score").field("stars"))
            .addAggregator(AggregationBuilders.max("timestamp").field("timestamp"));

        DataFrameTransformConfig config = createTransformConfig("data-frame-transform-crud",
            groups,
            aggs,
            "reviews-by-user-business-day",
            REVIEWS_INDEX_NAME);

        assertTrue(putDataFrameTransform(config, RequestOptions.DEFAULT).isAcknowledged());
        assertTrue(startDataFrameTransform(config.getId(), RequestOptions.DEFAULT).isAcknowledged());

        waitUntilCheckpoint(config.getId(), 1L);

        DataFrameTransformStateAndStats stats = getDataFrameTransformStats(config.getId()).getTransformsStateAndStats().get(0);

        assertThat(stats.getTransformState().getIndexerState(), equalTo(IndexerState.STARTED));
    }


}
