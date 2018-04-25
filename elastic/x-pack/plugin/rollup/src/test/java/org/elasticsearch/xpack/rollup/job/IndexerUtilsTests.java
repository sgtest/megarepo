/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.rollup.job;

import org.apache.lucene.document.Document;
import org.apache.lucene.document.LongPoint;
import org.apache.lucene.document.SortedNumericDocValuesField;
import org.apache.lucene.index.DirectoryReader;
import org.apache.lucene.index.IndexReader;
import org.apache.lucene.index.RandomIndexWriter;
import org.apache.lucene.search.IndexSearcher;
import org.apache.lucene.search.MatchAllDocsQuery;
import org.apache.lucene.store.Directory;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.index.mapper.DateFieldMapper;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.index.mapper.NumberFieldMapper;
import org.elasticsearch.search.aggregations.Aggregation;
import org.elasticsearch.search.aggregations.Aggregations;
import org.elasticsearch.search.aggregations.Aggregator;
import org.elasticsearch.search.aggregations.AggregatorTestCase;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregation;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.composite.DateHistogramValuesSourceBuilder;
import org.elasticsearch.search.aggregations.bucket.composite.TermsValuesSourceBuilder;
import org.elasticsearch.search.aggregations.bucket.histogram.DateHistogramAggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.histogram.DateHistogramInterval;
import org.elasticsearch.search.aggregations.bucket.terms.TermsAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.InternalNumericMetricsAggregation;
import org.elasticsearch.search.aggregations.metrics.avg.AvgAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.max.MaxAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.sum.SumAggregationBuilder;
import org.elasticsearch.xpack.core.rollup.RollupField;
import org.elasticsearch.xpack.core.rollup.job.DateHistoGroupConfig;
import org.elasticsearch.xpack.core.rollup.job.GroupConfig;
import org.elasticsearch.xpack.core.rollup.job.MetricConfig;
import org.elasticsearch.xpack.core.rollup.job.RollupJobStats;
import org.elasticsearch.xpack.core.rollup.ConfigTestHelpers;
import org.elasticsearch.xpack.rollup.Rollup;
import org.joda.time.DateTime;
import org.mockito.stubbing.Answer;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import static org.mockito.Mockito.mock;
import static org.hamcrest.Matchers.equalTo;
import static org.mockito.Mockito.when;

public class IndexerUtilsTests extends AggregatorTestCase {
    public void testMissingFields() throws IOException {
        String indexName = randomAlphaOfLengthBetween(1, 10);
        RollupJobStats stats = new RollupJobStats(0, 0, 0, 0);

        String timestampField = "the_histo";
        String valueField = "the_avg";

        Directory directory = newDirectory();
        RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory);

        int numDocs = randomIntBetween(1,10);
        for (int i = 0; i < numDocs; i++) {
            Document document = new Document();
            long timestamp = new DateTime().minusDays(i).getMillis();
            document.add(new SortedNumericDocValuesField(timestampField, timestamp));
            document.add(new LongPoint(timestampField, timestamp));
            document.add(new SortedNumericDocValuesField(valueField, randomIntBetween(1,100)));
            indexWriter.addDocument(document);
        }

        indexWriter.close();

        IndexReader indexReader = DirectoryReader.open(directory);
        IndexSearcher indexSearcher = newIndexSearcher(indexReader);

        DateFieldMapper.Builder builder = new DateFieldMapper.Builder(timestampField);
        DateFieldMapper.DateFieldType timestampFieldType = builder.fieldType();
        timestampFieldType.setHasDocValues(true);
        timestampFieldType.setName(timestampField);

        MappedFieldType valueFieldType = new NumberFieldMapper.NumberFieldType(NumberFieldMapper.NumberType.LONG);
        valueFieldType.setName(valueField);
        valueFieldType.setHasDocValues(true);
        valueFieldType.setName(valueField);

        // Setup the composite agg
        //TODO swap this over to DateHistoConfig.Builder once DateInterval is in
        DateHistoGroupConfig dateHistoGroupConfig = new DateHistoGroupConfig.Builder()
                .setField(timestampField)
                .setInterval(DateHistogramInterval.days(1))
                .build();
        CompositeAggregationBuilder compositeBuilder =
                new CompositeAggregationBuilder(RollupIndexer.AGGREGATION_NAME, dateHistoGroupConfig.toBuilders());
        MetricConfig metricConfig = new MetricConfig.Builder()
                .setField("does_not_exist")
                .setMetrics(Collections.singletonList("max"))
                .build();
        metricConfig.toBuilders().forEach(compositeBuilder::subAggregation);

        Aggregator aggregator = createAggregator(compositeBuilder, indexSearcher, timestampFieldType, valueFieldType);
        aggregator.preCollection();
        indexSearcher.search(new MatchAllDocsQuery(), aggregator);
        aggregator.postCollection();
        CompositeAggregation composite = (CompositeAggregation) aggregator.buildAggregation(0L);
        indexReader.close();
        directory.close();

        List<IndexRequest> docs = IndexerUtils.processBuckets(composite, indexName, stats,
                ConfigTestHelpers.getGroupConfig().build(), "foo");

        assertThat(docs.size(), equalTo(numDocs));
        for (IndexRequest doc : docs) {
            Map<String, Object> map = doc.sourceAsMap();
            assertNull(map.get("does_not_exist"));
            assertThat(map.get("the_histo." + DateHistogramAggregationBuilder.NAME + "." + RollupField.COUNT_FIELD), equalTo(1));
        }
    }

    public void testCorrectFields() throws IOException {
        String indexName = randomAlphaOfLengthBetween(1, 10);
        RollupJobStats stats= new RollupJobStats(0, 0, 0, 0);

        String timestampField = "the_histo";
        String valueField = "the_avg";

        Directory directory = newDirectory();
        RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory);

        int numDocs = randomIntBetween(1,10);
        for (int i = 0; i < numDocs; i++) {
            Document document = new Document();
            long timestamp = new DateTime().minusDays(i).getMillis();
            document.add(new SortedNumericDocValuesField(timestampField, timestamp));
            document.add(new LongPoint(timestampField, timestamp));
            document.add(new SortedNumericDocValuesField(valueField, randomIntBetween(1,100)));
            indexWriter.addDocument(document);
        }

        indexWriter.close();

        IndexReader indexReader = DirectoryReader.open(directory);
        IndexSearcher indexSearcher = newIndexSearcher(indexReader);

        DateFieldMapper.Builder builder = new DateFieldMapper.Builder(timestampField);
        DateFieldMapper.DateFieldType timestampFieldType = builder.fieldType();
        timestampFieldType.setHasDocValues(true);
        timestampFieldType.setName(timestampField);

        MappedFieldType valueFieldType = new NumberFieldMapper.NumberFieldType(NumberFieldMapper.NumberType.LONG);
        valueFieldType.setName(valueField);
        valueFieldType.setHasDocValues(true);
        valueFieldType.setName(valueField);

        // Setup the composite agg
        //TODO swap this over to DateHistoConfig.Builder once DateInterval is in
        DateHistogramValuesSourceBuilder dateHisto
                = new DateHistogramValuesSourceBuilder("the_histo." + DateHistogramAggregationBuilder.NAME)
                .field(timestampField)
                .interval(1);

        CompositeAggregationBuilder compositeBuilder = new CompositeAggregationBuilder(RollupIndexer.AGGREGATION_NAME,
                Collections.singletonList(dateHisto));

        MetricConfig metricConfig = new MetricConfig.Builder().setField(valueField).setMetrics(Collections.singletonList("max")).build();
        metricConfig.toBuilders().forEach(compositeBuilder::subAggregation);

        Aggregator aggregator = createAggregator(compositeBuilder, indexSearcher, timestampFieldType, valueFieldType);
        aggregator.preCollection();
        indexSearcher.search(new MatchAllDocsQuery(), aggregator);
        aggregator.postCollection();
        CompositeAggregation composite = (CompositeAggregation) aggregator.buildAggregation(0L);
        indexReader.close();
        directory.close();

        List<IndexRequest> docs = IndexerUtils.processBuckets(composite, indexName, stats,
                ConfigTestHelpers.getGroupConfig().build(), "foo");

        assertThat(docs.size(), equalTo(numDocs));
        for (IndexRequest doc : docs) {
            Map<String, Object> map = doc.sourceAsMap();
            assertNotNull( map.get(valueField + "." + MaxAggregationBuilder.NAME + "." + RollupField.VALUE));
            assertThat(map.get("the_histo." + DateHistogramAggregationBuilder.NAME + "." + RollupField.COUNT_FIELD), equalTo(1));
        }
    }

    public void testNumericTerms() throws IOException {
        String indexName = randomAlphaOfLengthBetween(1, 10);
        RollupJobStats stats= new RollupJobStats(0, 0, 0, 0);

        String timestampField = "the_histo";
        String valueField = "the_avg";

        Directory directory = newDirectory();
        RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory);

        int numDocs = randomIntBetween(1,10);
        for (int i = 0; i < numDocs; i++) {
            Document document = new Document();
            document.add(new SortedNumericDocValuesField(valueField, i));
            document.add(new LongPoint(valueField, i));
            indexWriter.addDocument(document);
        }

        indexWriter.close();

        IndexReader indexReader = DirectoryReader.open(directory);
        IndexSearcher indexSearcher = newIndexSearcher(indexReader);

        MappedFieldType valueFieldType = new NumberFieldMapper.NumberFieldType(NumberFieldMapper.NumberType.LONG);
        valueFieldType.setName(valueField);
        valueFieldType.setHasDocValues(true);
        valueFieldType.setName(valueField);

        // Setup the composite agg
        TermsValuesSourceBuilder terms
                = new TermsValuesSourceBuilder("the_terms." + TermsAggregationBuilder.NAME).field(valueField);
        CompositeAggregationBuilder compositeBuilder = new CompositeAggregationBuilder(RollupIndexer.AGGREGATION_NAME,
                Collections.singletonList(terms));

        MetricConfig metricConfig = new MetricConfig.Builder().setField(valueField).setMetrics(Collections.singletonList("max")).build();
        metricConfig.toBuilders().forEach(compositeBuilder::subAggregation);

        Aggregator aggregator = createAggregator(compositeBuilder, indexSearcher, valueFieldType);
        aggregator.preCollection();
        indexSearcher.search(new MatchAllDocsQuery(), aggregator);
        aggregator.postCollection();
        CompositeAggregation composite = (CompositeAggregation) aggregator.buildAggregation(0L);
        indexReader.close();
        directory.close();

        List<IndexRequest> docs = IndexerUtils.processBuckets(composite, indexName, stats,
                ConfigTestHelpers.getGroupConfig().build(), "foo");

        assertThat(docs.size(), equalTo(numDocs));
        for (IndexRequest doc : docs) {
            Map<String, Object> map = doc.sourceAsMap();
            assertNotNull( map.get(valueField + "." + MaxAggregationBuilder.NAME + "." + RollupField.VALUE));
            assertThat(map.get("the_terms." + TermsAggregationBuilder.NAME + "." + RollupField.COUNT_FIELD), equalTo(1));
        }
    }

    public void testEmptyCounts() throws IOException {
        String indexName = randomAlphaOfLengthBetween(1, 10);
        RollupJobStats stats= new RollupJobStats(0, 0, 0, 0);

        String timestampField = "ts";
        String valueField = "the_avg";

        Directory directory = newDirectory();
        RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory);

        int numDocs = randomIntBetween(1,10);
        for (int i = 0; i < numDocs; i++) {
            Document document = new Document();
            long timestamp = new DateTime().minusDays(i).getMillis();
            document.add(new SortedNumericDocValuesField(timestampField, timestamp));
            document.add(new LongPoint(timestampField, timestamp));
            document.add(new SortedNumericDocValuesField(valueField, randomIntBetween(1,100)));
            indexWriter.addDocument(document);
        }

        indexWriter.close();

        IndexReader indexReader = DirectoryReader.open(directory);
        IndexSearcher indexSearcher = newIndexSearcher(indexReader);

        DateFieldMapper.Builder builder = new DateFieldMapper.Builder(timestampField);
        DateFieldMapper.DateFieldType timestampFieldType = builder.fieldType();
        timestampFieldType.setHasDocValues(true);
        timestampFieldType.setName(timestampField);

        MappedFieldType valueFieldType = new NumberFieldMapper.NumberFieldType(NumberFieldMapper.NumberType.LONG);
        valueFieldType.setName(valueField);
        valueFieldType.setHasDocValues(true);
        valueFieldType.setName(valueField);

        // Setup the composite agg
        DateHistogramValuesSourceBuilder dateHisto
                = new DateHistogramValuesSourceBuilder("the_histo." + DateHistogramAggregationBuilder.NAME)
                    .field(timestampField)
                    .dateHistogramInterval(new DateHistogramInterval("1d"));

        CompositeAggregationBuilder compositeBuilder = new CompositeAggregationBuilder(RollupIndexer.AGGREGATION_NAME,
                Collections.singletonList(dateHisto));

        MetricConfig metricConfig = new MetricConfig.Builder().setField("another_field").setMetrics(Arrays.asList("avg", "sum")).build();
        metricConfig.toBuilders().forEach(compositeBuilder::subAggregation);

        Aggregator aggregator = createAggregator(compositeBuilder, indexSearcher, timestampFieldType, valueFieldType);
        aggregator.preCollection();
        indexSearcher.search(new MatchAllDocsQuery(), aggregator);
        aggregator.postCollection();
        CompositeAggregation composite = (CompositeAggregation) aggregator.buildAggregation(0L);
        indexReader.close();
        directory.close();

        List<IndexRequest> docs = IndexerUtils.processBuckets(composite, indexName, stats,
                ConfigTestHelpers.getGroupConfig().build(), "foo");

        assertThat(docs.size(), equalTo(numDocs));
        for (IndexRequest doc : docs) {
            Map<String, Object> map = doc.sourceAsMap();
            assertNull(map.get("another_field." + AvgAggregationBuilder.NAME + "." + RollupField.VALUE));
            assertNotNull(map.get("another_field." + SumAggregationBuilder.NAME + "." + RollupField.VALUE));
            assertThat(map.get("the_histo." + DateHistogramAggregationBuilder.NAME + "." + RollupField.COUNT_FIELD), equalTo(1));
        }
    }

    public void testKeyOrdering() {
        CompositeAggregation composite = mock(CompositeAggregation.class);

        when(composite.getBuckets()).thenAnswer((Answer<List<CompositeAggregation.Bucket>>) invocationOnMock -> {
            List<CompositeAggregation.Bucket> foos = new ArrayList<>();

            CompositeAggregation.Bucket bucket = mock(CompositeAggregation.Bucket.class);
            LinkedHashMap<String, Object> keys = new LinkedHashMap<>(3);
            keys.put("foo.date_histogram", 123L);
            keys.put("bar.terms", "baz");
            keys.put("abc.histogram", 1.9);
            keys = shuffleMap(keys, Collections.emptySet());
            when(bucket.getKey()).thenReturn(keys);

            List<Aggregation> list = new ArrayList<>(3);
            InternalNumericMetricsAggregation.SingleValue mockAgg = mock(InternalNumericMetricsAggregation.SingleValue.class);
            when(mockAgg.getName()).thenReturn("123");
            list.add(mockAgg);

            InternalNumericMetricsAggregation.SingleValue mockAgg2 = mock(InternalNumericMetricsAggregation.SingleValue.class);
            when(mockAgg2.getName()).thenReturn("abc");
            list.add(mockAgg2);

            InternalNumericMetricsAggregation.SingleValue mockAgg3 = mock(InternalNumericMetricsAggregation.SingleValue.class);
            when(mockAgg3.getName()).thenReturn("yay");
            list.add(mockAgg3);

            Collections.shuffle(list, random());

            Aggregations aggs = new Aggregations(list);
            when(bucket.getAggregations()).thenReturn(aggs);
            when(bucket.getDocCount()).thenReturn(1L);

            foos.add(bucket);

            return foos;
        });

        GroupConfig.Builder groupConfig = ConfigTestHelpers.getGroupConfig();
        groupConfig.setHisto(ConfigTestHelpers.getHisto().setFields(Collections.singletonList("abc")).build());

        List<IndexRequest> docs = IndexerUtils.processBuckets(composite, "foo", new RollupJobStats(), groupConfig.build(), "foo");
        assertThat(docs.size(), equalTo(1));
        assertThat(docs.get(0).id(), equalTo("1237859798"));
    }

    interface Mock {
        List<? extends CompositeAggregation.Bucket> getBuckets();
    }
}
