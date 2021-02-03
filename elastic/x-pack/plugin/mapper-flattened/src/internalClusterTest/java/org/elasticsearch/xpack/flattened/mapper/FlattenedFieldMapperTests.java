/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.flattened.mapper;

import org.apache.lucene.index.DocValuesType;
import org.apache.lucene.index.IndexableField;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.mapper.DocumentMapper;
import org.elasticsearch.index.mapper.FieldMapper;
import org.elasticsearch.index.mapper.FieldNamesFieldMapper;
import org.elasticsearch.index.mapper.MapperParsingException;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.index.mapper.MapperTestCase;
import org.elasticsearch.index.mapper.ParsedDocument;
import org.elasticsearch.index.mapper.SourceToParse;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.xpack.flattened.FlattenedMapperPlugin;
import org.elasticsearch.xpack.flattened.mapper.FlattenedFieldMapper.KeyedFlattenedFieldType;
import org.elasticsearch.xpack.flattened.mapper.FlattenedFieldMapper.RootFlattenedFieldType;

import java.io.IOException;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.Map;

import static org.apache.lucene.analysis.BaseTokenStreamTestCase.assertTokenStreamContents;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;

public class FlattenedFieldMapperTests extends MapperTestCase {

    @Override
    protected void minimalMapping(XContentBuilder b) throws IOException {
        b.field("type", "flattened");
    }

    @Override
    protected Object getSampleValueForDocument() {
        return Map.of("key", "value");
    }

    @Override
    protected void registerParameters(ParameterChecker checker) throws IOException {
        checker.registerConflictCheck("doc_values", b -> b.field("doc_values", false));
        checker.registerConflictCheck("index", b -> b.field("index", false));
        checker.registerConflictCheck("index_options", b -> b.field("index_options", "freqs"));
        checker.registerConflictCheck("null_value", b -> b.field("null_value", "foo"));
        checker.registerConflictCheck("similarity", b -> b.field("similarity", "boolean"));

        checker.registerUpdateCheck(b -> b.field("eager_global_ordinals", true),
            m -> assertTrue(m.fieldType().eagerGlobalOrdinals()));
        checker.registerUpdateCheck(b -> b.field("ignore_above", 256),
            m -> assertEquals(256, ((FlattenedFieldMapper)m).ignoreAbove()));
        checker.registerUpdateCheck(b -> b.field("split_queries_on_whitespace", true),
            m -> assertEquals("_whitespace", m.fieldType().getTextSearchInfo().getSearchAnalyzer().name()));
        checker.registerUpdateCheck(b -> b.field("depth_limit", 10),
            m -> assertEquals(10, ((FlattenedFieldMapper)m).depthLimit()));
    }

    @Override
    protected Collection<Plugin> getPlugins() {
        return Collections.singleton(new FlattenedMapperPlugin());
    }

    public void testDefaults() throws Exception {
        DocumentMapper mapper = createDocumentMapper(fieldMapping(this::minimalMapping));
        ParsedDocument parsedDoc = mapper.parse(source(b -> b.startObject("field").field("key", "value").endObject()));

        // Check the root fields.
        IndexableField[] fields = parsedDoc.rootDoc().getFields("field");
        assertEquals(2, fields.length);

        assertEquals("field", fields[0].name());
        assertEquals(new BytesRef("value"), fields[0].binaryValue());
        assertFalse(fields[0].fieldType().stored());
        assertTrue(fields[0].fieldType().omitNorms());
        assertEquals(DocValuesType.NONE, fields[0].fieldType().docValuesType());

        assertEquals("field", fields[1].name());
        assertEquals(new BytesRef("value"), fields[1].binaryValue());
        assertEquals(DocValuesType.SORTED_SET, fields[1].fieldType().docValuesType());

        // Check the keyed fields.
        IndexableField[] keyedFields = parsedDoc.rootDoc().getFields("field._keyed");
        assertEquals(2, keyedFields.length);

        assertEquals("field._keyed", keyedFields[0].name());
        assertEquals(new BytesRef("key\0value"), keyedFields[0].binaryValue());
        assertFalse(keyedFields[0].fieldType().stored());
        assertTrue(keyedFields[0].fieldType().omitNorms());
        assertEquals(DocValuesType.NONE, keyedFields[0].fieldType().docValuesType());

        assertEquals("field._keyed", keyedFields[1].name());
        assertEquals(new BytesRef("key\0value"), keyedFields[1].binaryValue());
        assertEquals(DocValuesType.SORTED_SET, keyedFields[1].fieldType().docValuesType());

        // Check that there is no 'field names' field.
        IndexableField[] fieldNamesFields = parsedDoc.rootDoc().getFields(FieldNamesFieldMapper.NAME);
        assertEquals(0, fieldNamesFields.length);
    }

    public void testDisableIndex() throws Exception {

        DocumentMapper mapper = createDocumentMapper(fieldMapping(b -> {
            b.field("type", "flattened");
            b.field("index", false);
        }));
        ParsedDocument parsedDoc = mapper.parse(source(b -> b.startObject("field").field("key", "value").endObject()));

        IndexableField[] fields = parsedDoc.rootDoc().getFields("field");
        assertEquals(1, fields.length);
        assertEquals(DocValuesType.SORTED_SET, fields[0].fieldType().docValuesType());

        IndexableField[] keyedFields = parsedDoc.rootDoc().getFields("field._keyed");
        assertEquals(1, keyedFields.length);
        assertEquals(DocValuesType.SORTED_SET, keyedFields[0].fieldType().docValuesType());
    }

    public void testDisableDocValues() throws Exception {

        DocumentMapper mapper = createDocumentMapper(fieldMapping(b -> {
            b.field("type", "flattened");
            b.field("doc_values", false);
        }));
        ParsedDocument parsedDoc = mapper.parse(source(b -> b.startObject("field").field("key", "value").endObject()));

        IndexableField[] fields = parsedDoc.rootDoc().getFields("field");
        assertEquals(1, fields.length);
        assertEquals(DocValuesType.NONE, fields[0].fieldType().docValuesType());

        IndexableField[] keyedFields = parsedDoc.rootDoc().getFields("field._keyed");
        assertEquals(1, keyedFields.length);
        assertEquals(DocValuesType.NONE, keyedFields[0].fieldType().docValuesType());

        IndexableField[] fieldNamesFields = parsedDoc.rootDoc().getFields(FieldNamesFieldMapper.NAME);
        assertEquals(1, fieldNamesFields.length);
        assertEquals("field", fieldNamesFields[0].stringValue());
    }

    public void testIndexOptions() throws IOException {

        createDocumentMapper(fieldMapping(b -> {
            b.field("type", "flattened");
            b.field("index_options", "freqs");
        }));

        for (String indexOptions : Arrays.asList("positions", "offsets")) {
            Exception e = expectThrows(MapperParsingException.class,
                () -> createDocumentMapper(fieldMapping(b -> {
                        b.field("type", "flattened");
                        b.field("index_options", indexOptions);
                    })));
            assertThat(e.getMessage(), containsString("Unknown value [" + indexOptions
                + "] for field [index_options] - accepted values are [docs, freqs]"));
        }
    }

    public void testNullField() throws Exception {
        DocumentMapper mapper = createDocumentMapper(fieldMapping(this::minimalMapping));
        ParsedDocument parsedDoc = mapper.parse(source(b -> b.nullField("field")));
        IndexableField[] fields = parsedDoc.rootDoc().getFields("field");
        assertEquals(0, fields.length);
    }

    public void testMalformedJson() throws Exception {
        DocumentMapper mapper = createDocumentMapper(fieldMapping(this::minimalMapping));

        expectThrows(MapperParsingException.class, () -> mapper.parse(source(b -> b.field("field", "not a JSON object"))));

        BytesReference doc2 = new BytesArray("{ \"field\": { \"key\": \"value\" ");
        expectThrows(MapperParsingException.class, () -> mapper.parse(
            new SourceToParse("test", "1", doc2, XContentType.JSON)));
    }

    public void testFieldMultiplicity() throws Exception {

        DocumentMapper mapper = createDocumentMapper(fieldMapping(this::minimalMapping));

        ParsedDocument parsedDoc = mapper.parse(source(b -> {
            b.startArray("field");
            {
                b.startObject().field("key1", "value").endObject();
                b.startObject();
                {
                    b.field("key2", true);
                    b.field("key3", false);
                }
                b.endObject();
            }
            b.endArray();
        }));

        IndexableField[] fields = parsedDoc.rootDoc().getFields("field");
        assertEquals(6, fields.length);
        assertEquals(new BytesRef("value"), fields[0].binaryValue());
        assertEquals(new BytesRef("true"), fields[2].binaryValue());
        assertEquals(new BytesRef("false"), fields[4].binaryValue());

        IndexableField[] keyedFields = parsedDoc.rootDoc().getFields("field._keyed");
        assertEquals(6, keyedFields.length);
        assertEquals(new BytesRef("key1\0value"), keyedFields[0].binaryValue());
        assertEquals(new BytesRef("key2\0true"), keyedFields[2].binaryValue());
        assertEquals(new BytesRef("key3\0false"), keyedFields[4].binaryValue());
    }

    public void testDepthLimit() throws IOException {
        // First verify the default behavior when depth_limit is not set.
        MapperService mapperService = createMapperService(fieldMapping(this::minimalMapping));

        mapperService.documentMapper().parse(source(b -> {
            b.startObject("field");
            {
                b.startObject("key1");
                {
                    b.startObject("key2").field("key3", "value").endObject();
                }
                b.endObject();
            }
            b.endObject();
        }));

        // Set a lower value for depth_limit and check that the field is rejected.
        merge(mapperService, fieldMapping(b -> {
            b.field("type", "flattened");
            b.field("depth_limit", 2);
        }));

        expectThrows(MapperParsingException.class, () ->
            mapperService.documentMapper().parse(source(b -> {
                b.startObject("field");
                {
                    b.startObject("key1");
                    {
                        b.startObject("key2").field("key3", "value").endObject();
                    }
                    b.endObject();
                }
                b.endObject();
            })));
    }

    public void testEagerGlobalOrdinals() throws IOException {

        DocumentMapper defMapper = createDocumentMapper(fieldMapping(this::minimalMapping));
        FieldMapper fieldMapper = (FieldMapper) defMapper.mappers().getMapper("field");
        assertFalse(fieldMapper.fieldType().eagerGlobalOrdinals());

        DocumentMapper mapper = createDocumentMapper(fieldMapping(b -> {
            b.field("type", "flattened");
            b.field("eager_global_ordinals", true);
        }));

        fieldMapper = (FieldMapper) mapper.mappers().getMapper("field");
        assertTrue(fieldMapper.fieldType().eagerGlobalOrdinals());
    }

    public void testIgnoreAbove() throws IOException {
        // First verify the default behavior when ignore_above is not set.
        DocumentMapper mapper = createDocumentMapper(fieldMapping(this::minimalMapping));

        ParsedDocument parsedDoc = mapper.parse(source(b -> {
            b.startArray("field");
            {
                b.startObject().field("key", "a longer then usual value").endObject();
            }
            b.endArray();
        }));
        IndexableField[] fields = parsedDoc.rootDoc().getFields("field");
        assertEquals(2, fields.length);

        // Set a lower value for ignore_above and check that the field is skipped.
        DocumentMapper newMapper = createDocumentMapper(fieldMapping(b -> {
            b.field("type", "flattened");
            b.field("ignore_above", 10);
        }));

        ParsedDocument newParsedDoc = newMapper.parse(source(b -> {
            b.startArray("field");
            {
                b.startObject().field("key", "a longer then usual value").endObject();
            }
            b.endArray();
        }));
        IndexableField[] newFields = newParsedDoc.rootDoc().getFields("field");
        assertEquals(0, newFields.length);
    }

    public void testNullValues() throws Exception {
        DocumentMapper mapper = createDocumentMapper(mapping(b -> {
            b.startObject("field").field("type", "flattened").endObject();
            b.startObject("other_field");
            {
                b.field("type", "flattened");
                b.field("null_value", "placeholder");
            }
            b.endObject();
        }));

        ParsedDocument parsedDoc = mapper.parse(source(b -> {
            b.startObject("field").nullField("key").endObject();
            b.startObject("other_field").nullField("key").endObject();
        }));

        IndexableField[] fields = parsedDoc.rootDoc().getFields("field");
        assertEquals(0, fields.length);

        IndexableField[] otherFields = parsedDoc.rootDoc().getFields("other_field");
        assertEquals(2, otherFields.length);
        assertEquals(new BytesRef("placeholder"), otherFields[0].binaryValue());
        assertEquals(new BytesRef("placeholder"), otherFields[1].binaryValue());

        IndexableField[] prefixedOtherFields = parsedDoc.rootDoc().getFields("other_field._keyed");
        assertEquals(2, prefixedOtherFields.length);
        assertEquals(new BytesRef("key\0placeholder"), prefixedOtherFields[0].binaryValue());
        assertEquals(new BytesRef("key\0placeholder"), prefixedOtherFields[1].binaryValue());
    }

    public void testSplitQueriesOnWhitespace() throws IOException {
        MapperService mapperService = createMapperService(fieldMapping(b -> {
            b.field("type", "flattened");
            b.field("split_queries_on_whitespace", true);
        }));

        RootFlattenedFieldType rootFieldType = (RootFlattenedFieldType) mapperService.fieldType("field");
        assertThat(rootFieldType.getTextSearchInfo().getSearchAnalyzer().name(), equalTo("_whitespace"));
        assertTokenStreamContents(rootFieldType.getTextSearchInfo().getSearchAnalyzer().analyzer().tokenStream("", "Hello World"),
            new String[] {"Hello", "World"});

        KeyedFlattenedFieldType keyedFieldType = (KeyedFlattenedFieldType) mapperService.fieldType("field.key");
        assertThat(keyedFieldType.getTextSearchInfo().getSearchAnalyzer().name(), equalTo("_whitespace"));
        assertTokenStreamContents(keyedFieldType.getTextSearchInfo().getSearchAnalyzer().analyzer().tokenStream("", "Hello World"),
            new String[] {"Hello", "World"});
    }
}
