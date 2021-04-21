/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.search.aggregations.bucket.terms;

import org.apache.lucene.document.BinaryDocValuesField;
import org.apache.lucene.document.Document;
import org.apache.lucene.document.Field;
import org.apache.lucene.document.InetAddressPoint;
import org.apache.lucene.document.LatLonDocValuesField;
import org.apache.lucene.document.LongPoint;
import org.apache.lucene.document.NumericDocValuesField;
import org.apache.lucene.document.SortedDocValuesField;
import org.apache.lucene.document.SortedNumericDocValuesField;
import org.apache.lucene.document.SortedSetDocValuesField;
import org.apache.lucene.document.StringField;
import org.apache.lucene.index.DirectoryReader;
import org.apache.lucene.index.IndexReader;
import org.apache.lucene.index.IndexableField;
import org.apache.lucene.index.RandomIndexWriter;
import org.apache.lucene.search.DocValuesFieldExistsQuery;
import org.apache.lucene.search.IndexSearcher;
import org.apache.lucene.search.MatchAllDocsQuery;
import org.apache.lucene.search.Query;
import org.apache.lucene.search.TermInSetQuery;
import org.apache.lucene.search.TotalHits;
import org.apache.lucene.store.Directory;
import org.apache.lucene.util.BytesRef;
import org.apache.lucene.util.NumericUtils;
import org.elasticsearch.common.CheckedConsumer;
import org.elasticsearch.common.breaker.CircuitBreaker;
import org.elasticsearch.common.geo.GeoPoint;
import org.elasticsearch.common.lucene.search.Queries;
import org.elasticsearch.common.network.InetAddresses;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.common.util.MockBigArrays;
import org.elasticsearch.common.util.MockPageCacheRecycler;
import org.elasticsearch.index.mapper.DateFieldMapper.DateFieldType;
import org.elasticsearch.index.mapper.GeoPointFieldMapper;
import org.elasticsearch.index.mapper.IdFieldMapper;
import org.elasticsearch.index.mapper.IpFieldMapper;
import org.elasticsearch.index.mapper.KeywordFieldMapper;
import org.elasticsearch.index.mapper.KeywordFieldMapper.KeywordField;
import org.elasticsearch.index.mapper.KeywordFieldMapper.KeywordFieldType;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.index.mapper.NestedPathFieldMapper;
import org.elasticsearch.index.mapper.NumberFieldMapper;
import org.elasticsearch.index.mapper.NumberFieldMapper.NumberFieldType;
import org.elasticsearch.index.mapper.ObjectMapper;
import org.elasticsearch.index.mapper.RangeFieldMapper;
import org.elasticsearch.index.mapper.RangeType;
import org.elasticsearch.index.mapper.SeqNoFieldMapper;
import org.elasticsearch.index.mapper.Uid;
import org.elasticsearch.index.query.MatchAllQueryBuilder;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.indices.breaker.NoneCircuitBreakerService;
import org.elasticsearch.script.MockScriptEngine;
import org.elasticsearch.script.Script;
import org.elasticsearch.script.ScriptEngine;
import org.elasticsearch.script.ScriptModule;
import org.elasticsearch.script.ScriptService;
import org.elasticsearch.script.ScriptType;
import org.elasticsearch.script.StringFieldScript;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.aggregations.AggregationBuilder;
import org.elasticsearch.search.aggregations.AggregationBuilders;
import org.elasticsearch.search.aggregations.AggregationExecutionException;
import org.elasticsearch.search.aggregations.Aggregator;
import org.elasticsearch.search.aggregations.AggregatorTestCase;
import org.elasticsearch.search.aggregations.BucketOrder;
import org.elasticsearch.search.aggregations.InternalAggregation;
import org.elasticsearch.search.aggregations.InternalMultiBucketAggregation;
import org.elasticsearch.search.aggregations.MultiBucketConsumerService;
import org.elasticsearch.search.aggregations.bucket.MultiBucketsAggregation;
import org.elasticsearch.search.aggregations.bucket.filter.Filter;
import org.elasticsearch.search.aggregations.bucket.filter.FilterAggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.filter.InternalFilter;
import org.elasticsearch.search.aggregations.bucket.global.GlobalAggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.global.InternalGlobal;
import org.elasticsearch.search.aggregations.bucket.histogram.DateHistogramAggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.histogram.DateHistogramInterval;
import org.elasticsearch.search.aggregations.bucket.histogram.InternalDateHistogram;
import org.elasticsearch.search.aggregations.bucket.nested.InternalNested;
import org.elasticsearch.search.aggregations.bucket.nested.NestedAggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.nested.NestedAggregatorTests;
import org.elasticsearch.search.aggregations.metrics.CardinalityAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.InternalMax;
import org.elasticsearch.search.aggregations.metrics.InternalTopHits;
import org.elasticsearch.search.aggregations.metrics.MaxAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.TopHitsAggregationBuilder;
import org.elasticsearch.search.aggregations.pipeline.BucketScriptPipelineAggregationBuilder;
import org.elasticsearch.search.aggregations.pipeline.PipelineAggregator.PipelineTree;
import org.elasticsearch.search.aggregations.support.AggregationContext;
import org.elasticsearch.search.aggregations.support.AggregationInspectionHelper;
import org.elasticsearch.search.aggregations.support.CoreValuesSourceType;
import org.elasticsearch.search.aggregations.support.ValueType;
import org.elasticsearch.search.aggregations.support.ValuesSourceType;
import org.elasticsearch.search.lookup.SearchLookup;
import org.elasticsearch.search.runtime.StringScriptFieldTermQuery;
import org.elasticsearch.search.sort.FieldSortBuilder;
import org.elasticsearch.search.sort.ScoreSortBuilder;
import org.elasticsearch.test.geo.RandomGeoGenerator;

import java.io.IOException;
import java.net.InetAddress;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.Comparator;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.function.BiFunction;
import java.util.function.Consumer;
import java.util.function.Function;

import static java.util.Collections.singleton;
import static java.util.stream.Collectors.toList;
import static org.elasticsearch.index.mapper.SeqNoFieldMapper.PRIMARY_TERM_NAME;
import static org.elasticsearch.search.aggregations.AggregationBuilders.terms;
import static org.elasticsearch.search.aggregations.PipelineAggregatorBuilders.bucketScript;
import static org.hamcrest.Matchers.closeTo;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;
import static org.hamcrest.Matchers.hasEntry;
import static org.hamcrest.Matchers.hasKey;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.not;

public class TermsAggregatorTests extends AggregatorTestCase {

    private boolean randomizeAggregatorImpl = true;

    // Constants for a script that returns a string
    private static final String STRING_SCRIPT_NAME = "string_script";
    private static final String STRING_SCRIPT_OUTPUT = "Orange";

    @Override
    protected ScriptService getMockScriptService() {
        Map<String, Function<Map<String, Object>, Object>> scripts = new HashMap<>();
        Map<String, Function<Map<String, Object>, Object>> nonDeterministicScripts = new HashMap<>();

        scripts.put(STRING_SCRIPT_NAME, value -> STRING_SCRIPT_OUTPUT);

        MockScriptEngine scriptEngine = new MockScriptEngine(MockScriptEngine.NAME,
            scripts,
            nonDeterministicScripts,
            Collections.emptyMap());
        Map<String, ScriptEngine> engines = Collections.singletonMap(scriptEngine.getType(), scriptEngine);

        return new ScriptService(Settings.EMPTY, engines, ScriptModule.CORE_CONTEXTS);
    }

    protected <A extends Aggregator> A createAggregator(AggregationBuilder aggregationBuilder, AggregationContext context)
        throws IOException {
        try {
            if (randomizeAggregatorImpl) {
                TermsAggregatorFactory.COLLECT_SEGMENT_ORDS = randomBoolean();
                TermsAggregatorFactory.REMAP_GLOBAL_ORDS = randomBoolean();
            }
            return super.createAggregator(aggregationBuilder, context);
        } finally {
            TermsAggregatorFactory.COLLECT_SEGMENT_ORDS = null;
            TermsAggregatorFactory.REMAP_GLOBAL_ORDS = null;
        }
    }

    @Override
    protected AggregationBuilder createAggBuilderForTypeTest(MappedFieldType fieldType, String fieldName) {
        return new TermsAggregationBuilder("foo").field(fieldName);
    }

    @Override
    protected List<ValuesSourceType> getSupportedValuesSourceTypes() {
        return List.of(CoreValuesSourceType.NUMERIC,
            CoreValuesSourceType.KEYWORD,
            CoreValuesSourceType.IP,
            CoreValuesSourceType.DATE,
            CoreValuesSourceType.BOOLEAN);
    }

    public void testUsesGlobalOrdinalsByDefault() throws Exception {
        randomizeAggregatorImpl = false;

        Directory directory = newDirectory();
        RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory);
        indexWriter.close();
        IndexReader indexReader = DirectoryReader.open(directory);
        // We do not use LuceneTestCase.newSearcher because we need a DirectoryReader
        IndexSearcher indexSearcher = new IndexSearcher(indexReader);

        TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("_name").userValueTypeHint(ValueType.STRING)
            .field("string");
        MappedFieldType fieldType = new KeywordFieldMapper.KeywordFieldType("string");

        TermsAggregator aggregator = createAggregator(aggregationBuilder, indexSearcher, fieldType);
        assertThat(aggregator, instanceOf(GlobalOrdinalsStringTermsAggregator.class));
        GlobalOrdinalsStringTermsAggregator globalAgg = (GlobalOrdinalsStringTermsAggregator) aggregator;
        assertThat(globalAgg.descriptCollectionStrategy(), equalTo("dense"));

        // Infers depth_first because the maxOrd is 0 which is less than the size
        aggregationBuilder
            .subAggregation(AggregationBuilders.cardinality("card").field("string"));
        aggregator = createAggregator(aggregationBuilder, indexSearcher, fieldType);
        assertThat(aggregator, instanceOf(GlobalOrdinalsStringTermsAggregator.class));
        globalAgg = (GlobalOrdinalsStringTermsAggregator) aggregator;
        assertThat(globalAgg.collectMode, equalTo(Aggregator.SubAggCollectionMode.DEPTH_FIRST));
        assertThat(globalAgg.descriptCollectionStrategy(), equalTo("remap using single bucket ords"));

        aggregationBuilder
            .collectMode(Aggregator.SubAggCollectionMode.DEPTH_FIRST);
        aggregator = createAggregator(aggregationBuilder, indexSearcher, fieldType);
        assertThat(aggregator, instanceOf(GlobalOrdinalsStringTermsAggregator.class));
        globalAgg = (GlobalOrdinalsStringTermsAggregator) aggregator;
        assertThat(globalAgg.collectMode, equalTo(Aggregator.SubAggCollectionMode.DEPTH_FIRST));
        assertThat(globalAgg.descriptCollectionStrategy(), equalTo("remap using single bucket ords"));

        aggregationBuilder
            .collectMode(Aggregator.SubAggCollectionMode.BREADTH_FIRST);
        aggregator = createAggregator(aggregationBuilder, indexSearcher, fieldType);
        assertThat(aggregator, instanceOf(GlobalOrdinalsStringTermsAggregator.class));
        globalAgg = (GlobalOrdinalsStringTermsAggregator) aggregator;
        assertThat(globalAgg.collectMode, equalTo(Aggregator.SubAggCollectionMode.BREADTH_FIRST));
        assertThat(globalAgg.descriptCollectionStrategy(), equalTo("dense"));

        aggregationBuilder
            .order(BucketOrder.aggregation("card", true));
        aggregator = createAggregator(aggregationBuilder, indexSearcher, fieldType);
        assertThat(aggregator, instanceOf(GlobalOrdinalsStringTermsAggregator.class));
        globalAgg = (GlobalOrdinalsStringTermsAggregator) aggregator;
        assertThat(globalAgg.descriptCollectionStrategy(), equalTo("remap using single bucket ords"));

        indexReader.close();
        directory.close();
    }

    public void testSimple() throws Exception {
        MappedFieldType fieldType = new KeywordFieldMapper.KeywordFieldType("string", randomBoolean(), true, Collections.emptyMap());
        TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("_name")
            .executionHint(randomFrom(TermsAggregatorFactory.ExecutionMode.values()).toString())
            .field("string")
            .order(BucketOrder.key(true));
        testCase(aggregationBuilder, new MatchAllDocsQuery(), iw -> {
            iw.addDocument(doc(fieldType, "a", "b"));
            iw.addDocument(doc(fieldType, "", "c", "a"));
            iw.addDocument(doc(fieldType, "b", "d"));
            iw.addDocument(doc(fieldType, ""));
        }, (InternalTerms<?, ?> result) -> {
            assertEquals(5, result.getBuckets().size());
            assertEquals("", result.getBuckets().get(0).getKeyAsString());
            assertEquals(2L, result.getBuckets().get(0).getDocCount());
            assertEquals("a", result.getBuckets().get(1).getKeyAsString());
            assertEquals(2L, result.getBuckets().get(1).getDocCount());
            assertEquals("b", result.getBuckets().get(2).getKeyAsString());
            assertEquals(2L, result.getBuckets().get(2).getDocCount());
            assertEquals("c", result.getBuckets().get(3).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(3).getDocCount());
            assertEquals("d", result.getBuckets().get(4).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(4).getDocCount());
            assertTrue(AggregationInspectionHelper.hasValue(result));
        }, fieldType);
    }

    public void testStringShardMinDocCount() throws IOException {
        MappedFieldType fieldType = new KeywordFieldMapper.KeywordFieldType("string", true, true, Collections.emptyMap());
        for (TermsAggregatorFactory.ExecutionMode executionMode : TermsAggregatorFactory.ExecutionMode.values()) {
            TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("_name")
                .field("string")
                .executionHint(executionMode.toString())
                .size(2)
                .minDocCount(2)
                .shardMinDocCount(2)
                .order(BucketOrder.key(true));
            testCase(aggregationBuilder, new MatchAllDocsQuery(), iw -> {
                // force single shard/segment
                iw.addDocuments(Arrays.asList(
                    doc(fieldType, "a", "b"),
                    doc(fieldType, "", "c", "d"),
                    doc(fieldType, "b", "d"),
                    doc(fieldType, "b")));
            }, (InternalTerms<?, ?> result) -> {
                assertEquals(2, result.getBuckets().size());
                assertEquals("b", result.getBuckets().get(0).getKeyAsString());
                assertEquals(3L, result.getBuckets().get(0).getDocCount());
                assertEquals("d", result.getBuckets().get(1).getKeyAsString());
                assertEquals(2L, result.getBuckets().get(1).getDocCount());
            }, fieldType);
        }
    }

    public void testManyTerms() throws Exception {
        MappedFieldType fieldType = new KeywordFieldMapper.KeywordFieldType("string", randomBoolean(), true, Collections.emptyMap());
        TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("_name")
            .executionHint(randomFrom(TermsAggregatorFactory.ExecutionMode.values()).toString())
            .field("string");
        testCase(aggregationBuilder, new MatchAllDocsQuery(), iw -> {
            /*
             * index all of the fields into a single segment so our
             * test gets accurate counts. We *could* set the shard size
             * very very high but we want to test the branch of the
             * aggregation building code that picks the top sorted aggs.
             */
            List<List<? extends IndexableField>> docs = new ArrayList<>();
            for (int i = 0; i < TermsAggregatorFactory.MAX_ORDS_TO_TRY_FILTERS - 200; i++) {
                String s = String.format(Locale.ROOT, "b%03d", i);
                docs.add(doc(fieldType, s));
                if (i % 100 == 7) {
                    docs.add(doc(fieldType, s));
                }
            }
            iw.addDocuments(docs);
        }, (StringTerms result) -> {
            assertThat(result.getBuckets().stream().map(StringTerms.Bucket::getKey).collect(toList()),
                equalTo(List.of("b007", "b107", "b207", "b307", "b407", "b507", "b607", "b707", "b000", "b001")));
        }, fieldType);
    }

    private List<IndexableField> doc(MappedFieldType ft, String... values) {
        List<IndexableField> doc = new ArrayList<IndexableField>();
        for (String v : values) {
            BytesRef bytes = new BytesRef(v);
            doc.add(new SortedSetDocValuesField(ft.name(), bytes));
            if (ft.isSearchable()) {
                doc.add(new KeywordField(ft.name(), bytes, KeywordFieldMapper.Defaults.FIELD_TYPE));
            }
        }
        return doc;
    }

    public void testStringIncludeExclude() throws Exception {
        MappedFieldType ft1 = new KeywordFieldMapper.KeywordFieldType("mv_field", randomBoolean(), true, Collections.emptyMap());
        MappedFieldType ft2 = new KeywordFieldMapper.KeywordFieldType("sv_field", randomBoolean(), true, Collections.emptyMap());
        CheckedConsumer<RandomIndexWriter, IOException> buildIndex = iw -> {
            iw.addDocument(doc(ft1, ft2, "val000", "val001", "val001"));
            iw.addDocument(doc(ft1, ft2, "val002", "val003", "val003"));
            iw.addDocument(doc(ft1, ft2, "val004", "val005", "val005"));
            iw.addDocument(doc(ft1, ft2, "val006", "val007", "val007"));
            iw.addDocument(doc(ft1, ft2, "val008", "val009", "val009"));
            iw.addDocument(doc(ft1, ft2, "val010", "val011", "val011"));
        };
        String executionHint = randomFrom(TermsAggregatorFactory.ExecutionMode.values()).toString();

        AggregationBuilder builder = new TermsAggregationBuilder("_name").executionHint(executionHint)
            .includeExclude(new IncludeExclude("val00.+", null))
            .field("mv_field")
            .size(12)
            .order(BucketOrder.key(true));
        testCase(builder, new MatchAllDocsQuery(), buildIndex, (StringTerms result) -> {
            assertEquals(10, result.getBuckets().size());
            assertEquals("val000", result.getBuckets().get(0).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(0).getDocCount());
            assertEquals("val001", result.getBuckets().get(1).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(1).getDocCount());
            assertEquals("val002", result.getBuckets().get(2).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(2).getDocCount());
            assertEquals("val003", result.getBuckets().get(3).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(3).getDocCount());
            assertEquals("val004", result.getBuckets().get(4).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(4).getDocCount());
            assertEquals("val005", result.getBuckets().get(5).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(5).getDocCount());
            assertEquals("val006", result.getBuckets().get(6).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(6).getDocCount());
            assertEquals("val007", result.getBuckets().get(7).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(7).getDocCount());
            assertEquals("val008", result.getBuckets().get(8).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(8).getDocCount());
            assertEquals("val009", result.getBuckets().get(9).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(9).getDocCount());
            assertTrue(AggregationInspectionHelper.hasValue(result));
        }, ft1, ft2);

        builder = new TermsAggregationBuilder("_name").executionHint(executionHint)
            .includeExclude(new IncludeExclude("val00.+", null))
            .field("sv_field")
            .order(BucketOrder.key(true));
        testCase(builder, new MatchAllDocsQuery(), buildIndex, (StringTerms result) -> {
            assertEquals(5, result.getBuckets().size());
            assertEquals("val001", result.getBuckets().get(0).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(0).getDocCount());
            assertEquals("val003", result.getBuckets().get(1).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(1).getDocCount());
            assertEquals("val005", result.getBuckets().get(2).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(2).getDocCount());
            assertEquals("val007", result.getBuckets().get(3).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(3).getDocCount());
            assertEquals("val009", result.getBuckets().get(4).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(4).getDocCount());
            assertTrue(AggregationInspectionHelper.hasValue(result));
        }, ft1, ft2);

        builder = new TermsAggregationBuilder("_name").executionHint(executionHint)
            .includeExclude(new IncludeExclude("val00.+", null))
            .field("sv_field")
            .order(BucketOrder.key(true));
        testCase(builder, new MatchAllDocsQuery(), buildIndex, (StringTerms result) -> {
            assertEquals(5, result.getBuckets().size());
            assertEquals("val001", result.getBuckets().get(0).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(0).getDocCount());
            assertEquals("val003", result.getBuckets().get(1).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(1).getDocCount());
            assertEquals("val005", result.getBuckets().get(2).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(2).getDocCount());
            assertEquals("val007", result.getBuckets().get(3).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(3).getDocCount());
            assertEquals("val009", result.getBuckets().get(4).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(4).getDocCount());
            assertTrue(AggregationInspectionHelper.hasValue(result));
        }, ft1, ft2);

        builder = new TermsAggregationBuilder("_name").executionHint(executionHint)
            .includeExclude(new IncludeExclude("val00.+", "(val000|val001)"))
            .field("mv_field")
            .order(BucketOrder.key(true));
        testCase(builder, new MatchAllDocsQuery(), buildIndex, (StringTerms result) -> {
            assertEquals(8, result.getBuckets().size());
            assertEquals("val002", result.getBuckets().get(0).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(0).getDocCount());
            assertEquals("val003", result.getBuckets().get(1).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(1).getDocCount());
            assertEquals("val004", result.getBuckets().get(2).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(2).getDocCount());
            assertEquals("val005", result.getBuckets().get(3).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(3).getDocCount());
            assertEquals("val006", result.getBuckets().get(4).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(4).getDocCount());
            assertEquals("val007", result.getBuckets().get(5).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(5).getDocCount());
            assertEquals("val008", result.getBuckets().get(6).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(6).getDocCount());
            assertEquals("val009", result.getBuckets().get(7).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(7).getDocCount());
            assertTrue(AggregationInspectionHelper.hasValue(result));
        }, ft1, ft2);

        builder = new TermsAggregationBuilder("_name").executionHint(executionHint)
            .includeExclude(new IncludeExclude(null, "val00.+"))
            .field("mv_field")
            .order(BucketOrder.key(true));
        testCase(builder, new MatchAllDocsQuery(), buildIndex, (StringTerms result) -> {
            assertEquals(2, result.getBuckets().size());
            assertEquals("val010", result.getBuckets().get(0).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(0).getDocCount());
            assertEquals("val011", result.getBuckets().get(1).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(1).getDocCount());
            assertTrue(AggregationInspectionHelper.hasValue(result));
        }, ft1, ft2);

        builder = new TermsAggregationBuilder("_name").executionHint(executionHint)
            .includeExclude(new IncludeExclude(new String[] { "val000", "val010" }, null))
            .field("mv_field")
            .order(BucketOrder.key(true));
        testCase(builder, new MatchAllDocsQuery(), buildIndex, (StringTerms result) -> {
            assertEquals(2, result.getBuckets().size());
            assertEquals("val000", result.getBuckets().get(0).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(0).getDocCount());
            assertEquals("val010", result.getBuckets().get(1).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(1).getDocCount());
            assertTrue(AggregationInspectionHelper.hasValue(result));
        }, ft1, ft2);

        builder = new TermsAggregationBuilder("_name").executionHint(executionHint)
            .includeExclude(
                new IncludeExclude(
                    null,
                    new String[] { "val001", "val002", "val003", "val004", "val005", "val006", "val007", "val008", "val009", "val011" }
                )
            )
            .field("mv_field")
            .order(BucketOrder.key(true));
        testCase(builder, new MatchAllDocsQuery(), buildIndex, (StringTerms result) -> {
            assertEquals(2, result.getBuckets().size());
            assertEquals("val000", result.getBuckets().get(0).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(0).getDocCount());
            assertEquals("val010", result.getBuckets().get(1).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(1).getDocCount());
            assertTrue(AggregationInspectionHelper.hasValue(result));
        }, ft1, ft2);

        builder = new TermsAggregationBuilder("_name").executionHint(executionHint)
            .includeExclude(
                new IncludeExclude(
                    "val00.+",
                    null,
                    null,
                    new String[] { "val001", "val002", "val003", "val004", "val005", "val006", "val007", "val008" }
                )
            )
            .field("mv_field")
            .order(BucketOrder.key(true));
        testCase(builder, new MatchAllDocsQuery(), buildIndex, (StringTerms result) -> {
            assertEquals(2, result.getBuckets().size());
            assertEquals("val000", result.getBuckets().get(0).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(0).getDocCount());
            assertEquals("val009", result.getBuckets().get(1).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(1).getDocCount());
            assertTrue(AggregationInspectionHelper.hasValue(result));
        }, ft1, ft2);

        builder = new TermsAggregationBuilder("_name").executionHint(executionHint)
            .includeExclude(new IncludeExclude(null, "val01.+", new String[] { "val001", "val002", "val010" }, null))
            .field("mv_field")
            .order(BucketOrder.key(true));
        testCase(builder, new MatchAllDocsQuery(), buildIndex, (StringTerms result) -> {
            assertEquals(2, result.getBuckets().size());
            assertEquals("val001", result.getBuckets().get(0).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(0).getDocCount());
            assertEquals("val002", result.getBuckets().get(1).getKeyAsString());
            assertEquals(1L, result.getBuckets().get(1).getDocCount());
            assertTrue(AggregationInspectionHelper.hasValue(result));
        }, ft1, ft2);
    }

    private List<IndexableField> doc(MappedFieldType ft1, MappedFieldType ft2, String f1v1, String f1v2, String f2v) {
        List<IndexableField> doc = new ArrayList<IndexableField>();
        doc.add(new SortedSetDocValuesField(ft1.name(), new BytesRef(f1v1)));
        doc.add(new SortedSetDocValuesField(ft1.name(), new BytesRef(f1v2)));
        if (ft1.isSearchable()) {
            doc.add(new KeywordField(ft1.name(), new BytesRef(f1v1), KeywordFieldMapper.Defaults.FIELD_TYPE));
            doc.add(new KeywordField(ft1.name(), new BytesRef(f1v2), KeywordFieldMapper.Defaults.FIELD_TYPE));
        }
        doc.add(new SortedDocValuesField(ft2.name(), new BytesRef(f2v)));
        if (ft2.isSearchable()) {
            doc.add(new KeywordField(ft2.name(), new BytesRef(f2v), KeywordFieldMapper.Defaults.FIELD_TYPE));
        }
        return doc;
    }

    public void testNumericIncludeExclude() throws Exception {
        try (Directory directory = newDirectory()) {
            try (RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory)) {
                Document document = new Document();
                document.add(new NumericDocValuesField("long_field", 0));
                document.add(new NumericDocValuesField("double_field", Double.doubleToRawLongBits(0.0)));
                indexWriter.addDocument(document);
                document = new Document();
                document.add(new NumericDocValuesField("long_field", 1));
                document.add(new NumericDocValuesField("double_field", Double.doubleToRawLongBits(1.0)));
                indexWriter.addDocument(document);
                document = new Document();
                document.add(new NumericDocValuesField("long_field", 2));
                document.add(new NumericDocValuesField("double_field", Double.doubleToRawLongBits(2.0)));
                indexWriter.addDocument(document);
                document = new Document();
                document.add(new NumericDocValuesField("long_field", 3));
                document.add(new NumericDocValuesField("double_field", Double.doubleToRawLongBits(3.0)));
                indexWriter.addDocument(document);
                document = new Document();
                document.add(new NumericDocValuesField("long_field", 4));
                document.add(new NumericDocValuesField("double_field", Double.doubleToRawLongBits(4.0)));
                indexWriter.addDocument(document);
                document = new Document();
                document.add(new NumericDocValuesField("long_field", 5));
                document.add(new NumericDocValuesField("double_field", Double.doubleToRawLongBits(5.0)));
                indexWriter.addDocument(document);
                try (IndexReader indexReader = maybeWrapReaderEs(indexWriter.getReader())) {
                    IndexSearcher indexSearcher = newIndexSearcher(indexReader);
                    MappedFieldType fieldType
                        = new NumberFieldMapper.NumberFieldType("long_field", NumberFieldMapper.NumberType.LONG);

                    String executionHint = randomFrom(TermsAggregatorFactory.ExecutionMode.values()).toString();
                    TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("_name")
                        .userValueTypeHint(ValueType.LONG)
                        .executionHint(executionHint)
                        .includeExclude(new IncludeExclude(new long[]{0, 5}, null))
                        .field("long_field")
                        .order(BucketOrder.key(true));
                    AggregationContext context = createAggregationContext(indexSearcher, null, fieldType);
                    TermsAggregator aggregator = createAggregator(aggregationBuilder, context);
                    aggregator.preCollection();
                    indexSearcher.search(new MatchAllDocsQuery(), aggregator);
                    aggregator.postCollection();
                    Terms result = reduce(aggregator, context.bigArrays());
                    assertEquals(2, result.getBuckets().size());
                    assertEquals(0L, result.getBuckets().get(0).getKey());
                    assertEquals(1L, result.getBuckets().get(0).getDocCount());
                    assertEquals(5L, result.getBuckets().get(1).getKey());
                    assertEquals(1L, result.getBuckets().get(1).getDocCount());
                    assertTrue(AggregationInspectionHelper.hasValue((InternalTerms)result));

                    aggregationBuilder = new TermsAggregationBuilder("_name").userValueTypeHint(ValueType.LONG)
                        .executionHint(executionHint)
                        .includeExclude(new IncludeExclude(null, new long[]{0, 5}))
                        .field("long_field")
                        .order(BucketOrder.key(true));
                    context = createAggregationContext(indexSearcher, null, fieldType);
                    aggregator = createAggregator(aggregationBuilder, context);
                    aggregator.preCollection();
                    indexSearcher.search(new MatchAllDocsQuery(), aggregator);
                    aggregator.postCollection();
                    result = reduce(aggregator, context.bigArrays());
                    assertEquals(4, result.getBuckets().size());
                    assertEquals(1L, result.getBuckets().get(0).getKey());
                    assertEquals(1L, result.getBuckets().get(0).getDocCount());
                    assertEquals(2L, result.getBuckets().get(1).getKey());
                    assertEquals(1L, result.getBuckets().get(1).getDocCount());
                    assertEquals(3L, result.getBuckets().get(2).getKey());
                    assertEquals(1L, result.getBuckets().get(2).getDocCount());
                    assertEquals(4L, result.getBuckets().get(3).getKey());
                    assertEquals(1L, result.getBuckets().get(3).getDocCount());
                    assertTrue(AggregationInspectionHelper.hasValue((InternalTerms)result));

                    fieldType
                        = new NumberFieldMapper.NumberFieldType("double_field", NumberFieldMapper.NumberType.DOUBLE);
                    aggregationBuilder = new TermsAggregationBuilder("_name").userValueTypeHint(ValueType.DOUBLE)
                        .executionHint(executionHint)
                        .includeExclude(new IncludeExclude(new double[]{0.0, 5.0}, null))
                        .field("double_field")
                        .order(BucketOrder.key(true));
                    context = createAggregationContext(indexSearcher, null, fieldType);
                    aggregator = createAggregator(aggregationBuilder, context);
                    aggregator.preCollection();
                    indexSearcher.search(new MatchAllDocsQuery(), aggregator);
                    aggregator.postCollection();
                    result = reduce(aggregator, context.bigArrays());
                    assertEquals(2, result.getBuckets().size());
                    assertEquals(0.0, result.getBuckets().get(0).getKey());
                    assertEquals(1L, result.getBuckets().get(0).getDocCount());
                    assertEquals(5.0, result.getBuckets().get(1).getKey());
                    assertEquals(1L, result.getBuckets().get(1).getDocCount());
                    assertTrue(AggregationInspectionHelper.hasValue((InternalTerms)result));

                    aggregationBuilder = new TermsAggregationBuilder("_name").userValueTypeHint(ValueType.DOUBLE)
                        .executionHint(executionHint)
                        .includeExclude(new IncludeExclude(null, new double[]{0.0, 5.0}))
                        .field("double_field")
                        .order(BucketOrder.key(true));
                    context = createAggregationContext(indexSearcher, null, fieldType);
                    aggregator = createAggregator(aggregationBuilder, context);
                    aggregator.preCollection();
                    indexSearcher.search(new MatchAllDocsQuery(), aggregator);
                    aggregator.postCollection();
                    result = reduce(aggregator, context.bigArrays());
                    assertEquals(4, result.getBuckets().size());
                    assertEquals(1.0, result.getBuckets().get(0).getKey());
                    assertEquals(1L, result.getBuckets().get(0).getDocCount());
                    assertEquals(2.0, result.getBuckets().get(1).getKey());
                    assertEquals(1L, result.getBuckets().get(1).getDocCount());
                    assertEquals(3.0, result.getBuckets().get(2).getKey());
                    assertEquals(1L, result.getBuckets().get(2).getDocCount());
                    assertEquals(4.0, result.getBuckets().get(3).getKey());
                    assertEquals(1L, result.getBuckets().get(3).getDocCount());
                    assertTrue(AggregationInspectionHelper.hasValue((InternalTerms)result));
                }
            }
        }
    }

    public void testStringTermsAggregator() throws Exception {
        MappedFieldType fieldType = new KeywordFieldMapper.KeywordFieldType("field", randomBoolean(), true, Collections.emptyMap());
        BiFunction<String, Boolean, List<IndexableField>> luceneFieldFactory = (val, mv) -> {
            List<IndexableField> result = new ArrayList<>(2);
            if (mv) {
                result.add(new SortedSetDocValuesField("field", new BytesRef(val)));
            } else {
                result.add(new SortedDocValuesField("field", new BytesRef(val)));
            }
            if (fieldType.isSearchable()) {
                result.add(new KeywordField("field", new BytesRef(val), KeywordFieldMapper.Defaults.FIELD_TYPE));
            }
            return result;
        };
        termsAggregator(ValueType.STRING, fieldType, i -> Integer.toString(i),
            String::compareTo, luceneFieldFactory);
        termsAggregatorWithNestedMaxAgg(ValueType.STRING, fieldType, i -> Integer.toString(i), val -> luceneFieldFactory.apply(val, false));
    }

    public void testLongTermsAggregator() throws Exception {
        BiFunction<Long, Boolean, List<IndexableField>> luceneFieldFactory = (val, mv) -> {
            if (mv) {
                return List.of(new SortedNumericDocValuesField("field", val));
            } else {
                return List.of(new NumericDocValuesField("field", val));
            }
        };
        MappedFieldType fieldType
            = new NumberFieldMapper.NumberFieldType("field", NumberFieldMapper.NumberType.LONG);
        termsAggregator(ValueType.LONG, fieldType, Integer::longValue, Long::compareTo, luceneFieldFactory);
        termsAggregatorWithNestedMaxAgg(ValueType.LONG, fieldType, Integer::longValue, val -> luceneFieldFactory.apply(val, false));
    }

    public void testDoubleTermsAggregator() throws Exception {
        BiFunction<Double, Boolean, List<IndexableField>> luceneFieldFactory = (val, mv) -> {
            if (mv) {
                return List.of(new SortedNumericDocValuesField("field", Double.doubleToRawLongBits(val)));
            } else {
                return List.of(new NumericDocValuesField("field", Double.doubleToRawLongBits(val)));
            }
        };
        MappedFieldType fieldType
            = new NumberFieldMapper.NumberFieldType("field", NumberFieldMapper.NumberType.DOUBLE);
        termsAggregator(ValueType.DOUBLE, fieldType, Integer::doubleValue, Double::compareTo, luceneFieldFactory);
        termsAggregatorWithNestedMaxAgg(ValueType.DOUBLE, fieldType, Integer::doubleValue,
            val -> luceneFieldFactory.apply(val, false));
    }

    public void testIpTermsAggregator() throws Exception {
        IpFieldMapper.IpFieldType fieldType = new IpFieldMapper.IpFieldType("field");
        BiFunction<InetAddress, Boolean, List<IndexableField>> luceneFieldFactory = (val, mv) -> {
            List<IndexableField> result = new ArrayList<>(2);
            if (mv) {
                result.add(new SortedSetDocValuesField("field", new BytesRef(InetAddressPoint.encode(val))));
            } else {
                result.add(new SortedDocValuesField("field", new BytesRef(InetAddressPoint.encode(val))));
            }
            if (fieldType.isSearchable()) {
                result.add(new InetAddressPoint("field", val));
            }
            return result;
        };
        InetAddress[] base = new InetAddress[] { InetAddresses.forString("192.168.0.0") };
        Comparator<InetAddress> comparator = (o1, o2) -> {
            BytesRef b1 = new BytesRef(InetAddressPoint.encode(o1));
            BytesRef b2 = new BytesRef(InetAddressPoint.encode(o2));
            return b1.compareTo(b2);
        };
        termsAggregator(ValueType.IP, fieldType, i -> base[0] = InetAddressPoint.nextUp(base[0]), comparator, luceneFieldFactory);
    }

    private <T> void termsAggregator(ValueType valueType, MappedFieldType fieldType,
                                     Function<Integer, T> valueFactory, Comparator<T> keyComparator,
                                     BiFunction<T, Boolean, List<IndexableField>> luceneFieldFactory) throws Exception {
        final Map<T, Integer> counts = new HashMap<>();
        final Map<T, Integer> filteredCounts = new HashMap<>();
        int numTerms = scaledRandomIntBetween(8, 128);
        for (int i = 0; i < numTerms; i++) {
            int numDocs = scaledRandomIntBetween(2, 32);
            T key = valueFactory.apply(i);
            counts.put(key, numDocs);
            filteredCounts.put(key, 0);
        }

        try (Directory directory = newDirectory()) {
            boolean multiValued = randomBoolean();
            try (RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory)) {
                if (multiValued == false) {
                    for (Map.Entry<T, Integer> entry : counts.entrySet()) {
                        for (int i = 0; i < entry.getValue(); i++) {
                            Document document = new Document();
                            luceneFieldFactory.apply(entry.getKey(), false).forEach(document::add);
                            if (randomBoolean()) {
                                document.add(new StringField("include", "yes", Field.Store.NO));
                                filteredCounts.computeIfPresent(entry.getKey(), (key, integer) -> integer + 1);
                            }
                            indexWriter.addDocument(document);
                        }
                    }
                } else {
                    Iterator<Map.Entry<T, Integer>> iterator = counts.entrySet().iterator();
                    while (iterator.hasNext()) {
                        Map.Entry<T, Integer> entry1 = iterator.next();
                        Map.Entry<T, Integer> entry2 = null;
                        if (randomBoolean() && iterator.hasNext()) {
                            entry2 = iterator.next();
                            if (entry1.getValue().compareTo(entry2.getValue()) < 0) {
                                Map.Entry<T, Integer> temp = entry1;
                                entry1 = entry2;
                                entry2 = temp;
                            }
                        }

                        for (int i = 0; i < entry1.getValue(); i++) {
                            Document document = new Document();
                            luceneFieldFactory.apply(entry1.getKey(), true).forEach(document::add);
                            if (entry2 != null && i < entry2.getValue()) {
                                luceneFieldFactory.apply(entry2.getKey(), true).forEach(document::add);
                            }
                            indexWriter.addDocument(document);
                        }
                    }
                }
                try (IndexReader indexReader = maybeWrapReaderEs(indexWriter.getReader())) {
                    boolean order = randomBoolean();
                    List<Map.Entry<T, Integer>> expectedBuckets = new ArrayList<>();
                    expectedBuckets.addAll(counts.entrySet());
                    BucketOrder bucketOrder;
                    Comparator<Map.Entry<T, Integer>> comparator;
                    if (randomBoolean()) {
                        bucketOrder = BucketOrder.key(order);
                        comparator = Comparator.comparing(Map.Entry::getKey, keyComparator);
                    } else {
                        // if order by count then we need to use compound so that we can also sort by key as tie breaker:
                        bucketOrder = BucketOrder.compound(BucketOrder.count(order), BucketOrder.key(order));
                        comparator = Comparator.comparing(Map.Entry::getValue);
                        comparator = comparator.thenComparing(Comparator.comparing(Map.Entry::getKey, keyComparator));
                    }
                    if (order == false) {
                        comparator = comparator.reversed();
                    }
                    expectedBuckets.sort(comparator);
                    int size = randomIntBetween(1, counts.size());

                    String executionHint = randomFrom(TermsAggregatorFactory.ExecutionMode.values()).toString();
                    logger.info("bucket_order={} size={} execution_hint={}", bucketOrder, size, executionHint);
                    IndexSearcher indexSearcher = newIndexSearcher(indexReader);
                    AggregationBuilder aggregationBuilder = new TermsAggregationBuilder("_name")
                        .userValueTypeHint(valueType)
                        .executionHint(executionHint)
                        .size(size)
                        .shardSize(size)
                        .field("field")
                        .order(bucketOrder);

                    AggregationContext context = createAggregationContext(indexSearcher, new MatchAllDocsQuery(), fieldType);
                    Aggregator aggregator = createAggregator(aggregationBuilder, context);
                    aggregator.preCollection();
                    indexSearcher.search(new MatchAllDocsQuery(), aggregator);
                    aggregator.postCollection();
                    Terms result = reduce(aggregator, context.bigArrays());
                    assertEquals(size, result.getBuckets().size());
                    for (int i = 0; i < size; i++) {
                        Map.Entry<T, Integer>  expected = expectedBuckets.get(i);
                        Terms.Bucket actual = result.getBuckets().get(i);
                        if (valueType == ValueType.IP) {
                            assertEquals(String.valueOf(expected.getKey()).substring(1), actual.getKey());
                        } else {
                            assertEquals(expected.getKey(), actual.getKey());
                        }
                        assertEquals(expected.getValue().longValue(), actual.getDocCount());
                    }

                    if (multiValued == false) {
                        MappedFieldType filterFieldType = new KeywordFieldMapper.KeywordFieldType("include");
                        aggregationBuilder = new FilterAggregationBuilder("_name1", QueryBuilders.termQuery("include", "yes"));
                        aggregationBuilder.subAggregation(new TermsAggregationBuilder("_name2")
                            .userValueTypeHint(valueType)
                            .executionHint(executionHint)
                            .size(numTerms)
                            .collectMode(randomFrom(Aggregator.SubAggCollectionMode.values()))
                            .field("field"));
                        context = createAggregationContext(indexSearcher, null, fieldType, filterFieldType);
                        aggregator = createAggregator(aggregationBuilder, context);
                        aggregator.preCollection();
                        indexSearcher.search(new MatchAllDocsQuery(), aggregator);
                        aggregator.postCollection();
                        result = ((Filter) reduce(aggregator, context.bigArrays())).getAggregations().get("_name2");
                        int expectedFilteredCounts = 0;
                        for (Integer count : filteredCounts.values()) {
                            if (count > 0) {
                                expectedFilteredCounts++;
                            }
                        }
                        assertEquals(expectedFilteredCounts, result.getBuckets().size());
                        for (Terms.Bucket actual : result.getBuckets()) {
                            Integer expectedCount;
                            if (valueType == ValueType.IP) {
                                expectedCount = filteredCounts.get(InetAddresses.forString((String)actual.getKey()));
                            } else {
                                expectedCount = filteredCounts.get(actual.getKey());
                            }
                            assertEquals(expectedCount.longValue(), actual.getDocCount());
                        }
                    }
                }
            }
        }
    }

    private <T> void termsAggregatorWithNestedMaxAgg(ValueType valueType, MappedFieldType fieldType,
                                     Function<Integer, T> valueFactory,
                                     Function<T, List<IndexableField>> luceneFieldFactory) throws Exception {
        final Map<T, Long> counts = new HashMap<>();
        int numTerms = scaledRandomIntBetween(8, 128);
        for (int i = 0; i < numTerms; i++) {
            counts.put(valueFactory.apply(i), randomLong());
        }

        try (Directory directory = newDirectory()) {
            try (RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory)) {
                for (Map.Entry<T, Long> entry : counts.entrySet()) {
                    List<IndexableField> document = new ArrayList<>();
                    document.addAll(luceneFieldFactory.apply(entry.getKey()));
                    document.add(new NumericDocValuesField("value", entry.getValue()));
                    indexWriter.addDocument(document);
                }
                try (IndexReader indexReader = maybeWrapReaderEs(indexWriter.getReader())) {
                    boolean order = randomBoolean();
                    List<Map.Entry<T, Long>> expectedBuckets = new ArrayList<>();
                    expectedBuckets.addAll(counts.entrySet());
                    BucketOrder bucketOrder = BucketOrder.aggregation("_max", order);
                    Comparator<Map.Entry<T, Long>> comparator = Comparator.comparing(Map.Entry::getValue, Long::compareTo);
                    if (order == false) {
                        comparator = comparator.reversed();
                    }
                    expectedBuckets.sort(comparator);
                    int size = randomIntBetween(1, counts.size());

                    String executionHint = randomFrom(TermsAggregatorFactory.ExecutionMode.values()).toString();
                    Aggregator.SubAggCollectionMode collectionMode = randomFrom(Aggregator.SubAggCollectionMode.values());
                    logger.info("bucket_order={} size={} execution_hint={}, collect_mode={}",
                        bucketOrder, size, executionHint, collectionMode);
                    IndexSearcher indexSearcher = newIndexSearcher(indexReader);
                    AggregationBuilder aggregationBuilder = new TermsAggregationBuilder("_name")
                        .userValueTypeHint(valueType)
                        .executionHint(executionHint)
                        .collectMode(collectionMode)
                        .size(size)
                        .shardSize(size)
                        .field("field")
                        .order(bucketOrder)
                        .subAggregation(AggregationBuilders.max("_max").field("value"));

                    MappedFieldType fieldType2
                        = new NumberFieldMapper.NumberFieldType("value", NumberFieldMapper.NumberType.LONG);
                    AggregationContext context = createAggregationContext(indexSearcher, new MatchAllDocsQuery(), fieldType, fieldType2);
                    Aggregator aggregator = createAggregator(aggregationBuilder, context);
                    aggregator.preCollection();
                    indexSearcher.search(new MatchAllDocsQuery(), aggregator);
                    aggregator.postCollection();
                    Terms result = reduce(aggregator, context.bigArrays());
                    assertEquals(size, result.getBuckets().size());
                    for (int i = 0; i < size; i++) {
                        Map.Entry<T, Long>  expected = expectedBuckets.get(i);
                        Terms.Bucket actual = result.getBuckets().get(i);
                        assertEquals(expected.getKey(), actual.getKey());
                    }
                }
            }
        }
    }

    public void testEmpty() throws Exception {
        try (Directory directory = newDirectory()) {
            try (RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory)) {
                MappedFieldType fieldType1 = new KeywordFieldMapper.KeywordFieldType("string");
                MappedFieldType fieldType2
                    = new NumberFieldMapper.NumberFieldType("long", NumberFieldMapper.NumberType.LONG);
                MappedFieldType fieldType3
                    = new NumberFieldMapper.NumberFieldType("double", NumberFieldMapper.NumberType.DOUBLE);
                try (IndexReader indexReader = maybeWrapReaderEs(indexWriter.getReader())) {
                    IndexSearcher indexSearcher = newIndexSearcher(indexReader);
                    TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("_name")
                        .userValueTypeHint(ValueType.STRING)
                        .field("string");
                    AggregationContext context = createAggregationContext(indexSearcher, null, fieldType1);
                    Aggregator aggregator = createAggregator(aggregationBuilder, context);
                    aggregator.preCollection();
                    indexSearcher.search(new MatchAllDocsQuery(), aggregator);
                    aggregator.postCollection();
                    Terms result = reduce(aggregator, context.bigArrays());
                    assertEquals("_name", result.getName());
                    assertEquals(0, result.getBuckets().size());

                    aggregationBuilder = new TermsAggregationBuilder("_name").userValueTypeHint(ValueType.LONG).field("long");
                    context = createAggregationContext(indexSearcher, null, fieldType2);
                    aggregator = createAggregator(aggregationBuilder, context);
                    aggregator.preCollection();
                    indexSearcher.search(new MatchAllDocsQuery(), aggregator);
                    aggregator.postCollection();
                    result = reduce(aggregator, context.bigArrays());
                    assertEquals("_name", result.getName());
                    assertEquals(0, result.getBuckets().size());

                    aggregationBuilder = new TermsAggregationBuilder("_name").userValueTypeHint(ValueType.DOUBLE).field("double");
                    context = createAggregationContext(indexSearcher, null, fieldType3);
                    aggregator = createAggregator(aggregationBuilder, context);
                    aggregator.preCollection();
                    indexSearcher.search(new MatchAllDocsQuery(), aggregator);
                    aggregator.postCollection();
                    result = reduce(aggregator, context.bigArrays());
                    assertEquals("_name", result.getName());
                    assertEquals(0, result.getBuckets().size());
                }
            }
        }
    }

    public void testUnmapped() throws Exception {
        try (Directory directory = newDirectory()) {
            try (RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory)) {
                try (IndexReader indexReader = maybeWrapReaderEs(indexWriter.getReader())) {
                    IndexSearcher indexSearcher = newIndexSearcher(indexReader);
                    ValueType[] valueTypes = new ValueType[]{ValueType.STRING, ValueType.LONG, ValueType.DOUBLE};
                    String[] fieldNames = new String[]{"string", "long", "double"};
                    for (int i = 0; i < fieldNames.length; i++) {
                        TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("_name")
                            .userValueTypeHint(valueTypes[i])
                            .field(fieldNames[i]);
                        AggregationContext context = createAggregationContext(indexSearcher, null);
                        Aggregator aggregator = createAggregator(aggregationBuilder, context);
                        aggregator.preCollection();
                        indexSearcher.search(new MatchAllDocsQuery(), aggregator);
                        aggregator.postCollection();
                        Terms result = reduce(aggregator, context.bigArrays());
                        assertEquals("_name", result.getName());
                        assertEquals(0, result.getBuckets().size());
                        assertFalse(AggregationInspectionHelper.hasValue((InternalTerms)result));
                    }
                }
            }
        }
    }

    public void testUnmappedWithMissing() throws Exception {
        try (Directory directory = newDirectory()) {
            try (RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory)) {

                Document document = new Document();
                document.add(new NumericDocValuesField("unrelated_value", 100));
                indexWriter.addDocument(document);

                try (IndexReader indexReader = maybeWrapReaderEs(indexWriter.getReader())) {

                    MappedFieldType fieldType1 = new KeywordFieldMapper.KeywordFieldType("unrelated_value");

                    IndexSearcher indexSearcher = newIndexSearcher(indexReader);
                    ValueType[] valueTypes = new ValueType[]{ValueType.STRING, ValueType.LONG, ValueType.DOUBLE};
                    String[] fieldNames = new String[]{"string", "long", "double"};
                    Object[] missingValues = new Object[]{"abc", 19L, 19.2};


                    for (int i = 0; i < fieldNames.length; i++) {
                        TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("_name")
                            .userValueTypeHint(valueTypes[i])
                            .field(fieldNames[i]).missing(missingValues[i]);
                        AggregationContext context = createAggregationContext(indexSearcher, null, fieldType1);
                        Aggregator aggregator = createAggregator(aggregationBuilder, context);
                        aggregator.preCollection();
                        indexSearcher.search(new MatchAllDocsQuery(), aggregator);
                        aggregator.postCollection();
                        Terms result = reduce(aggregator, context.bigArrays());
                        assertEquals("_name", result.getName());
                        assertEquals(1, result.getBuckets().size());
                        assertEquals(missingValues[i], result.getBuckets().get(0).getKey());
                        assertEquals(1, result.getBuckets().get(0).getDocCount());
                    }
                }
            }
        }
    }

    public void testRangeField() throws Exception {
        try (Directory directory = newDirectory()) {
            double start = randomDouble();
            double end = randomDoubleBetween(Math.nextUp(start), Double.MAX_VALUE, false);
            RangeType rangeType = RangeType.DOUBLE;
            final RangeFieldMapper.Range range = new RangeFieldMapper.Range(rangeType, start, end, true, true);
            final String fieldName = "field";
            final BinaryDocValuesField field = new BinaryDocValuesField(fieldName, rangeType.encodeRanges(Collections.singleton(range)));
            try (RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory)) {
                Document document = new Document();
                document.add(field);
                indexWriter.addDocument(document);
                try (IndexReader indexReader = maybeWrapReaderEs(indexWriter.getReader())) {
                    MappedFieldType fieldType = new RangeFieldMapper.RangeFieldType(fieldName, rangeType);
                    IndexSearcher indexSearcher = newIndexSearcher(indexReader);
                    TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("_name") .field(fieldName);
                    expectThrows(IllegalArgumentException.class, () -> {
                        createAggregator(aggregationBuilder, indexSearcher, fieldType);
                    });
                }
            }
        }
    }

    public void testGeoPointField() throws Exception {
        try (Directory directory = newDirectory()) {
            GeoPoint point = RandomGeoGenerator.randomPoint(random());
            final String field = "field";
            try (RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory)) {
                Document document = new Document();
                document.add(new LatLonDocValuesField(field, point.getLat(), point.getLon()));
                indexWriter.addDocument(document);
                try (IndexReader indexReader = maybeWrapReaderEs(indexWriter.getReader())) {
                    MappedFieldType fieldType = new GeoPointFieldMapper.GeoPointFieldType("field");
                    IndexSearcher indexSearcher = newIndexSearcher(indexReader);
                    TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("_name") .field(field);
                    expectThrows(IllegalArgumentException.class, () -> {
                        createAggregator(aggregationBuilder, indexSearcher, fieldType);
                    });
                }
            }
        }
    }

    public void testIpField() throws Exception {
        MappedFieldType fieldType
            = new IpFieldMapper.IpFieldType("field", randomBoolean(), false, true, null, null, Collections.emptyMap());
        testCase(new TermsAggregationBuilder("_name").field("field"), new MatchAllDocsQuery(), iw -> {
            Document document = new Document();
            InetAddress point = InetAddresses.forString("192.168.100.42");
            document.add(new SortedSetDocValuesField("field", new BytesRef(InetAddressPoint.encode(point))));
            if (fieldType.isSearchable()) {
                document.add(new InetAddressPoint("field", point));
            }
            iw.addDocument(document);
        }, (StringTerms result) -> {
            assertEquals("_name", result.getName());
            assertEquals(1, result.getBuckets().size());
            assertEquals("192.168.100.42", result.getBuckets().get(0).getKey());
            assertEquals(1, result.getBuckets().get(0).getDocCount());
        }, fieldType);
    }

    public void testNestedTermsAgg() throws Exception {
        MappedFieldType fieldType1 = new KeywordFieldMapper.KeywordFieldType("field1", randomBoolean(), true, Collections.emptyMap());
        MappedFieldType fieldType2 = new KeywordFieldMapper.KeywordFieldType("field2", randomBoolean(), true, Collections.emptyMap());
        try (Directory directory = newDirectory()) {
            try (RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory)) {
                List<IndexableField> document = new ArrayList<>();
                document.addAll(doc(fieldType1, "a"));
                document.addAll(doc(fieldType2, "b"));
                indexWriter.addDocument(document);
                document = new ArrayList<>();
                document.addAll(doc(fieldType1, "c"));
                document.addAll(doc(fieldType2, "d"));
                indexWriter.addDocument(document);
                document = new ArrayList<>();
                document.addAll(doc(fieldType1, "e"));
                document.addAll(doc(fieldType2, "f"));
                indexWriter.addDocument(document);
                try (IndexReader indexReader = maybeWrapReaderEs(indexWriter.getReader())) {
                    IndexSearcher indexSearcher = newIndexSearcher(indexReader);
                    String executionHint = randomFrom(TermsAggregatorFactory.ExecutionMode.values()).toString();
                    Aggregator.SubAggCollectionMode collectionMode = randomFrom(Aggregator.SubAggCollectionMode.values());
                    TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("_name1")
                        .userValueTypeHint(ValueType.STRING)
                        .executionHint(executionHint)
                        .collectMode(collectionMode)
                        .field("field1")
                        .order(BucketOrder.key(true))
                        .subAggregation(new TermsAggregationBuilder("_name2").userValueTypeHint(ValueType.STRING)
                            .executionHint(executionHint)
                            .collectMode(collectionMode)
                            .field("field2")
                            .order(BucketOrder.key(true))
                        );
                    AggregationContext context = createAggregationContext(indexSearcher, new MatchAllDocsQuery(), fieldType1, fieldType2);
                    Aggregator aggregator = createAggregator(aggregationBuilder, context);
                    aggregator.preCollection();
                    indexSearcher.search(new MatchAllDocsQuery(), aggregator);
                    aggregator.postCollection();
                    Terms result = reduce(aggregator, context.bigArrays());
                    assertEquals(3, result.getBuckets().size());
                    assertEquals("a", result.getBuckets().get(0).getKeyAsString());
                    assertEquals(1L, result.getBuckets().get(0).getDocCount());
                    Terms.Bucket nestedBucket = ((Terms) result.getBuckets().get(0).getAggregations().get("_name2")).getBuckets().get(0);
                    assertEquals("b", nestedBucket.getKeyAsString());
                    assertEquals("c", result.getBuckets().get(1).getKeyAsString());
                    assertEquals(1L, result.getBuckets().get(1).getDocCount());
                    nestedBucket = ((Terms) result.getBuckets().get(1).getAggregations().get("_name2")).getBuckets().get(0);
                    assertEquals("d", nestedBucket.getKeyAsString());
                    assertEquals("e", result.getBuckets().get(2).getKeyAsString());
                    assertEquals(1L, result.getBuckets().get(2).getDocCount());
                    nestedBucket = ((Terms) result.getBuckets().get(2).getAggregations().get("_name2")).getBuckets().get(0);
                    assertEquals("f", nestedBucket.getKeyAsString());
                }
            }
        }
    }

    public void testMixLongAndDouble() throws Exception {
        for (TermsAggregatorFactory.ExecutionMode executionMode : TermsAggregatorFactory.ExecutionMode.values()) {
            TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("_name").userValueTypeHint(ValueType.LONG)
                .executionHint(executionMode.toString())
                .field("number")
                .order(BucketOrder.key(true));
            List<InternalAggregation> aggs = new ArrayList<> ();
            int numLongs = randomIntBetween(1, 3);
            for (int i = 0; i < numLongs; i++) {
                final Directory dir;
                try (IndexReader reader = createIndexWithLongs()) {
                    dir = ((DirectoryReader) reader).directory();
                    IndexSearcher searcher = new IndexSearcher(reader);
                    MappedFieldType fieldType =
                        new NumberFieldMapper.NumberFieldType("number", NumberFieldMapper.NumberType.LONG);
                    aggs.add(buildInternalAggregation(aggregationBuilder, fieldType, searcher));
                }
                dir.close();
            }
            int numDoubles = randomIntBetween(1, 3);
            for (int i = 0; i < numDoubles; i++) {
                final Directory dir;
                try (IndexReader reader = createIndexWithDoubles()) {
                    dir = ((DirectoryReader) reader).directory();
                    IndexSearcher searcher = new IndexSearcher(reader);
                    MappedFieldType fieldType =
                        new NumberFieldMapper.NumberFieldType("number", NumberFieldMapper.NumberType.DOUBLE);
                    aggs.add(buildInternalAggregation(aggregationBuilder, fieldType, searcher));
                }
                dir.close();
            }
            InternalAggregation.ReduceContext ctx = InternalAggregation.ReduceContext.forFinalReduction(
                    new MockBigArrays(new MockPageCacheRecycler(Settings.EMPTY), new NoneCircuitBreakerService()),
                    null, b -> {}, PipelineTree.EMPTY);
            for (InternalAggregation internalAgg : aggs) {
                InternalAggregation mergedAggs = internalAgg.reduce(aggs, ctx);
                assertTrue(mergedAggs instanceof DoubleTerms);
                long expected = numLongs + numDoubles;
                List<? extends Terms.Bucket> buckets = ((DoubleTerms) mergedAggs).getBuckets();
                assertEquals(4, buckets.size());
                assertEquals("1.0", buckets.get(0).getKeyAsString());
                assertEquals(expected, buckets.get(0).getDocCount());
                assertEquals("10.0", buckets.get(1).getKeyAsString());
                assertEquals(expected * 2, buckets.get(1).getDocCount());
                assertEquals("100.0", buckets.get(2).getKeyAsString());
                assertEquals(expected * 2, buckets.get(2).getDocCount());
                assertEquals("1000.0", buckets.get(3).getKeyAsString());
                assertEquals(expected, buckets.get(3).getDocCount());
            }
        }
    }

    public void testGlobalAggregationWithScore() throws IOException {
        try (Directory directory = newDirectory()) {
            try (RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory)) {
                Document document = new Document();
                document.add(new SortedDocValuesField("keyword", new BytesRef("a")));
                indexWriter.addDocument(document);
                document = new Document();
                document.add(new SortedDocValuesField("keyword", new BytesRef("c")));
                indexWriter.addDocument(document);
                document = new Document();
                document.add(new SortedDocValuesField("keyword", new BytesRef("e")));
                indexWriter.addDocument(document);
                try (IndexReader indexReader = maybeWrapReaderEs(indexWriter.getReader())) {
                    IndexSearcher indexSearcher = newIndexSearcher(indexReader);
                    String executionHint = randomFrom(TermsAggregatorFactory.ExecutionMode.values()).toString();
                    Aggregator.SubAggCollectionMode collectionMode = randomFrom(Aggregator.SubAggCollectionMode.values());
                    GlobalAggregationBuilder globalBuilder = new GlobalAggregationBuilder("global")
                        .subAggregation(
                            new TermsAggregationBuilder("terms").userValueTypeHint(ValueType.STRING)
                                .executionHint(executionHint)
                                .collectMode(collectionMode)
                                .field("keyword")
                                .order(BucketOrder.key(true))
                                .subAggregation(
                                    new TermsAggregationBuilder("sub_terms").userValueTypeHint(ValueType.STRING)
                                        .executionHint(executionHint)
                                        .collectMode(collectionMode)
                                        .field("keyword").order(BucketOrder.key(true))
                                        .subAggregation(
                                            new TopHitsAggregationBuilder("top_hits")
                                                .storedField("_none_")
                                        )
                                )
                        );

                    MappedFieldType fieldType = new KeywordFieldMapper.KeywordFieldType("keyword");

                    InternalGlobal result = searchAndReduce(indexSearcher, new MatchAllDocsQuery(), globalBuilder, fieldType);
                    InternalMultiBucketAggregation<?, ?> terms = result.getAggregations().get("terms");
                    assertThat(terms.getBuckets().size(), equalTo(3));
                    for (MultiBucketsAggregation.Bucket bucket : terms.getBuckets()) {
                        InternalMultiBucketAggregation<?, ?> subTerms = bucket.getAggregations().get("sub_terms");
                        assertThat(subTerms.getBuckets().size(), equalTo(1));
                        MultiBucketsAggregation.Bucket subBucket  = subTerms.getBuckets().get(0);
                        InternalTopHits topHits = subBucket.getAggregations().get("top_hits");
                        assertThat(topHits.getHits().getHits().length, equalTo(1));
                        for (SearchHit hit : topHits.getHits()) {
                            assertThat(hit.getScore(), greaterThan(0f));
                        }
                    }
                }
            }
        }
    }

    public void testWithNestedAggregations() throws IOException {
        try (Directory directory = newDirectory()) {
            try (RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory)) {
                for (int i = 0; i < 10; i++) {
                    int[] nestedValues = new int[i];
                    for (int j = 0; j < i; j++) {
                        nestedValues[j] = j;
                    }
                    indexWriter.addDocuments(generateDocsWithNested(Integer.toString(i), i, nestedValues));
                }
                indexWriter.commit();
                for (Aggregator.SubAggCollectionMode mode : Aggregator.SubAggCollectionMode.values()) {
                    for (boolean withScore : new boolean[]{true, false}) {
                        NestedAggregationBuilder nested = new NestedAggregationBuilder("nested", "nested_object")
                            .subAggregation(new TermsAggregationBuilder("terms").userValueTypeHint(ValueType.LONG)
                                .field("nested_value")
                                // force the breadth_first mode
                                .collectMode(mode)
                                .order(BucketOrder.key(true))
                                .subAggregation(
                                    new TopHitsAggregationBuilder("top_hits")
                                        .sort(withScore ? new ScoreSortBuilder() : new FieldSortBuilder("_doc"))
                                        .storedField("_none_")
                                )
                            );
                        MappedFieldType fieldType
                            = new NumberFieldMapper.NumberFieldType("nested_value", NumberFieldMapper.NumberType.LONG);
                        try (IndexReader indexReader = wrapInMockESDirectoryReader(DirectoryReader.open(directory))) {
                            {
                                InternalNested result = searchAndReduce(newSearcher(indexReader, false, true),
                                    // match root document only
                                    new DocValuesFieldExistsQuery(PRIMARY_TERM_NAME), nested, fieldType);
                                InternalMultiBucketAggregation<?, ?> terms = result.getAggregations().get("terms");
                                assertNestedTopHitsScore(terms, withScore);
                            }

                            {
                                FilterAggregationBuilder filter = new FilterAggregationBuilder("filter", new MatchAllQueryBuilder())
                                    .subAggregation(nested);
                                InternalFilter result = searchAndReduce(newSearcher(indexReader, false, true),
                                    // match root document only
                                    new DocValuesFieldExistsQuery(PRIMARY_TERM_NAME), filter, fieldType);
                                InternalNested nestedResult = result.getAggregations().get("nested");
                                InternalMultiBucketAggregation<?, ?> terms = nestedResult.getAggregations().get("terms");
                                assertNestedTopHitsScore(terms, withScore);
                            }
                        }
                    }
                }
            }
        }
    }

    public void testHeisenpig() throws IOException {
        MappedFieldType nestedFieldType = new NumberFieldMapper.NumberFieldType("number", NumberFieldMapper.NumberType.LONG);
        KeywordFieldType animalFieldType = new KeywordFieldType("str", randomBoolean(), true, Collections.emptyMap());
        try (Directory directory = newDirectory()) {
            try (RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory)) {
                String[] tags = new String[] {"danger", "fluffiness"};
                indexWriter.addDocuments(generateAnimalDocsWithNested("1", animalFieldType, "sheep", tags, new int[] {1, 10}));
                indexWriter.addDocuments(generateAnimalDocsWithNested("2", animalFieldType, "cow", tags, new int[] {3, 1}));
                indexWriter.addDocuments(generateAnimalDocsWithNested("3", animalFieldType, "pig", tags, new int[] {100, 1}));
                indexWriter.commit();
                NestedAggregationBuilder nested = new NestedAggregationBuilder("nested", "nested_object")
                    .subAggregation(
                        new MaxAggregationBuilder("max_number").field("number")
                    );
                TermsAggregationBuilder terms = new TermsAggregationBuilder("str_terms")
                    .field("str")
                    .subAggregation(nested)
                    .shardSize(10)
                    .size(10)
                    .order(BucketOrder.aggregation("nested>max_number", false));
                try (IndexReader indexReader = wrapInMockESDirectoryReader(DirectoryReader.open(directory))) {
                    StringTerms result = searchAndReduce(newSearcher(indexReader, false, true),
                        // match root document only
                        Queries.newNonNestedFilter(), terms, animalFieldType, nestedFieldType);
                    assertThat(result.getBuckets().get(0).getKeyAsString(), equalTo("pig"));
                    assertThat(result.getBuckets().get(0).docCount, equalTo(1L));
                    assertThat(((InternalMax) (((InternalNested)result.getBuckets().get(0).getAggregations().get("nested"))
                        .getAggregations().get("max_number"))).getValue(), closeTo(100.0, 0.00001));
                }
            }
        }
    }

    public void testSortingWithNestedAggregations() throws IOException {
        try (Directory directory = newDirectory()) {
            try (RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory)) {
                for (int i = 0; i < 12; i++) {
                    int[] nestedValues = new int[i];
                    for (int j = 0; j < i; j++) {
                        nestedValues[j] = j;
                    }
                    indexWriter.addDocuments(generateDocsWithNested(Integer.toString(i), i % 4, nestedValues));
                }
                indexWriter.commit();
                NestedAggregationBuilder nested = new NestedAggregationBuilder("nested", "nested_object")
                    .subAggregation(
                        new MaxAggregationBuilder("max_val").field("nested_value")
                    );
                TermsAggregationBuilder terms = new TermsAggregationBuilder("terms")
                    .field("value")
                    .subAggregation(nested)
                    .shardSize(1)
                    .size(1)
                    .order(BucketOrder.aggregation("nested>max_val", false));
                MappedFieldType nestedFieldType = new NumberFieldMapper.NumberFieldType("nested_value", NumberFieldMapper.NumberType.LONG);
                MappedFieldType fieldType = new NumberFieldMapper.NumberFieldType("value", NumberFieldMapper.NumberType.LONG);
                try (IndexReader indexReader = wrapInMockESDirectoryReader(DirectoryReader.open(directory))) {
                    LongTerms result = searchAndReduce(newSearcher(indexReader, false, true),
                        // match root document only
                        new DocValuesFieldExistsQuery(PRIMARY_TERM_NAME), terms, fieldType, nestedFieldType);
                    assertThat(result.getBuckets().get(0).term, equalTo(3L));
                    assertThat(((InternalMax) (((InternalNested)result.getBuckets().get(0).getAggregations().get("nested"))
                        .getAggregations().get("max_val"))).getValue(), closeTo(10.0, 0.00001));
                }
            }
        }
    }

    public void testManySegmentsStillSingleton() throws IOException {
        NumberFieldType nFt = new NumberFieldType("n", NumberFieldMapper.NumberType.LONG);
        KeywordFieldType strFt = new KeywordFieldType("str", true, true, Collections.emptyMap());
        AggregationBuilder builder = new TermsAggregationBuilder("n").field("n")
            .subAggregation(new TermsAggregationBuilder("str").field("str"));
        withNonMergingIndex(iw -> {
            iw.addDocument(
                List.of(
                    new SortedNumericDocValuesField("n", 1),
                    new LongPoint("n", 1),
                    new SortedSetDocValuesField("str", new BytesRef("sheep")),
                    new Field("str", new BytesRef("sheep"), KeywordFieldMapper.Defaults.FIELD_TYPE)
                )
            );
            iw.commit();   // Force two segments
            iw.addDocument(
                List.of(
                    new SortedNumericDocValuesField("n", 1),
                    new LongPoint("n", 1),
                    new SortedSetDocValuesField("str", new BytesRef("cow")),
                    new Field("str", new BytesRef("sheep"), KeywordFieldMapper.Defaults.FIELD_TYPE)
                )
            );
        }, searcher -> debugTestCase(
            builder,
            new MatchAllDocsQuery(),
            searcher,
            (LongTerms result, Class<? extends Aggregator> impl, Map<String, Map<String, Object>> debug) -> {
                Map<String, Object> subDebug = debug.get("n.str");
                assertThat(subDebug, hasEntry("segments_with_single_valued_ords", 2));
                assertThat(subDebug, hasEntry("segments_with_multi_valued_ords", 0));
            },
            nFt,
            strFt
        ));
    }

    public void testNumberToStringValueScript() throws IOException {
        MappedFieldType fieldType
            = new NumberFieldMapper.NumberFieldType("number", NumberFieldMapper.NumberType.INTEGER);

        TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("name")
            .userValueTypeHint(ValueType.STRING)
            .field("number")
            .script(new Script(ScriptType.INLINE, MockScriptEngine.NAME, STRING_SCRIPT_NAME, Collections.emptyMap()));

        testCase(aggregationBuilder, new MatchAllDocsQuery(), iw -> {
            final int numDocs = 10;
            for (int i = 0; i < numDocs; i++) {
                iw.addDocument(singleton(new NumericDocValuesField("number", i + 1)));
            }
        }, (Consumer<InternalTerms>) terms -> {
            assertTrue(AggregationInspectionHelper.hasValue(terms));
        }, fieldType);
    }

    public void testThreeLayerStringViaGlobalOrds() throws IOException {
        threeLayerStringTestCase("global_ordinals");
    }

    public void testThreeLayerStringViaMap() throws IOException {
        threeLayerStringTestCase("map");
    }

    private void threeLayerStringTestCase(String executionHint) throws IOException {
        MappedFieldType ift = new KeywordFieldType("i", randomBoolean(), true, Collections.emptyMap());
        MappedFieldType jft = new KeywordFieldType("j", randomBoolean(), true, Collections.emptyMap());
        MappedFieldType kft = new KeywordFieldType("k", randomBoolean(), true, Collections.emptyMap());

        try (Directory dir = newDirectory()) {
            try (RandomIndexWriter writer = new RandomIndexWriter(random(), dir)) {
                for (int i = 0; i < 10; i++) {
                    for (int j = 0; j < 10; j++) {
                        for (int k = 0; k < 10; k++) {
                            List<IndexableField> d = new ArrayList<>();
                            d.addAll(doc(ift, Integer.toString(i)));
                            d.addAll(doc(jft, Integer.toString(j)));
                            d.addAll(doc(kft, Integer.toString(k)));
                            writer.addDocument(d);
                        }
                    }
                }
                try (IndexReader reader = maybeWrapReaderEs(writer.getReader())) {
                    IndexSearcher searcher = newIndexSearcher(reader);
                    TermsAggregationBuilder request = new TermsAggregationBuilder("i").field("i").executionHint(executionHint)
                        .subAggregation(new TermsAggregationBuilder("j").field("j").executionHint(executionHint)
                            .subAggregation(new TermsAggregationBuilder("k").field("k").executionHint(executionHint)));
                    StringTerms result = searchAndReduce(searcher, new MatchAllDocsQuery(), request, ift, jft, kft);
                    for (int i = 0; i < 10; i++) {
                        StringTerms.Bucket iBucket = result.getBucketByKey(Integer.toString(i));
                        assertThat(iBucket.getDocCount(), equalTo(100L));
                        StringTerms jAgg = iBucket.getAggregations().get("j");
                        for (int j = 0; j < 10; j++) {
                            StringTerms.Bucket jBucket = jAgg.getBucketByKey(Integer.toString(j));
                            assertThat(jBucket.getDocCount(), equalTo(10L));
                            StringTerms kAgg = jBucket.getAggregations().get("k");
                            for (int k = 0; k < 10; k++) {
                                StringTerms.Bucket kBucket = kAgg.getBucketByKey(Integer.toString(k));
                                assertThat(kBucket.getDocCount(), equalTo(1L));
                            }
                        }
                    }
                }
            }
        }
    }

    public void testThreeLayerLong() throws IOException {
        try (Directory dir = newDirectory()) {
            try (RandomIndexWriter writer = new RandomIndexWriter(random(), dir)) {
                for (int i = 0; i < 10; i++) {
                    for (int j = 0; j < 10; j++) {
                        for (int k = 0; k < 10; k++) {
                            Document d = new Document();
                            d.add(new SortedNumericDocValuesField("i", i));
                            d.add(new SortedNumericDocValuesField("j", j));
                            d.add(new SortedNumericDocValuesField("k", k));
                            writer.addDocument(d);
                        }
                    }
                }
                try (IndexReader reader = maybeWrapReaderEs(writer.getReader())) {
                    IndexSearcher searcher = newIndexSearcher(reader);
                    TermsAggregationBuilder request = new TermsAggregationBuilder("i").field("i")
                        .subAggregation(new TermsAggregationBuilder("j").field("j")
                            .subAggregation(new TermsAggregationBuilder("k").field("k")));
                    LongTerms result = searchAndReduce(searcher, new MatchAllDocsQuery(), request,
                        longField("i"), longField("j"), longField("k"));
                    for (int i = 0; i < 10; i++) {
                        LongTerms.Bucket iBucket = result.getBucketByKey(Integer.toString(i));
                        assertThat(iBucket.getDocCount(), equalTo(100L));
                        LongTerms jAgg = iBucket.getAggregations().get("j");
                        for (int j = 0; j < 10; j++) {
                            LongTerms.Bucket jBucket = jAgg.getBucketByKey(Integer.toString(j));
                            assertThat(jBucket.getDocCount(), equalTo(10L));
                            LongTerms kAgg = jBucket.getAggregations().get("k");
                            for (int k = 0; k < 10; k++) {
                                LongTerms.Bucket kBucket = kAgg.getBucketByKey(Integer.toString(k));
                                assertThat(kBucket.getDocCount(), equalTo(1L));
                            }
                        }
                    }
                }
            }
        }
    }

    private void assertNestedTopHitsScore(InternalMultiBucketAggregation<?, ?> terms, boolean withScore) {
        assertThat(terms.getBuckets().size(), equalTo(9));
        int ptr = 9;
        for (MultiBucketsAggregation.Bucket bucket : terms.getBuckets()) {
            InternalTopHits topHits = bucket.getAggregations().get("top_hits");
            assertThat(topHits.getHits().getTotalHits().value, equalTo((long) ptr));
            assertEquals(TotalHits.Relation.EQUAL_TO, topHits.getHits().getTotalHits().relation);
            if (withScore) {
                assertThat(topHits.getHits().getMaxScore(), equalTo(1f));
            } else {
                assertThat(topHits.getHits().getMaxScore(), equalTo(Float.NaN));
            }
            --ptr;
        }
    }

    public void testOrderByPipelineAggregation() throws Exception {
        try (Directory directory = newDirectory()) {
            try (RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory)) {
                try (IndexReader indexReader = maybeWrapReaderEs(indexWriter.getReader())) {
                    IndexSearcher indexSearcher = newIndexSearcher(indexReader);

                    BucketScriptPipelineAggregationBuilder bucketScriptAgg = bucketScript(
                        "script", new Script("2.718"));
                    TermsAggregationBuilder termsAgg = terms("terms")
                        .field("field")
                        .userValueTypeHint(ValueType.STRING)
                        .order(BucketOrder.aggregation("script", true))
                        .subAggregation(bucketScriptAgg);

                    MappedFieldType fieldType = new KeywordFieldMapper.KeywordFieldType("field");

                    AggregationExecutionException e = expectThrows(AggregationExecutionException.class,
                        () -> createAggregator(termsAgg, indexSearcher, fieldType));
                    assertEquals("Invalid aggregation order path [script]. The provided aggregation [script] " +
                        "either does not exist, or is a pipeline aggregation and cannot be used to sort the buckets.",
                        e.getMessage());
                }
            }
        }
    }

    public void testFormatWithMissing() throws IOException {
        MappedFieldType fieldType
            = new NumberFieldMapper.NumberFieldType("number", NumberFieldMapper.NumberType.INTEGER);

        TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("name")
            .field("number")
            .format("$###.00")
            .missing(randomFrom(42, "$42", 42.0));

        testCase(aggregationBuilder, new MatchAllDocsQuery(), iw -> {
            final int numDocs = 10;
            iw.addDocument(singleton(new NumericDocValuesField("not_number", 0)));
            for (int i = 1; i < numDocs; i++) {
                iw.addDocument(singleton(new NumericDocValuesField("number", i + 1)));
            }
        }, (Consumer<InternalTerms<?, ?>>) terms -> assertTrue(AggregationInspectionHelper.hasValue(terms)), fieldType);
    }

    public void testFormatCannotParseMissing() throws IOException {
        MappedFieldType fieldType = new NumberFieldMapper.NumberFieldType("number", NumberFieldMapper.NumberType.INTEGER);

        TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("name").field("number").format("$###.00").missing("42");

        RuntimeException ex = expectThrows(RuntimeException.class, () -> testCase(aggregationBuilder, new MatchAllDocsQuery(), iw -> {
            final int numDocs = 10;
            iw.addDocument(singleton(new NumericDocValuesField("not_number", 0)));
            for (int i = 1; i < numDocs; i++) {
                iw.addDocument(singleton(new NumericDocValuesField("number", i + 1)));
            }
        }, (Consumer<InternalTerms<?, ?>>) terms -> fail("Should have thrown"), fieldType));

        assertThat(ex.getMessage(), equalTo("Cannot parse the value [42] using the pattern [$###.00]"));
    }

    public void testOrderByCardinality() throws IOException {
        boolean bIsString = randomBoolean();
        TermsAggregationBuilder aggregationBuilder = new TermsAggregationBuilder("a").field("a")
            .size(3)
            .shardSize(3)
            .subAggregation(new CardinalityAggregationBuilder("b").field("b"))
            .order(BucketOrder.aggregation("b", false));

        /*
         * Build documents where larger "a"s obviously have more distinct "b"s
         * associated with them. But insert them into Lucene in a random
         * order using Lucene's randomizeWriter so we'll bump into situations
         * where documents in the last segment change the outcome of the
         * cardinality agg. At least, right now the bug has to do with
         * documents in the last segment. But randomize so we can catch
         * new and strange bugs in the future. Finally, its important that
         * we have few enough values that cardinality can be exact.
         */
        List<List<IndexableField>> docs = new ArrayList<>();
        for (int a = 0; a < 10; a++) {
            for (int b = 0; b <= a; b++) {
                docs.add(
                    List.of(
                        new NumericDocValuesField("a", a),
                        bIsString ? new SortedSetDocValuesField("b", new BytesRef(Integer.toString(b))) : new NumericDocValuesField("b", b)
                    )
                );
            }
        }
        Collections.shuffle(docs, random());
        try (Directory directory = newDirectory()) {
            RandomIndexWriter iw = new RandomIndexWriter(random(), directory);
            for (List<IndexableField> doc : docs) {
                iw.addDocument(doc);
            }
            iw.close();

            try (DirectoryReader unwrapped = DirectoryReader.open(directory);
                    IndexReader indexReader = wrapDirectoryReader(unwrapped)) {
                IndexSearcher indexSearcher = newIndexSearcher(indexReader);

                LongTerms terms = searchAndReduce(
                    createIndexSettings(),
                    indexSearcher,
                    new MatchAllDocsQuery(),
                    aggregationBuilder,
                    Integer.MAX_VALUE,
                    false,
                    new NumberFieldMapper.NumberFieldType("a", NumberFieldMapper.NumberType.INTEGER),
                    bIsString
                        ? new KeywordFieldMapper.KeywordFieldType("b")
                        : new NumberFieldMapper.NumberFieldType("b", NumberFieldMapper.NumberType.INTEGER)
                );
                assertThat(
                    terms.getBuckets().stream().map(MultiBucketsAggregation.Bucket::getKey).collect(toList()),
                    equalTo(List.of(9L, 8L, 7L))
                );
                assertThat(
                    terms.getBuckets().stream().map(MultiBucketsAggregation.Bucket::getDocCount).collect(toList()),
                    equalTo(List.of(10L, 9L, 8L))
                );
            }
        }
    }

    public void testAsSubAgg() throws IOException {
        DateFieldType dft = new DateFieldType("d");
        KeywordFieldType kft = new KeywordFieldType("k", false, true, Collections.emptyMap());
        AggregationBuilder builder = new DateHistogramAggregationBuilder("dh").field("d")
            .calendarInterval(DateHistogramInterval.YEAR)
            .subAggregation(new TermsAggregationBuilder("k").field("k"));
        CheckedConsumer<RandomIndexWriter, IOException> buildIndex = iw -> {
            iw.addDocument(
                List.of(
                    new SortedNumericDocValuesField("d", dft.parse("2020-02-01T00:00:00Z")),
                    new LongPoint("d", dft.parse("2020-02-01T00:00:00Z")),
                    new SortedSetDocValuesField("k", new BytesRef("a"))
                )
            );
            iw.addDocument(
                List.of(
                    new SortedNumericDocValuesField("d", dft.parse("2020-03-01T00:00:00Z")),
                    new LongPoint("d", dft.parse("2020-03-01T00:00:00Z")),
                    new SortedSetDocValuesField("k", new BytesRef("a"))
                )
            );
            iw.addDocument(
                List.of(
                    new SortedNumericDocValuesField("d", dft.parse("2021-02-01T00:00:00Z")),
                    new LongPoint("d", dft.parse("2021-02-01T00:00:00Z")),
                    new SortedSetDocValuesField("k", new BytesRef("a"))
                )
            );
            iw.addDocument(
                List.of(
                    new SortedNumericDocValuesField("d", dft.parse("2021-03-01T00:00:00Z")),
                    new LongPoint("d", dft.parse("2021-03-01T00:00:00Z")),
                    new SortedSetDocValuesField("k", new BytesRef("a"))
                )
            );
            iw.addDocument(
                List.of(
                    new SortedNumericDocValuesField("d", dft.parse("2020-02-01T00:00:00Z")),
                    new LongPoint("d", dft.parse("2020-02-01T00:00:00Z")),
                    new SortedSetDocValuesField("k", new BytesRef("b"))
                )
            );
        };
        testCase(builder, new MatchAllDocsQuery(), buildIndex, (InternalDateHistogram dh) -> {
            assertThat(
                dh.getBuckets().stream().map(InternalDateHistogram.Bucket::getKeyAsString).collect(toList()),
                equalTo(List.of("2020-01-01T00:00:00.000Z", "2021-01-01T00:00:00.000Z"))
            );
            StringTerms terms = dh.getBuckets().get(0).getAggregations().get("k");
            assertThat(terms.getBuckets().stream().map(StringTerms.Bucket::getKey).collect(toList()), equalTo(List.of("a", "b")));
            terms = dh.getBuckets().get(1).getAggregations().get("k");
            assertThat(terms.getBuckets().stream().map(StringTerms.Bucket::getKey).collect(toList()), equalTo(List.of("a")));
        }, dft, kft);
        withAggregator(builder, new MatchAllDocsQuery(), buildIndex, (searcher, aggregator) -> {
            TermsAggregator terms = (TermsAggregator) aggregator.subAggregator("k");
            Map<String, Object> info = new HashMap<>();
            terms.collectDebugInfo(info::put);
            assertThat(info, hasEntry("collection_strategy", "remap using many bucket ords packed using [2/62] bits"));
        }, dft, kft);
    }

    public void testWithFilterAndPreciseSize() throws IOException {
        KeywordFieldType kft = new KeywordFieldType("k", true, true, Collections.emptyMap());
        CheckedConsumer<RandomIndexWriter, IOException> buildIndex = iw -> {
            iw.addDocument(
                List.of(
                    new Field("k", new BytesRef("a"), KeywordFieldMapper.Defaults.FIELD_TYPE),
                    new SortedSetDocValuesField("k", new BytesRef("a"))
                )
            );
            iw.addDocument(
                List.of(
                    new Field("k", new BytesRef("b"), KeywordFieldMapper.Defaults.FIELD_TYPE),
                    new SortedSetDocValuesField("k", new BytesRef("b"))
                )
            );
            iw.addDocument(
                List.of(
                    new Field("k", new BytesRef("c"), KeywordFieldMapper.Defaults.FIELD_TYPE),
                    new SortedSetDocValuesField("k", new BytesRef("c"))
                )
            );
        };
        TermsAggregationBuilder builder = new TermsAggregationBuilder("k").field("k");
        /*
         * There was a bug where we would accidentally send buckets with 0
         * docs in them back to the coordinating node which would take up a
         * slot that a bucket with docs in it deserves. Combination of
         * ordering by bucket, the precise size, and the top level query
         * would trigger that bug.
         */
        builder.size(2).order(BucketOrder.key(true));
        Query topLevel = new TermInSetQuery("k", new BytesRef[] {new BytesRef("b"), new BytesRef("c")});
        testCase(builder, topLevel, buildIndex, (StringTerms terms) -> {
            assertThat(terms.getBuckets().stream().map(StringTerms.Bucket::getKey).collect(toList()), equalTo(List.of("b", "c")));
        }, kft);
        withAggregator(builder, topLevel, buildIndex, (searcher, terms) -> {
            Map<String, Object> info = new HashMap<>();
            terms.collectDebugInfo(info::put);
            assertThat(info, hasEntry("delegate", "FiltersAggregator.FilterByFilter"));
        }, kft);
    }

    /**
     * If the top level query is a runtime field we should still use
     * {@link StringTermsAggregatorFromFilters} because we expect it'll still
     * be faster that the normal aggregator, even though running the script
     * for the runtime field is quite a bit more expensive than a regular
     * query. The thing is, we won't be executing the script more times than
     * we would if it were just at the top level.
     */
    public void testRuntimeFieldTopLevelQueryStillOptimized() throws IOException {
        long totalDocs = 500;
        SearchLookup lookup = new SearchLookup(s -> null, (ft, l) -> null);
        StringFieldScript.LeafFactory scriptFactory = ctx -> new StringFieldScript("dummy", Map.of(), lookup, ctx) {
            @Override
            public void execute() {
                emit("cat");
            }
        };
        BytesRef[] values = new BytesRef[] {
            new BytesRef("stuff"), new BytesRef("more_stuff"), new BytesRef("other_stuff"),
        };
        Query query = new StringScriptFieldTermQuery(new Script("dummy"), scriptFactory, "dummy", "cat", false);
        debugTestCase(new TermsAggregationBuilder("t").field("k"), query, iw -> {
            for (int d = 0; d < totalDocs; d++) {
                BytesRef value = values[d % values.length];
                iw.addDocument(
                    List.of(new Field("k", value, KeywordFieldMapper.Defaults.FIELD_TYPE), new SortedSetDocValuesField("k", value))
                );
            }
        }, (StringTerms r, Class<? extends Aggregator> impl, Map<String, Map<String, Object>> debug) -> {
            assertThat(
                r.getBuckets().stream().map(StringTerms.Bucket::getKey).collect(toList()),
                equalTo(List.of("more_stuff", "stuff", "other_stuff"))
            );
            assertThat(r.getBuckets().stream().map(StringTerms.Bucket::getDocCount).collect(toList()), equalTo(List.of(167L, 167L, 166L)));
            assertThat(impl, equalTo(StringTermsAggregatorFromFilters.class));
            Map<?, ?> topLevelDebug = (Map<?, ?>) debug.get("t");
            Map<?, ?> delegateDebug = (Map<?, ?>) topLevelDebug.get("delegate_debug");
            // We don't estimate the cost here so these shouldn't show up
            assertThat(delegateDebug, not(hasKey("estimated_cost")));
            assertThat(delegateDebug, not(hasKey("max_cost")));
            assertThat((int) delegateDebug.get("segments_counted"), greaterThan(0));
        }, new KeywordFieldType("k", true, true, Collections.emptyMap()));
    }

    private final SeqNoFieldMapper.SequenceIDFields sequenceIDFields = SeqNoFieldMapper.SequenceIDFields.emptySeqID();
    private List<Document> generateDocsWithNested(String id, int value, int[] nestedValues) {
        List<Document> documents = new ArrayList<>();

        for (int nestedValue : nestedValues) {
            Document document = new Document();
            document.add(new Field(IdFieldMapper.NAME, Uid.encodeId(id), IdFieldMapper.Defaults.NESTED_FIELD_TYPE));
            document.add(new Field(NestedPathFieldMapper.NAME, "nested_object", NestedPathFieldMapper.Defaults.FIELD_TYPE));
            document.add(new SortedNumericDocValuesField("nested_value", nestedValue));
            documents.add(document);
        }

        Document document = new Document();
        document.add(new Field(IdFieldMapper.NAME, Uid.encodeId(id), IdFieldMapper.Defaults.FIELD_TYPE));
        document.add(new Field(NestedPathFieldMapper.NAME, "docs", NestedPathFieldMapper.Defaults.FIELD_TYPE));
        document.add(new SortedNumericDocValuesField("value", value));
        document.add(sequenceIDFields.primaryTerm);
        documents.add(document);

        return documents;
    }

    private List<List<IndexableField>> generateAnimalDocsWithNested(
        String id,
        KeywordFieldType animalFieldType,
        String animal,
        String[] tags,
        int[] nestedValues
    ) {
        List<List<IndexableField>> documents = new ArrayList<>();

        for (int i = 0; i < tags.length; i++) {
            List<IndexableField> document = new ArrayList<>();
            document.add(new Field(IdFieldMapper.NAME, Uid.encodeId(id), IdFieldMapper.Defaults.NESTED_FIELD_TYPE));

            document.add(new Field(NestedPathFieldMapper.NAME, "nested_object", NestedPathFieldMapper.Defaults.FIELD_TYPE));
            document.add(new SortedDocValuesField("tag", new BytesRef(tags[i])));
            document.add(new SortedNumericDocValuesField("number", nestedValues[i]));
            documents.add(document);
        }

        List<IndexableField> document = new ArrayList<>();
        document.add(new Field(IdFieldMapper.NAME, Uid.encodeId(id), IdFieldMapper.Defaults.FIELD_TYPE));
        document.addAll(doc(animalFieldType, animal));
        document.add(new Field(NestedPathFieldMapper.NAME, "docs", NestedPathFieldMapper.Defaults.FIELD_TYPE));
        document.add(sequenceIDFields.primaryTerm);
        documents.add(document);

        return documents;
    }

    private IndexReader createIndexWithLongs() throws IOException {
        Directory directory = newDirectory();
        RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory);
        Document document = new Document();
        document.add(new SortedNumericDocValuesField("number", 10));
        document.add(new SortedNumericDocValuesField("number", 100));
        indexWriter.addDocument(document);
        document = new Document();
        document.add(new SortedNumericDocValuesField("number", 1));
        document.add(new SortedNumericDocValuesField("number", 100));
        indexWriter.addDocument(document);
        document = new Document();
        document.add(new SortedNumericDocValuesField("number", 10));
        document.add(new SortedNumericDocValuesField("number", 1000));
        indexWriter.addDocument(document);
        indexWriter.close();
        return DirectoryReader.open(directory);
    }

    private IndexReader createIndexWithDoubles() throws IOException {
        Directory directory = newDirectory();
        RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory);
        Document document = new Document();
        document.add(new SortedNumericDocValuesField("number", NumericUtils.doubleToSortableLong(10.0d)));
        document.add(new SortedNumericDocValuesField("number", NumericUtils.doubleToSortableLong(100.0d)));
        indexWriter.addDocument(document);
        document = new Document();
        document.add(new SortedNumericDocValuesField("number", NumericUtils.doubleToSortableLong(1.0d)));
        document.add(new SortedNumericDocValuesField("number", NumericUtils.doubleToSortableLong(100.0d)));
        indexWriter.addDocument(document);
        document = new Document();
        document.add(new SortedNumericDocValuesField("number", NumericUtils.doubleToSortableLong(10.0d)));
        document.add(new SortedNumericDocValuesField("number", NumericUtils.doubleToSortableLong(1000.0d)));
        indexWriter.addDocument(document);
        indexWriter.close();
        return DirectoryReader.open(directory);
    }

    private InternalAggregation buildInternalAggregation(TermsAggregationBuilder builder, MappedFieldType fieldType,
                                                         IndexSearcher searcher) throws IOException {
        TermsAggregator aggregator = createAggregator(builder, searcher, fieldType);
        aggregator.preCollection();
        searcher.search(new MatchAllDocsQuery(), aggregator);
        aggregator.postCollection();
        return aggregator.buildTopLevel();
    }

    private <T extends InternalAggregation> T reduce(Aggregator agg, BigArrays bigArrays) throws IOException {
        // now do the final reduce
        MultiBucketConsumerService.MultiBucketConsumer reduceBucketConsumer =
            new MultiBucketConsumerService.MultiBucketConsumer(Integer.MAX_VALUE,
                new NoneCircuitBreakerService().getBreaker(CircuitBreaker.REQUEST));
        InternalAggregation.ReduceContext context = InternalAggregation.ReduceContext.forFinalReduction(
            bigArrays, getMockScriptService(), reduceBucketConsumer, PipelineTree.EMPTY);

        T topLevel  = (T) agg.buildTopLevel();
        T result = (T) topLevel.reduce(Collections.singletonList(topLevel), context);
        doAssertReducedMultiBucketConsumer(result, reduceBucketConsumer);
        return result;
    }

    @Override
    protected List<ObjectMapper> objectMappers() {
        return List.of(NestedAggregatorTests.nestedObject("nested_object"));
    }
}
