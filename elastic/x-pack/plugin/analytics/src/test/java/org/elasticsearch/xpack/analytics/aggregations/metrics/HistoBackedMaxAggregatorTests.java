/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.analytics.aggregations.metrics;

import com.tdunning.math.stats.Centroid;
import com.tdunning.math.stats.TDigest;
import org.apache.lucene.document.BinaryDocValuesField;
import org.apache.lucene.document.Field;
import org.apache.lucene.document.StringField;
import org.apache.lucene.index.RandomIndexWriter;
import org.apache.lucene.index.Term;
import org.apache.lucene.search.MatchAllDocsQuery;
import org.apache.lucene.search.Query;
import org.apache.lucene.search.TermQuery;
import org.elasticsearch.common.CheckedConsumer;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.plugins.SearchPlugin;
import org.elasticsearch.search.aggregations.AggregationBuilder;
import org.elasticsearch.search.aggregations.AggregatorTestCase;
import org.elasticsearch.search.aggregations.metrics.InternalMax;
import org.elasticsearch.search.aggregations.metrics.MaxAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.TDigestState;
import org.elasticsearch.search.aggregations.support.AggregationInspectionHelper;
import org.elasticsearch.search.aggregations.support.CoreValuesSourceType;
import org.elasticsearch.search.aggregations.support.ValuesSourceType;
import org.elasticsearch.xpack.analytics.AnalyticsPlugin;
import org.elasticsearch.xpack.analytics.aggregations.support.AnalyticsValuesSourceType;
import org.elasticsearch.xpack.analytics.mapper.HistogramFieldMapper;

import java.io.IOException;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.Iterator;
import java.util.List;
import java.util.function.Consumer;

import static java.util.Collections.singleton;
import static org.elasticsearch.search.aggregations.AggregationBuilders.max;

public class HistoBackedMaxAggregatorTests extends AggregatorTestCase {

    private static final String FIELD_NAME = "field";

    public void testNoDocs() throws IOException {
        testCase(new MatchAllDocsQuery(), iw -> {
            // Intentionally not writing any docs
        }, max -> {
            assertEquals(Double.NEGATIVE_INFINITY, max.getValue(), 0d);
            assertFalse(AggregationInspectionHelper.hasValue(max));
        });
    }

    public void testNoMatchingField() throws IOException {
        testCase(new MatchAllDocsQuery(), iw -> {
            iw.addDocument(singleton(getDocValue("wrong_field", new double[] {3, 1.2, 10})));
            iw.addDocument(singleton(getDocValue("wrong_field", new double[] {5.3, 6, 20})));
        }, max -> {
            assertEquals(Double.NEGATIVE_INFINITY, max.getValue(), 0d);
            assertFalse(AggregationInspectionHelper.hasValue(max));
        });
    }

    public void testSimpleHistogram() throws IOException {
        testCase(new MatchAllDocsQuery(), iw -> {
            iw.addDocument(singleton(getDocValue(FIELD_NAME, new double[] {3, 1.2, 10})));
            iw.addDocument(singleton(getDocValue(FIELD_NAME, new double[] {5.3, 6, 6, 20})));
            iw.addDocument(singleton(getDocValue(FIELD_NAME, new double[] {-10, 0.01, 1, 90})));
        }, max -> {
            assertEquals(90d, max.getValue(), 0.01d);
            assertTrue(AggregationInspectionHelper.hasValue(max));
        });
    }

    public void testQueryFiltering() throws IOException {
        testCase(new TermQuery(new Term("match", "yes")), iw -> {
            iw.addDocument(Arrays.asList(
                new StringField("match", "yes", Field.Store.NO),
                getDocValue(FIELD_NAME, new double[] {3, 1.2, 10}))
            );
            iw.addDocument(Arrays.asList(
                new StringField("match", "yes", Field.Store.NO),
                getDocValue(FIELD_NAME, new double[] {5.3, 6, 20}))
            );
            iw.addDocument(Arrays.asList(
                new StringField("match", "no", Field.Store.NO),
                getDocValue(FIELD_NAME, new double[] {-34, 1.2, 10}))
            );
            iw.addDocument(Arrays.asList(
                new StringField("match", "no", Field.Store.NO),
                getDocValue(FIELD_NAME, new double[] {3, 1.2, 100}))
            );
            iw.addDocument(Arrays.asList(
                new StringField("match", "yes", Field.Store.NO),
                getDocValue(FIELD_NAME, new double[] {-10, 0.01, 1, 90}))
            );
        }, min -> {
            assertEquals(90d, min.getValue(), 0.01d);
            assertTrue(AggregationInspectionHelper.hasValue(min));
        });
    }

    private void testCase(Query query,
                          CheckedConsumer<RandomIndexWriter, IOException> indexer,
                          Consumer<InternalMax> verify) throws IOException {
        testCase(max("_name").field(FIELD_NAME), query, indexer, verify, defaultFieldType());
    }

    private BinaryDocValuesField getDocValue(String fieldName, double[] values) throws IOException {
        TDigest histogram = new TDigestState(100.0); //default
        for (double value : values) {
            histogram.add(value);
        }
        BytesStreamOutput streamOutput = new BytesStreamOutput();
        histogram.compress();
        Collection<Centroid> centroids = histogram.centroids();
        Iterator<Centroid> iterator = centroids.iterator();
        while ( iterator.hasNext()) {
            Centroid centroid = iterator.next();
            streamOutput.writeVInt(centroid.count());
            streamOutput.writeDouble(centroid.mean());
        }
        return new BinaryDocValuesField(fieldName, streamOutput.bytes().toBytesRef());
    }

    @Override
    protected List<SearchPlugin> getSearchPlugins() {
        return List.of(new AnalyticsPlugin());
    }

    @Override
    protected List<ValuesSourceType> getSupportedValuesSourceTypes() {
        // Note: this is the same list as Core, plus Analytics
        return List.of(
            CoreValuesSourceType.NUMERIC,
            CoreValuesSourceType.BOOLEAN,
            CoreValuesSourceType.DATE,
            AnalyticsValuesSourceType.HISTOGRAM
        );
    }

    @Override
    protected AggregationBuilder createAggBuilderForTypeTest(MappedFieldType fieldType, String fieldName) {
        return new MaxAggregationBuilder("_name").field(fieldName);
    }

    private MappedFieldType defaultFieldType() {
        return new HistogramFieldMapper.HistogramFieldType(HistoBackedMaxAggregatorTests.FIELD_NAME, true, Collections.emptyMap());
    }
}
