/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.search.aggregations.timeseries;

import org.apache.lucene.document.DoubleDocValuesField;
import org.apache.lucene.document.FloatDocValuesField;
import org.apache.lucene.document.NumericDocValuesField;
import org.apache.lucene.document.SortedNumericDocValuesField;
import org.apache.lucene.document.SortedSetDocValuesField;
import org.apache.lucene.index.IndexableField;
import org.apache.lucene.index.RandomIndexWriter;
import org.apache.lucene.search.MatchAllDocsQuery;
import org.apache.lucene.search.Query;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.core.CheckedConsumer;
import org.elasticsearch.index.mapper.DataStreamTimestampFieldMapper;
import org.elasticsearch.index.mapper.DateFieldMapper;
import org.elasticsearch.index.mapper.KeywordFieldMapper;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.index.mapper.NumberFieldMapper;
import org.elasticsearch.index.mapper.TimeSeriesIdFieldMapper;
import org.elasticsearch.search.aggregations.AggregatorTestCase;
import org.elasticsearch.search.aggregations.metrics.Sum;
import org.elasticsearch.search.aggregations.support.ValuesSourceType;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.SortedMap;
import java.util.TreeMap;
import java.util.function.Consumer;

import static org.elasticsearch.search.aggregations.AggregationBuilders.sum;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasSize;

public class TimeSeriesAggregatorTests extends AggregatorTestCase {

    @Override
    protected List<ValuesSourceType> getSupportedValuesSourceTypes() {
        return List.of();
    }

    public void testStandAloneTimeSeriesWithSum() throws IOException {
        TimeSeriesAggregationBuilder aggregationBuilder = new TimeSeriesAggregationBuilder("ts").subAggregation(sum("sum").field("val1"));
        long startTime = DateFieldMapper.DEFAULT_DATE_TIME_FORMATTER.parseMillis("2021-01-01T00:00:00Z");
        timeSeriesTestCase(aggregationBuilder, new MatchAllDocsQuery(), iw -> {
            writeTS(iw, startTime + 1, new Object[] { "dim1", "aaa", "dim2", "xxx" }, new Object[] { "val1", 1 });
            writeTS(iw, startTime + 2, new Object[] { "dim1", "aaa", "dim2", "yyy" }, new Object[] { "val1", 2 });
            writeTS(iw, startTime + 3, new Object[] { "dim1", "bbb", "dim2", "zzz" }, new Object[] { "val1", 3 });
            writeTS(iw, startTime + 4, new Object[] { "dim1", "bbb", "dim2", "zzz" }, new Object[] { "val1", 4 });
            writeTS(iw, startTime + 5, new Object[] { "dim1", "aaa", "dim2", "xxx" }, new Object[] { "val1", 5 });
            writeTS(iw, startTime + 6, new Object[] { "dim1", "aaa", "dim2", "yyy" }, new Object[] { "val1", 6 });
            writeTS(iw, startTime + 7, new Object[] { "dim1", "bbb", "dim2", "zzz" }, new Object[] { "val1", 7 });
            writeTS(iw, startTime + 8, new Object[] { "dim1", "bbb", "dim2", "zzz" }, new Object[] { "val1", 8 });
        }, ts -> {
            assertThat(ts.getBuckets(), hasSize(3));

            assertThat(ts.getBucketByKey("{dim1=aaa, dim2=xxx}").docCount, equalTo(2L));
            assertThat(((Sum) ts.getBucketByKey("{dim1=aaa, dim2=xxx}").getAggregations().get("sum")).getValue(), equalTo(6.0));
            assertThat(ts.getBucketByKey("{dim1=aaa, dim2=yyy}").docCount, equalTo(2L));
            assertThat(((Sum) ts.getBucketByKey("{dim1=aaa, dim2=yyy}").getAggregations().get("sum")).getValue(), equalTo(8.0));
            assertThat(ts.getBucketByKey("{dim1=bbb, dim2=zzz}").docCount, equalTo(4L));
            assertThat(((Sum) ts.getBucketByKey("{dim1=bbb, dim2=zzz}").getAggregations().get("sum")).getValue(), equalTo(22.0));

        },
            new KeywordFieldMapper.KeywordFieldType("dim1"),
            new KeywordFieldMapper.KeywordFieldType("dim2"),
            new NumberFieldMapper.NumberFieldType("val1", NumberFieldMapper.NumberType.INTEGER)
        );
    }

    public static void writeTS(RandomIndexWriter iw, long timestamp, Object[] dimensions, Object[] metrics) throws IOException {
        final List<IndexableField> fields = new ArrayList<>();
        fields.add(new SortedNumericDocValuesField(DataStreamTimestampFieldMapper.DEFAULT_PATH, timestamp));
        final SortedMap<String, BytesReference> dimensionFields = new TreeMap<>();
        for (int i = 0; i < dimensions.length; i += 2) {
            final BytesReference reference;
            if (dimensions[i + 1] instanceof Number) {
                reference = TimeSeriesIdFieldMapper.encodeTsidValue(((Number) dimensions[i + 1]).longValue());
            } else {
                reference = TimeSeriesIdFieldMapper.encodeTsidValue(dimensions[i + 1].toString());
            }
            dimensionFields.put(dimensions[i].toString(), reference);
        }
        for (int i = 0; i < metrics.length; i += 2) {
            if (metrics[i + 1] instanceof Integer || metrics[i + 1] instanceof Long) {
                fields.add(new NumericDocValuesField(metrics[i].toString(), ((Number) metrics[i + 1]).longValue()));
            } else if (metrics[i + 1] instanceof Float) {
                fields.add(new FloatDocValuesField(metrics[i].toString(), (float) metrics[i + 1]));
            } else if (metrics[i + 1] instanceof Double) {
                fields.add(new DoubleDocValuesField(metrics[i].toString(), (double) metrics[i + 1]));
            }
        }
        try (BytesStreamOutput out = new BytesStreamOutput()) {
            TimeSeriesIdFieldMapper.encodeTsid(out, dimensionFields);
            BytesReference timeSeriesId = out.bytes();
            fields.add(new SortedSetDocValuesField(TimeSeriesIdFieldMapper.NAME, timeSeriesId.toBytesRef()));
        }
        // TODO: Handle metrics
        iw.addDocument(fields.stream().toList());
    }

    private void timeSeriesTestCase(
        TimeSeriesAggregationBuilder builder,
        Query query,
        CheckedConsumer<RandomIndexWriter, IOException> buildIndex,
        Consumer<InternalTimeSeries> verify,
        MappedFieldType... fieldTypes
    ) throws IOException {
        MappedFieldType[] newFieldTypes = new MappedFieldType[fieldTypes.length + 2];
        newFieldTypes[0] = TimeSeriesIdFieldMapper.FIELD_TYPE;
        newFieldTypes[1] = new DateFieldMapper.DateFieldType("@timestamp");
        System.arraycopy(fieldTypes, 0, newFieldTypes, 2, fieldTypes.length);

        testCase(builder, query, buildIndex, verify, newFieldTypes);
    }

}
