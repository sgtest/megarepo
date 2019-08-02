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

package org.elasticsearch.search.aggregations.pipeline;

import org.apache.lucene.document.Document;
import org.apache.lucene.document.LongPoint;
import org.apache.lucene.document.NumericDocValuesField;
import org.apache.lucene.document.SortedNumericDocValuesField;
import org.apache.lucene.index.DirectoryReader;
import org.apache.lucene.index.IndexReader;
import org.apache.lucene.index.RandomIndexWriter;
import org.apache.lucene.search.IndexSearcher;
import org.apache.lucene.search.MatchAllDocsQuery;
import org.apache.lucene.search.Query;
import org.apache.lucene.store.Directory;
import org.elasticsearch.common.time.DateFormatters;
import org.elasticsearch.index.mapper.DateFieldMapper;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.index.mapper.NumberFieldMapper;
import org.elasticsearch.script.Script;
import org.elasticsearch.script.ScriptService;
import org.elasticsearch.search.aggregations.AggregatorTestCase;
import org.elasticsearch.search.aggregations.PipelineAggregationBuilder;
import org.elasticsearch.search.aggregations.TestAggregatorFactory;
import org.elasticsearch.search.aggregations.bucket.histogram.DateHistogramAggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.histogram.DateHistogramInterval;
import org.elasticsearch.search.aggregations.bucket.histogram.Histogram;
import org.elasticsearch.search.aggregations.bucket.histogram.InternalDateHistogram;
import org.elasticsearch.search.aggregations.metrics.AvgAggregationBuilder;

import java.io.IOException;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Consumer;
import java.util.stream.Collectors;

import static org.hamcrest.Matchers.equalTo;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class MovFnUnitTests extends AggregatorTestCase {

    private static final String DATE_FIELD = "date";
    private static final String INSTANT_FIELD = "instant";
    private static final String VALUE_FIELD = "value_field";

    private static final List<String> datasetTimes = Arrays.asList(
        "2017-01-01T01:07:45",
        "2017-01-02T03:43:34",
        "2017-01-03T04:11:00",
        "2017-01-04T05:11:31",
        "2017-01-05T08:24:05",
        "2017-01-06T13:09:32",
        "2017-01-07T13:47:43",
        "2017-01-08T16:14:34",
        "2017-01-09T17:09:50",
        "2017-01-10T22:55:46");

    private static final List<Integer> datasetValues = Arrays.asList(1,2,3,4,5,6,7,8,9,10);

    public void testMatchAllDocs() throws IOException {
        check(0, List.of(Double.NaN, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0));
    }

    public void testShift() throws IOException {
        check(1, List.of(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0));
        check(5, List.of(5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 10.0, 10.0, Double.NaN, Double.NaN));
        check(-5, List.of(Double.NaN, Double.NaN, Double.NaN, Double.NaN, Double.NaN, Double.NaN, 1.0, 2.0, 3.0, 4.0));
    }

    public void testWideWindow() throws IOException {
        Script script = new Script(Script.DEFAULT_SCRIPT_TYPE, "painless", "test", Collections.emptyMap());
        MovFnPipelineAggregationBuilder builder = new MovFnPipelineAggregationBuilder("mov_fn", "avg", script, 100);
        builder.setShift(50);
        check(builder, script, List.of(10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0));
    }

    private void check(int shift, List<Double> expected) throws IOException {
        Script script = new Script(Script.DEFAULT_SCRIPT_TYPE, "painless", "test", Collections.emptyMap());
        MovFnPipelineAggregationBuilder builder = new MovFnPipelineAggregationBuilder("mov_fn", "avg", script, 3);
        builder.setShift(shift);
        check(builder, script, expected);
    }

    private void check(MovFnPipelineAggregationBuilder builder, Script script, List<Double> expected) throws IOException {
        Query query = new MatchAllDocsQuery();
        DateHistogramAggregationBuilder aggBuilder = new DateHistogramAggregationBuilder("histo");
        aggBuilder.calendarInterval(DateHistogramInterval.DAY).field(DATE_FIELD);
        aggBuilder.subAggregation(new AvgAggregationBuilder("avg").field(VALUE_FIELD));
        aggBuilder.subAggregation(builder);

        executeTestCase(query, aggBuilder, histogram -> {
                List<? extends Histogram.Bucket> buckets = histogram.getBuckets();
                List<Double> actual = buckets.stream()
                    .map(bucket -> ((InternalSimpleValue) (bucket.getAggregations().get("mov_fn"))).value())
                    .collect(Collectors.toList());
                assertThat(actual, equalTo(expected));
            }, 1000, script);
    }


    private void executeTestCase(Query query,
                                 DateHistogramAggregationBuilder aggBuilder,
                                 Consumer<Histogram> verify,
                                 int maxBucket, Script script) throws IOException {

        try (Directory directory = newDirectory()) {
            try (RandomIndexWriter indexWriter = new RandomIndexWriter(random(), directory)) {
                Document document = new Document();
                int counter = 0;
                for (String date : datasetTimes) {
                    if (frequently()) {
                        indexWriter.commit();
                    }

                    long instant = asLong(date);
                    document.add(new SortedNumericDocValuesField(DATE_FIELD, instant));
                    document.add(new LongPoint(INSTANT_FIELD, instant));
                    document.add(new NumericDocValuesField(VALUE_FIELD, datasetValues.get(counter)));
                    indexWriter.addDocument(document);
                    document.clear();
                    counter += 1;
                }
            }

            ScriptService scriptService = mock(ScriptService.class);
            MovingFunctionScript.Factory factory = mock(MovingFunctionScript.Factory.class);
            when(scriptService.compile(script, MovingFunctionScript.CONTEXT)).thenReturn(factory);

            MovingFunctionScript scriptInstance = new MovingFunctionScript() {
                @Override
                public double execute(Map<String, Object> params, double[] values) {
                    assertNotNull(values);
                    return MovingFunctions.max(values);
                }
            };

            when(factory.newInstance()).thenReturn(scriptInstance);

            try (IndexReader indexReader = DirectoryReader.open(directory)) {
                IndexSearcher indexSearcher = newSearcher(indexReader, true, true);

                DateFieldMapper.Builder builder = new DateFieldMapper.Builder("_name");
                DateFieldMapper.DateFieldType fieldType = builder.fieldType();
                fieldType.setHasDocValues(true);
                fieldType.setName(aggBuilder.field());

                MappedFieldType valueFieldType = new NumberFieldMapper.NumberFieldType(NumberFieldMapper.NumberType.LONG);
                valueFieldType.setHasDocValues(true);
                valueFieldType.setName("value_field");

                InternalDateHistogram histogram;
                histogram = searchAndReduce(indexSearcher, query, aggBuilder, maxBucket, scriptService,
                    new MappedFieldType[]{fieldType, valueFieldType});
                verify.accept(histogram);
            }
        }
    }

    private static long asLong(String dateTime) {
        return DateFormatters.from(DateFieldMapper.DEFAULT_DATE_TIME_FORMATTER.parse(dateTime)).toInstant().toEpochMilli();
    }
    
    /**
     * The validation should verify the parent aggregation is allowed.
     */
    public void testValidate() throws IOException {
        final Set<PipelineAggregationBuilder> aggBuilders = new HashSet<>();
        Script script = new Script(Script.DEFAULT_SCRIPT_TYPE, "painless", "test", Collections.emptyMap());
        aggBuilders.add(new MovFnPipelineAggregationBuilder("mov_fn", "avg", script, 3));

        final MovFnPipelineAggregationBuilder builder = new MovFnPipelineAggregationBuilder("name", "invalid_agg>metric", script, 1);
        builder.validate(PipelineAggregationHelperTests.getRandomSequentiallyOrderedParentAgg(), Collections.emptySet(), aggBuilders);
    }

    /**
     * The validation should throw an IllegalArgumentException, since parent
     * aggregation is not a type of HistogramAggregatorFactory,
     * DateHistogramAggregatorFactory or AutoDateHistogramAggregatorFactory.
     */
    public void testValidateException() throws IOException {
        final Set<PipelineAggregationBuilder> aggBuilders = new HashSet<>();
        Script script = new Script(Script.DEFAULT_SCRIPT_TYPE, "painless", "test", Collections.emptyMap());
        aggBuilders.add(new MovFnPipelineAggregationBuilder("mov_fn", "avg", script, 3));
        TestAggregatorFactory parentFactory = TestAggregatorFactory.createInstance();

        final MovFnPipelineAggregationBuilder builder = new MovFnPipelineAggregationBuilder("name", "invalid_agg>metric", script, 1);
        IllegalStateException ex = expectThrows(IllegalStateException.class,
                () -> builder.validate(parentFactory, Collections.emptySet(), aggBuilders));
        assertEquals("moving_fn aggregation [name] must have a histogram, date_histogram or auto_date_histogram as parent",
                ex.getMessage());
    }
}
