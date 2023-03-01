/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.apache.lucene.index.DocValuesType;
import org.apache.lucene.index.IndexableField;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.common.Strings;
import org.elasticsearch.core.Tuple;
import org.elasticsearch.index.IndexMode;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.mapper.NumberFieldTypeTests.OutOfRangeSpec;
import org.elasticsearch.script.DoubleFieldScript;
import org.elasticsearch.script.LongFieldScript;
import org.elasticsearch.script.Script;
import org.elasticsearch.script.ScriptContext;
import org.elasticsearch.script.ScriptFactory;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xcontent.XContentBuilder;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.function.Function;
import java.util.function.Supplier;
import java.util.stream.Collectors;

import static org.hamcrest.Matchers.both;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.matchesPattern;

public abstract class NumberFieldMapperTests extends MapperTestCase {

    /**
     * @return a List of OutOfRangeSpec to test for this number type
     */
    protected abstract List<OutOfRangeSpec> outOfRangeSpecs();

    /**
     * @return an appropriate value to use for a missing value for this number type
     */
    protected abstract Number missingValue();

    /**
     * @return does this mapper allow index time scripts
     */
    protected boolean allowsIndexTimeScript() {
        return false;
    }

    @Override
    protected void registerParameters(ParameterChecker checker) throws IOException {
        checker.registerConflictCheck("doc_values", b -> b.field("doc_values", false));
        checker.registerConflictCheck("index", b -> b.field("index", false));
        checker.registerConflictCheck("store", b -> b.field("store", true));
        checker.registerConflictCheck("null_value", b -> b.field("null_value", 1));
        checker.registerUpdateCheck(b -> b.field("coerce", false), m -> assertFalse(((NumberFieldMapper) m).coerce()));

        if (allowsIndexTimeScript()) {
            checker.registerConflictCheck("script", b -> b.field("script", "foo"));
            checker.registerUpdateCheck(b -> {
                minimalMapping(b);
                b.field("script", "test");
                b.field("on_script_error", "fail");
            }, b -> {
                minimalMapping(b);
                b.field("script", "test");
                b.field("on_script_error", "continue");
            }, m -> assertThat((m).onScriptError, is(OnScriptError.CONTINUE)));
        }
    }

    @Override
    protected Object getSampleValueForDocument() {
        return 123;
    }

    public void testExistsQueryDocValuesDisabled() throws IOException {
        MapperService mapperService = createMapperService(fieldMapping(b -> {
            minimalMapping(b);
            b.field("doc_values", false);
        }));
        assertExistsQuery(mapperService);
        assertParseMinimalWarnings();
    }

    public void testAggregationsDocValuesDisabled() throws IOException {
        MapperService mapperService = createMapperService(fieldMapping(b -> {
            minimalMapping(b);
            b.field("doc_values", false);
        }));
        assertAggregatableConsistency(mapperService.fieldType("field"));
    }

    public void testDefaults() throws Exception {
        XContentBuilder mapping = fieldMapping(this::minimalMapping);
        DocumentMapper mapper = createDocumentMapper(mapping);
        assertEquals(Strings.toString(mapping), mapper.mappingSource().toString());

        ParsedDocument doc = mapper.parse(source(b -> b.field("field", 123)));

        IndexableField[] fields = doc.rootDoc().getFields("field");
        // One field indexes points
        assertEquals(1, Arrays.stream(fields).filter(f -> f.fieldType().pointIndexDimensionCount() != 0).count());
        // One field indexes doc values
        assertEquals(1, Arrays.stream(fields).filter(f -> f.fieldType().docValuesType() != DocValuesType.NONE).count());
    }

    public void testNotIndexed() throws Exception {
        DocumentMapper mapper = createDocumentMapper(fieldMapping(b -> {
            minimalMapping(b);
            b.field("index", false);
        }));
        ParsedDocument doc = mapper.parse(source(b -> b.field("field", 123)));

        IndexableField[] fields = doc.rootDoc().getFields("field");
        assertEquals(1, fields.length);
        IndexableField dvField = fields[0];
        assertEquals(DocValuesType.SORTED_NUMERIC, dvField.fieldType().docValuesType());
    }

    public void testNoDocValues() throws Exception {
        DocumentMapper mapper = createDocumentMapper(fieldMapping(b -> {
            minimalMapping(b);
            b.field("doc_values", false);
        }));
        ParsedDocument doc = mapper.parse(source(b -> b.field("field", 123)));

        IndexableField[] fields = doc.rootDoc().getFields("field");
        assertEquals(1, fields.length);
        IndexableField pointField = fields[0];
        assertEquals(1, pointField.fieldType().pointIndexDimensionCount());
        assertEquals(123, pointField.numericValue().doubleValue(), 0d);
    }

    public void testStore() throws Exception {
        DocumentMapper mapper = createDocumentMapper(fieldMapping(b -> {
            minimalMapping(b);
            b.field("store", true);
        }));
        ParsedDocument doc = mapper.parse(source(b -> b.field("field", 123)));

        IndexableField[] fields = doc.rootDoc().getFields("field");

        // One field indexes points
        assertEquals(1, Arrays.stream(fields).filter(f -> f.fieldType().pointIndexDimensionCount() != 0).count());
        // One field indexes doc values
        assertEquals(1, Arrays.stream(fields).filter(f -> f.fieldType().docValuesType() != DocValuesType.NONE).count());
        // One field is stored
        assertEquals(1, Arrays.stream(fields).filter(f -> f.fieldType().stored()).count());
    }

    public void testCoerce() throws IOException {
        DocumentMapper mapper = createDocumentMapper(fieldMapping(this::minimalMapping));
        ParsedDocument doc = mapper.parse(source(b -> b.field("field", "123")));

        IndexableField[] fields = doc.rootDoc().getFields("field");
        // One field indexes points
        assertEquals(1, Arrays.stream(fields).filter(f -> f.fieldType().pointIndexDimensionCount() != 0).count());
        // One field indexes doc values
        assertEquals(1, Arrays.stream(fields).filter(f -> f.fieldType().docValuesType() != DocValuesType.NONE).count());

        DocumentMapper mapper2 = createDocumentMapper(fieldMapping(b -> {
            minimalMapping(b);
            b.field("coerce", false);
        }));
        MapperParsingException e = expectThrows(MapperParsingException.class, () -> mapper2.parse(source(b -> b.field("field", "123"))));
        assertThat(e.getCause().getMessage(), containsString("passed as String"));
    }

    @Override
    protected boolean supportsIgnoreMalformed() {
        return true;
    }

    @Override
    protected List<ExampleMalformedValue> exampleMalformedValues() {
        return List.of(
            exampleMalformedValue("a").errorMatches("For input string: \"a\""),
            exampleMalformedValue(b -> b.value(false)).errorMatches(
                both(containsString("Current token")).and(containsString("not numeric, can not use numeric value accessors"))
            )
        );
    }

    /**
     * Test that in case the malformed value is an xContent object we throw error regardless of `ignore_malformed`
     */
    public void testIgnoreMalformedWithObject() throws Exception {
        SourceToParse malformed = source(b -> b.startObject("field").field("foo", "bar").endObject());
        for (Boolean ignoreMalformed : new Boolean[] { true, false }) {
            DocumentMapper mapper = createDocumentMapper(fieldMapping(b -> {
                minimalMapping(b);
                b.field("ignore_malformed", ignoreMalformed);
            }));
            MapperParsingException e = expectThrows(MapperParsingException.class, () -> mapper.parse(malformed));
            assertThat(e.getCause().getMessage(), containsString("Cannot parse object as number"));
        }
    }

    protected void testNullValue() throws IOException {
        DocumentMapper mapper = createDocumentMapper(fieldMapping(this::minimalMapping));
        SourceToParse source = source(b -> b.nullField("field"));
        ParsedDocument doc = mapper.parse(source);
        assertArrayEquals(new IndexableField[0], doc.rootDoc().getFields("field"));

        Number missing = missingValue();
        mapper = createDocumentMapper(fieldMapping(b -> {
            minimalMapping(b);
            b.field("null_value", missing);
        }));
        doc = mapper.parse(source);
        IndexableField[] fields = doc.rootDoc().getFields("field");
        assertEquals(2, fields.length);
        IndexableField pointField = fields[0];
        assertEquals(1, pointField.fieldType().pointIndexDimensionCount());
        assertFalse(pointField.fieldType().stored());
        assertEquals(123, pointField.numericValue().doubleValue(), 0d);
        IndexableField dvField = fields[1];
        assertEquals(DocValuesType.SORTED_NUMERIC, dvField.fieldType().docValuesType());
        assertFalse(dvField.fieldType().stored());
    }

    public void testOutOfRangeValues() throws IOException {
        for (OutOfRangeSpec item : outOfRangeSpecs()) {
            DocumentMapper mapper = createDocumentMapper(fieldMapping(b -> b.field("type", item.type.typeName())));
            Exception e = expectThrows(MapperParsingException.class, () -> mapper.parse(source(item::write)));
            assertThat(
                "Incorrect error message for [" + item.type + "] with value [" + item.value + "]",
                e.getCause().getMessage(),
                containsString(item.message)
            );
        }
    }

    public void testDimension() throws IOException {
        // Test default setting
        MapperService mapperService = createMapperService(fieldMapping(b -> minimalMapping(b)));
        NumberFieldMapper.NumberFieldType ft = (NumberFieldMapper.NumberFieldType) mapperService.fieldType("field");
        assertFalse(ft.isDimension());

        // dimension = false is allowed
        assertDimension(false, NumberFieldMapper.NumberFieldType::isDimension);

        // dimension = true is not allowed
        Exception e = expectThrows(MapperParsingException.class, () -> createDocumentMapper(fieldMapping(b -> {
            minimalMapping(b);
            b.field("time_series_dimension", true);
        })));
        assertThat(e.getCause().getMessage(), containsString("Parameter [time_series_dimension] cannot be set"));
    }

    public void testMetricType() throws IOException {
        // Test default setting
        MapperService mapperService = createMapperService(fieldMapping(this::minimalMapping));
        NumberFieldMapper.NumberFieldType ft = (NumberFieldMapper.NumberFieldType) mapperService.fieldType("field");
        assertNull(ft.getMetricType());

        assertMetricType("gauge", NumberFieldMapper.NumberFieldType::getMetricType);
        assertMetricType("counter", NumberFieldMapper.NumberFieldType::getMetricType);

        {
            // Test invalid metric type for this field type
            Exception e = expectThrows(MapperParsingException.class, () -> createMapperService(fieldMapping(b -> {
                minimalMapping(b);
                b.field("time_series_metric", "histogram");
            })));
            assertThat(
                e.getCause().getMessage(),
                containsString("Unknown value [histogram] for field [time_series_metric] - accepted values are [gauge, counter]")
            );
        }
        {
            // Test invalid metric type for this field type
            Exception e = expectThrows(MapperParsingException.class, () -> createMapperService(fieldMapping(b -> {
                minimalMapping(b);
                b.field("time_series_metric", "unknown");
            })));
            assertThat(
                e.getCause().getMessage(),
                containsString("Unknown value [unknown] for field [time_series_metric] - accepted values are [gauge, counter]")
            );
        }
    }

    public void testTimeSeriesIndexDefault() throws Exception {
        var randomMetricType = randomFrom(TimeSeriesParams.MetricType.scalar());
        var indexSettings = getIndexSettingsBuilder().put(IndexSettings.MODE.getKey(), IndexMode.TIME_SERIES.getName())
            .put(IndexMetadata.INDEX_ROUTING_PATH.getKey(), "dimension_field");
        var mapperService = createMapperService(indexSettings.build(), fieldMapping(b -> {
            minimalMapping(b);
            b.field("time_series_metric", randomMetricType.toString());
        }));
        var ft = (NumberFieldMapper.NumberFieldType) mapperService.fieldType("field");
        assertThat(ft.getMetricType(), equalTo(randomMetricType));
        assertThat(ft.isIndexed(), is(false));
    }

    public void testMetricAndDocvalues() {
        Exception e = expectThrows(MapperParsingException.class, () -> createDocumentMapper(fieldMapping(b -> {
            minimalMapping(b);
            b.field("time_series_metric", "counter").field("doc_values", false);
        })));
        assertThat(e.getCause().getMessage(), containsString("Field [time_series_metric] requires that [doc_values] is true"));
    }

    @Override
    protected final Object generateRandomInputValue(MappedFieldType ft) {
        Number n = randomNumber();
        return randomBoolean() ? n : n.toString();
    }

    @Override
    protected IngestScriptSupport ingestScriptSupport() {
        return new IngestScriptSupport() {
            @Override
            @SuppressWarnings("unchecked")
            protected <T> T compileOtherScript(Script script, ScriptContext<T> context) {
                if (context == LongFieldScript.CONTEXT) {
                    return (T) LongFieldScript.PARSE_FROM_SOURCE;
                }
                if (context == DoubleFieldScript.CONTEXT) {
                    return (T) DoubleFieldScript.PARSE_FROM_SOURCE;
                }
                throw new UnsupportedOperationException("Unknown script " + script.getIdOrCode());
            }

            @Override
            ScriptFactory emptyFieldScript() {
                return null;
            }

            @Override
            ScriptFactory nonEmptyFieldScript() {
                return null;
            }
        };
    }

    public void testScriptableTypes() throws IOException {
        if (allowsIndexTimeScript()) {
            createDocumentMapper(fieldMapping(b -> {
                minimalMapping(b);
                b.field("script", "foo");
            }));
        } else {
            Exception e = expectThrows(MapperParsingException.class, () -> createDocumentMapper(fieldMapping(b -> {
                minimalMapping(b);
                b.field("script", "foo");
            })));
            assertEquals("Failed to parse mapping: Unknown parameter [script] for mapper [field]", e.getMessage());
        }
    }

    public void testAllowMultipleValuesField() throws IOException {
        MapperService mapperService = createMapperService(fieldMapping(b -> minimalMapping(b)));

        Mapper mapper = mapperService.mappingLookup().getMapper("field");
        if (mapper instanceof NumberFieldMapper numberFieldMapper) {
            numberFieldMapper.setAllowMultipleValues(false);
        } else {
            fail("mapper [" + mapper.getClass() + "] error, not number field");
        }

        Exception e = expectThrows(
            MapperParsingException.class,
            () -> mapperService.documentMapper().parse(source(b -> b.array("field", randomNumber(), randomNumber(), randomNumber())))
        );
        assertThat(e.getCause().getMessage(), containsString("Only one field can be stored per key"));
    }

    protected abstract Number randomNumber();

    protected final class NumberSyntheticSourceSupport implements SyntheticSourceSupport {
        private final Long nullValue = usually() ? null : randomNumber().longValue();
        private final boolean coerce = rarely();

        private final Function<Number, Number> round;
        private final boolean ignoreMalformed;

        protected NumberSyntheticSourceSupport(Function<Number, Number> round, boolean ignoreMalformed) {
            this.round = round;
            this.ignoreMalformed = ignoreMalformed;
        }

        @Override
        public SyntheticSourceExample example(int maxVals) {
            if (randomBoolean()) {
                Tuple<Object, Object> v = generateValue();
                if (v.v2()instanceof Number n) {
                    return new SyntheticSourceExample(v.v1(), round.apply(n), this::mapping);
                }
                // ignore_malformed value
                return new SyntheticSourceExample(v.v1(), v.v2(), this::mapping);
            }
            List<Tuple<Object, Object>> values = randomList(1, maxVals, this::generateValue);
            List<Object> in = values.stream().map(Tuple::v1).toList();
            List<Object> outList = values.stream()
                .filter(v -> v.v2() instanceof Number)
                .map(t -> round.apply((Number) t.v2()))
                .sorted()
                .collect(Collectors.toCollection(ArrayList::new));
            values.stream().filter(v -> false == v.v2() instanceof Number).map(v -> v.v2()).forEach(outList::add);
            Object out = outList.size() == 1 ? outList.get(0) : outList;
            return new SyntheticSourceExample(in, out, this::mapping);
        }

        private Tuple<Object, Object> generateValue() {
            if (ignoreMalformed && randomBoolean()) {
                List<Supplier<Object>> choices = List.of(() -> "a" + randomAlphaOfLength(3), ESTestCase::randomBoolean);
                Object v = randomFrom(choices).get();
                return Tuple.tuple(v, v);
            }
            if (nullValue != null && randomBoolean()) {
                return Tuple.tuple(null, nullValue);
            }
            Number n = randomNumber();
            Object in = n;
            Number out = n;
            if (coerce && randomBoolean()) {
                in = in.toString();
            }
            return Tuple.tuple(in, out);
        }

        private void mapping(XContentBuilder b) throws IOException {
            minimalMapping(b);
            if (coerce) {
                b.field("coerce", true);
            }
            if (nullValue != null) {
                b.field("null_value", nullValue);
            }
            if (ignoreMalformed) {
                b.field("ignore_malformed", true);
            }
        }

        @Override
        public List<SyntheticSourceInvalidExample> invalidExample() throws IOException {
            return List.of(
                new SyntheticSourceInvalidExample(
                    matchesPattern("field \\[field] of type \\[.+] doesn't support synthetic source because it doesn't have doc values"),
                    b -> {
                        minimalMapping(b);
                        b.field("doc_values", false);
                    }
                )
            );
        }
    }
}
