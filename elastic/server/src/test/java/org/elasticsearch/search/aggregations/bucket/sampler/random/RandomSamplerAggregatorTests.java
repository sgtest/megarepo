/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.search.aggregations.bucket.sampler.random;

import org.apache.lucene.document.SortedNumericDocValuesField;
import org.apache.lucene.document.SortedSetDocValuesField;
import org.apache.lucene.search.MatchAllDocsQuery;
import org.apache.lucene.tests.index.RandomIndexWriter;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.index.mapper.KeywordFieldMapper;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.search.aggregations.AggregationBuilders;
import org.elasticsearch.search.aggregations.AggregatorTestCase;
import org.elasticsearch.search.aggregations.bucket.filter.Filter;
import org.elasticsearch.search.aggregations.metrics.Avg;

import java.io.IOException;
import java.util.List;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.stream.DoubleStream;
import java.util.stream.LongStream;

import static org.hamcrest.Matchers.allOf;
import static org.hamcrest.Matchers.closeTo;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;
import static org.hamcrest.Matchers.greaterThanOrEqualTo;
import static org.hamcrest.Matchers.lessThanOrEqualTo;

public class RandomSamplerAggregatorTests extends AggregatorTestCase {

    private static final String NUMERIC_FIELD_NAME = "value";
    private static final String KEYWORD_FIELD_NAME = "keyword";
    private static final String KEYWORD_FIELD_VALUE = "foo";

    public void testAggregationSampling() throws IOException {
        double[] avgs = new double[5];
        long[] counts = new long[5];
        AtomicInteger integer = new AtomicInteger();
        do {
            testCase(
                new RandomSamplerAggregationBuilder("my_agg").subAggregation(AggregationBuilders.avg("avg").field(NUMERIC_FIELD_NAME))
                    .setProbability(0.25),
                new MatchAllDocsQuery(),
                RandomSamplerAggregatorTests::writeTestDocs,
                (InternalRandomSampler result) -> {
                    counts[integer.get()] = result.getDocCount();
                    Avg agg = result.getAggregations().get("avg");
                    assertTrue(Double.isNaN(agg.getValue()) == false && Double.isFinite(agg.getValue()));
                    avgs[integer.get()] = agg.getValue();
                },
                longField(NUMERIC_FIELD_NAME)
            );
        } while (integer.incrementAndGet() < 5);
        long avgCount = LongStream.of(counts).sum() / integer.get();
        double avgAvg = DoubleStream.of(avgs).sum() / integer.get();
        assertThat(avgCount, allOf(greaterThanOrEqualTo(20L), lessThanOrEqualTo(70L)));
        assertThat(avgAvg, closeTo(1.5, 0.5));
    }

    public void testAggregationSamplingNestedAggsScaled() throws IOException {
        testCase(
            new RandomSamplerAggregationBuilder("my_agg").subAggregation(
                AggregationBuilders.filter("filter_outer", QueryBuilders.termsQuery(KEYWORD_FIELD_NAME, KEYWORD_FIELD_VALUE))
                    .subAggregation(
                        AggregationBuilders.filter("filter_inner", QueryBuilders.termsQuery(KEYWORD_FIELD_NAME, KEYWORD_FIELD_VALUE))
                    )
            ).setProbability(0.25),
            new MatchAllDocsQuery(),
            RandomSamplerAggregatorTests::writeTestDocs,
            (InternalRandomSampler result) -> {
                long sampledDocCount = result.getDocCount();
                Filter agg = result.getAggregations().get("filter_outer");
                long outerFilterDocCount = agg.getDocCount();
                Filter innerAgg = agg.getAggregations().get("filter_inner");
                long innerFilterDocCount = innerAgg.getDocCount();
                // subaggs should be scaled along with upper level aggs
                assertThat(outerFilterDocCount, equalTo(innerFilterDocCount));
                // sampled doc count is NOT scaled, and thus should be lower
                assertThat(outerFilterDocCount, greaterThan(sampledDocCount));
            },
            longField(NUMERIC_FIELD_NAME),
            keywordField(KEYWORD_FIELD_NAME)
        );
    }

    private static void writeTestDocs(RandomIndexWriter w) throws IOException {
        for (int i = 0; i < 75; i++) {
            w.addDocument(
                List.of(
                    new SortedNumericDocValuesField(NUMERIC_FIELD_NAME, 1),
                    new SortedSetDocValuesField(KEYWORD_FIELD_NAME, new BytesRef(KEYWORD_FIELD_VALUE)),
                    new KeywordFieldMapper.KeywordField(
                        KEYWORD_FIELD_NAME,
                        new BytesRef(KEYWORD_FIELD_VALUE),
                        KeywordFieldMapper.Defaults.FIELD_TYPE
                    )
                )
            );
        }
        for (int i = 0; i < 75; i++) {
            w.addDocument(
                List.of(
                    new SortedNumericDocValuesField(NUMERIC_FIELD_NAME, 2),
                    new SortedSetDocValuesField(KEYWORD_FIELD_NAME, new BytesRef(KEYWORD_FIELD_VALUE)),
                    new KeywordFieldMapper.KeywordField(
                        KEYWORD_FIELD_NAME,
                        new BytesRef(KEYWORD_FIELD_VALUE),
                        KeywordFieldMapper.Defaults.FIELD_TYPE
                    )
                )
            );
        }
    }

}
