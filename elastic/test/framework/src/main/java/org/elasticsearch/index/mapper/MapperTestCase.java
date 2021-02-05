/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.apache.lucene.index.DocValuesType;
import org.apache.lucene.index.IndexOptions;
import org.apache.lucene.index.IndexableField;
import org.apache.lucene.index.IndexableFieldType;
import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.search.DocValuesFieldExistsQuery;
import org.apache.lucene.search.IndexSearcher;
import org.apache.lucene.search.NormsFieldExistsQuery;
import org.apache.lucene.search.Query;
import org.apache.lucene.search.TermQuery;
import org.apache.lucene.util.SetOnce;
import org.elasticsearch.Version;
import org.elasticsearch.common.CheckedConsumer;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.index.fielddata.IndexFieldData;
import org.elasticsearch.index.fielddata.IndexFieldDataCache;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.indices.breaker.NoneCircuitBreakerService;
import org.elasticsearch.search.DocValueFormat;
import org.elasticsearch.search.lookup.SearchLookup;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.BiFunction;
import java.util.function.Consumer;
import java.util.function.Supplier;

import static org.hamcrest.Matchers.anyOf;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.instanceOf;

/**
 * Base class for testing {@link Mapper}s.
 */
public abstract class MapperTestCase extends MapperServiceTestCase {
    protected abstract void minimalMapping(XContentBuilder b) throws IOException;

    /**
     * Writes the field and a sample value for it to the provided {@link XContentBuilder}.
     * To be overridden in case the field should not be written at all in documents,
     * like in the case of runtime fields.
     */
    protected void writeField(XContentBuilder builder) throws IOException {
        builder.field("field");
        builder.value(getSampleValueForDocument());
    }

    /**
     * Returns a sample value for the field, to be used in a document
     */
    protected abstract Object getSampleValueForDocument();

    /**
     * Returns a sample value for the field, to be used when querying the field. Normally this is the same format as
     * what is indexed as part of a document, and returned by {@link #getSampleValueForDocument()}, but there
     * are cases where fields are queried differently frow how they are indexed e.g. token_count or runtime fields
     */
    protected Object getSampleValueForQuery() {
        return getSampleValueForDocument();
    }

    /**
     * This test verifies that the exists query created is the appropriate one, and aligns with the data structures
     * being created for a document with a value for the field. This can only be verified for the minimal mapping.
     * Field types that allow configurable doc_values or norms should write their own tests that creates the different
     * mappings combinations and invoke {@link #assertExistsQuery(MapperService)} to verify the behaviour.
     */
    public final void testExistsQueryMinimalMapping() throws IOException {
        MapperService mapperService = createMapperService(fieldMapping(this::minimalMapping));
        assertExistsQuery(mapperService);
        assertParseMinimalWarnings();
    }

    protected void assertExistsQuery(MapperService mapperService) throws IOException {
        ParseContext.Document fields = mapperService.documentMapper().parse(source(this::writeField)).rootDoc();
        SearchExecutionContext searchExecutionContext = createSearchExecutionContext(mapperService);
        MappedFieldType fieldType = mapperService.fieldType("field");
        Query query = fieldType.existsQuery(searchExecutionContext);
        assertExistsQuery(fieldType, query, fields);
    }

    protected void assertExistsQuery(MappedFieldType fieldType, Query query, ParseContext.Document fields) {
        if (fieldType.hasDocValues()) {
            assertThat(query, instanceOf(DocValuesFieldExistsQuery.class));
            DocValuesFieldExistsQuery fieldExistsQuery = (DocValuesFieldExistsQuery) query;
            assertEquals("field", fieldExistsQuery.getField());
            assertDocValuesField(fields, "field");
            assertNoFieldNamesField(fields);
        } else if (fieldType.getTextSearchInfo().hasNorms()) {
            assertThat(query, instanceOf(NormsFieldExistsQuery.class));
            NormsFieldExistsQuery normsFieldExistsQuery = (NormsFieldExistsQuery) query;
            assertEquals("field", normsFieldExistsQuery.getField());
            assertHasNorms(fields, "field");
            assertNoDocValuesField(fields, "field");
            assertNoFieldNamesField(fields);
        } else {
            assertThat(query, instanceOf(TermQuery.class));
            TermQuery termQuery = (TermQuery) query;
            assertEquals(FieldNamesFieldMapper.NAME, termQuery.getTerm().field());
            //we always perform a term query against _field_names, even when the field
            // is not added to _field_names because it is not indexed nor stored
            assertEquals("field", termQuery.getTerm().text());
            assertNoDocValuesField(fields, "field");
            if (fieldType.isSearchable() || fieldType.isStored()) {
                assertNotNull(fields.getField(FieldNamesFieldMapper.NAME));
            } else {
                assertNoFieldNamesField(fields);
            }
        }
    }

    protected static void assertNoFieldNamesField(ParseContext.Document fields) {
        assertNull(fields.getField(FieldNamesFieldMapper.NAME));
    }

    protected static void assertHasNorms(ParseContext.Document doc, String field) {
        IndexableField[] fields = doc.getFields(field);
        for (IndexableField indexableField : fields) {
            IndexableFieldType indexableFieldType = indexableField.fieldType();
            if (indexableFieldType.indexOptions() != IndexOptions.NONE) {
                assertFalse(indexableFieldType.omitNorms());
                return;
            }
        }
        fail("field [" + field + "] should be indexed but it isn't");
    }

    protected static void assertDocValuesField(ParseContext.Document doc, String field) {
        IndexableField[] fields = doc.getFields(field);
        for (IndexableField indexableField : fields) {
            if (indexableField.fieldType().docValuesType().equals(DocValuesType.NONE) == false) {
                return;
            }
        }
        fail("doc_values not present for field [" + field + "]");
    }

    protected static void assertNoDocValuesField(ParseContext.Document doc, String field) {
        IndexableField[] fields = doc.getFields(field);
        for (IndexableField indexableField : fields) {
            assertEquals(DocValuesType.NONE, indexableField.fieldType().docValuesType());
        }
    }

    public final void testEmptyName() {
        MapperParsingException e = expectThrows(MapperParsingException.class, () -> createMapperService(mapping(b -> {
            b.startObject("");
            minimalMapping(b);
            b.endObject();
        })));
        assertThat(e.getMessage(), containsString("name cannot be empty string"));
        assertParseMinimalWarnings();
    }

    public final void testMinimalSerializesToItself() throws IOException {
        XContentBuilder orig = JsonXContent.contentBuilder().startObject();
        createMapperService(fieldMapping(this::minimalMapping)).documentMapper().mapping().toXContent(orig, ToXContent.EMPTY_PARAMS);
        orig.endObject();
        XContentBuilder parsedFromOrig = JsonXContent.contentBuilder().startObject();
        createMapperService(orig).documentMapper().mapping().toXContent(parsedFromOrig, ToXContent.EMPTY_PARAMS);
        parsedFromOrig.endObject();
        assertEquals(Strings.toString(orig), Strings.toString(parsedFromOrig));
        assertParseMinimalWarnings();
    }

    // TODO make this final once we remove FieldMapperTestCase2
    public void testMinimalToMaximal() throws IOException {
        XContentBuilder orig = JsonXContent.contentBuilder().startObject();
        createMapperService(fieldMapping(this::minimalMapping)).documentMapper().mapping().toXContent(orig, INCLUDE_DEFAULTS);
        orig.endObject();
        XContentBuilder parsedFromOrig = JsonXContent.contentBuilder().startObject();
        createMapperService(orig).documentMapper().mapping().toXContent(parsedFromOrig, INCLUDE_DEFAULTS);
        parsedFromOrig.endObject();
        assertEquals(Strings.toString(orig), Strings.toString(parsedFromOrig));
        assertParseMaximalWarnings();
    }

    protected final void assertParseMinimalWarnings() {
        String[] warnings = getParseMinimalWarnings();
        if (warnings.length > 0) {
            assertWarnings(warnings);
        }
    }

    protected final void assertParseMaximalWarnings() {
        String[] warnings = getParseMaximalWarnings();
        if (warnings.length > 0) {
            assertWarnings(warnings);
        }
    }

    protected String[] getParseMinimalWarnings() {
        // Most mappers don't emit any warnings
        return Strings.EMPTY_ARRAY;
    }

    protected String[] getParseMaximalWarnings() {
        // Most mappers don't emit any warnings
        return Strings.EMPTY_ARRAY;
    }

    /**
     * Override to disable testing {@code meta} in fields that don't support it.
     */
    protected boolean supportsMeta() {
        return true;
    }

    protected void metaMapping(XContentBuilder b) throws IOException {
        minimalMapping(b);
    }

    public final void testMeta() throws IOException {
        assumeTrue("Field doesn't support meta", supportsMeta());
        XContentBuilder mapping = fieldMapping(
            b -> {
                metaMapping(b);
                b.field("meta", Collections.singletonMap("foo", "bar"));
            }
        );
        MapperService mapperService = createMapperService(mapping);
        assertEquals(
            XContentHelper.convertToMap(BytesReference.bytes(mapping), false, mapping.contentType()).v2(),
            XContentHelper.convertToMap(mapperService.documentMapper().mappingSource().uncompressed(), false, mapping.contentType()).v2()
        );

        mapping = fieldMapping(this::metaMapping);
        merge(mapperService, mapping);
        assertEquals(
            XContentHelper.convertToMap(BytesReference.bytes(mapping), false, mapping.contentType()).v2(),
            XContentHelper.convertToMap(mapperService.documentMapper().mappingSource().uncompressed(), false, mapping.contentType()).v2()
        );

        mapping = fieldMapping(b -> {
            metaMapping(b);
            b.field("meta", Collections.singletonMap("baz", "quux"));
        });
        merge(mapperService, mapping);
        assertEquals(
            XContentHelper.convertToMap(BytesReference.bytes(mapping), false, mapping.contentType()).v2(),
            XContentHelper.convertToMap(mapperService.documentMapper().mappingSource().uncompressed(), false, mapping.contentType()).v2()
        );
    }

    public final void testDeprecatedBoost() throws IOException {
        try {
            createMapperService(Version.V_7_10_0, fieldMapping(b -> {
                minimalMapping(b);
                b.field("boost", 2.0);
            }));
            String[] warnings = Strings.concatStringArrays(getParseMinimalWarnings(),
                new String[]{"Parameter [boost] on field [field] is deprecated and has no effect"});
            assertWarnings(warnings);
        } catch (MapperParsingException e) {
            assertThat(e.getMessage(), anyOf(
                containsString("Unknown parameter [boost]"),
                containsString("[boost : 2.0]")));
        }

        MapperParsingException e
            = expectThrows(MapperParsingException.class, () -> createMapperService(Version.V_8_0_0, fieldMapping(b -> {
            minimalMapping(b);
            b.field("boost", 2.0);
        })));
        assertThat(e.getMessage(), anyOf(
            containsString("Unknown parameter [boost]"),
            containsString("[boost : 2.0]")));

        assertParseMinimalWarnings();
    }

    /**
     * Use a {@linkplain ValueFetcher} to extract values from doc values.
     */
    protected final List<?> fetchFromDocValues(MapperService mapperService, MappedFieldType ft, DocValueFormat format, Object sourceValue)
        throws IOException {

        BiFunction<MappedFieldType, Supplier<SearchLookup>, IndexFieldData<?>> fieldDataLookup = (mft, lookupSource) -> mft
            .fielddataBuilder("test", () -> {
                throw new UnsupportedOperationException();
            })
            .build(new IndexFieldDataCache.None(), new NoneCircuitBreakerService());
        SetOnce<List<?>> result = new SetOnce<>();
        withLuceneIndex(mapperService, iw -> {
            iw.addDocument(mapperService.documentMapper().parse(source(b -> b.field(ft.name(), sourceValue))).rootDoc());
        }, iw -> {
            SearchLookup lookup = new SearchLookup(mapperService::fieldType, fieldDataLookup);
            ValueFetcher valueFetcher = new DocValueFetcher(format, lookup.getForField(ft));
            IndexSearcher searcher = newSearcher(iw);
            LeafReaderContext context = searcher.getIndexReader().leaves().get(0);
            lookup.source().setSegmentAndDocument(context, 0);
            valueFetcher.setNextReader(context);
            result.set(valueFetcher.fetchValues(lookup.source(), Collections.emptySet()));
        });
        return result.get();
    }

    private class UpdateCheck {
        final XContentBuilder init;
        final XContentBuilder update;
        final Consumer<FieldMapper> check;

        private UpdateCheck(CheckedConsumer<XContentBuilder, IOException> update,
                            Consumer<FieldMapper> check) throws IOException {
            this.init = fieldMapping(MapperTestCase.this::minimalMapping);
            this.update = fieldMapping(b -> {
                minimalMapping(b);
                update.accept(b);
            });
            this.check = check;
        }

        private UpdateCheck(CheckedConsumer<XContentBuilder, IOException> init,
                            CheckedConsumer<XContentBuilder, IOException> update,
                            Consumer<FieldMapper> check) throws IOException {
            this.init = fieldMapping(init);
            this.update = fieldMapping(update);
            this.check = check;
        }
    }

    private static class ConflictCheck {
        final XContentBuilder init;
        final XContentBuilder update;

        private ConflictCheck(XContentBuilder init, XContentBuilder update) {
            this.init = init;
            this.update = update;
        }
    }

    public class ParameterChecker {

        List<UpdateCheck> updateChecks = new ArrayList<>();
        Map<String, ConflictCheck> conflictChecks = new HashMap<>();

        /**
         * Register a check that a parameter can be updated, using the minimal mapping as a base
         *
         * @param update a field builder applied on top of the minimal mapping
         * @param check  a check that the updated parameter has been applied to the FieldMapper
         */
        public void registerUpdateCheck(CheckedConsumer<XContentBuilder, IOException> update,
                                        Consumer<FieldMapper> check) throws IOException {
            updateChecks.add(new UpdateCheck(update, check));
        }

        /**
         * Register a check that a parameter can be updated
         *
         * @param init   the initial mapping
         * @param update the updated mapping
         * @param check  a check that the updated parameter has been applied to the FieldMapper
         */
        public void registerUpdateCheck(CheckedConsumer<XContentBuilder, IOException> init,
                                        CheckedConsumer<XContentBuilder, IOException> update,
                                        Consumer<FieldMapper> check) throws IOException {
            updateChecks.add(new UpdateCheck(init, update, check));
        }

        /**
         * Register a check that a parameter update will cause a conflict, using the minimal mapping as a base
         *
         * @param param  the parameter name, expected to appear in the error message
         * @param update a field builder applied on top of the minimal mapping
         */
        public void registerConflictCheck(String param, CheckedConsumer<XContentBuilder, IOException> update) throws IOException {
            conflictChecks.put(param, new ConflictCheck(
                fieldMapping(MapperTestCase.this::minimalMapping),
                fieldMapping(b -> {
                    minimalMapping(b);
                    update.accept(b);
                })
            ));
        }

        /**
         * Register a check that a parameter update will cause a conflict
         *
         * @param param  the parameter name, expected to appear in the error message
         * @param init   the initial mapping
         * @param update the updated mapping
         */
        public void registerConflictCheck(String param, XContentBuilder init, XContentBuilder update) {
            conflictChecks.put(param, new ConflictCheck(init, update));
        }
    }

    protected abstract void registerParameters(ParameterChecker checker) throws IOException;

    public void testUpdates() throws IOException {
        ParameterChecker checker = new ParameterChecker();
        registerParameters(checker);
        for (UpdateCheck updateCheck : checker.updateChecks) {
            MapperService mapperService = createMapperService(updateCheck.init);
            merge(mapperService, updateCheck.update);
            FieldMapper mapper = (FieldMapper) mapperService.documentMapper().mappers().getMapper("field");
            updateCheck.check.accept(mapper);
            // do it again to ensure that we don't get conflicts the second time
            merge(mapperService, updateCheck.update);
            mapper = (FieldMapper) mapperService.documentMapper().mappers().getMapper("field");
            updateCheck.check.accept(mapper);

        }
        for (String param : checker.conflictChecks.keySet()) {
            MapperService mapperService = createMapperService(checker.conflictChecks.get(param).init);
            // merging the same change is fine
            merge(mapperService, checker.conflictChecks.get(param).init);
            // merging the conflicting update should throw an exception
            Exception e = expectThrows(IllegalArgumentException.class,
                "No conflict when updating parameter [" + param + "]",
                () -> merge(mapperService, checker.conflictChecks.get(param).update));
            assertThat(e.getMessage(), anyOf(
                containsString("Cannot update parameter [" + param + "]"),
                containsString("different [" + param + "]")));
        }
        assertParseMaximalWarnings();
    }

    public final void testTextSearchInfoConsistency() throws IOException {
        MapperService mapperService = createMapperService(fieldMapping(this::minimalMapping));
        MappedFieldType fieldType = mapperService.fieldType("field");
        if (fieldType.getTextSearchInfo() == TextSearchInfo.NONE) {
            expectThrows(IllegalArgumentException.class, () -> fieldType.termQuery(null, null));
        } else {
            SearchExecutionContext searchExecutionContext = createSearchExecutionContext(mapperService);
            assertNotNull(fieldType.termQuery(getSampleValueForQuery(), searchExecutionContext));
        }
        assertSearchable(fieldType);
        assertParseMinimalWarnings();
    }

    protected void assertSearchable(MappedFieldType fieldType) {
        assertEquals(fieldType.isSearchable(), fieldType.getTextSearchInfo() != TextSearchInfo.NONE);
    }
}
