/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.constantkeyword.mapper;

import org.apache.lucene.search.MatchNoDocsQuery;
import org.apache.lucene.search.Query;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.compress.CompressedXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.index.mapper.DocumentMapper;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.index.mapper.MapperParsingException;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.index.mapper.MapperService.MergeReason;
import org.elasticsearch.index.mapper.MapperTestCase;
import org.elasticsearch.index.mapper.ParseContext;
import org.elasticsearch.index.mapper.ParsedDocument;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.xpack.constantkeyword.ConstantKeywordMapperPlugin;
import org.elasticsearch.xpack.constantkeyword.mapper.ConstantKeywordFieldMapper.ConstantKeywordFieldType;

import java.io.IOException;
import java.util.Collection;
import java.util.List;

import static org.hamcrest.Matchers.instanceOf;

public class ConstantKeywordFieldMapperTests extends MapperTestCase {

    @Override
    protected void writeField(XContentBuilder builder) {
        //do nothing
    }

    @Override
    protected Object getSampleValueForDocument() {
        throw new UnsupportedOperationException();
    }

    @Override
    protected Object getSampleValueForQuery() {
        return "test";
    }

    @Override
    protected void assertExistsQuery(MappedFieldType fieldType, Query query, ParseContext.Document fields) {
        assertThat(query, instanceOf(MatchNoDocsQuery.class));
        assertNoFieldNamesField(fields);
    }

    @Override
    protected Collection<Plugin> getPlugins() {
        return List.of(new ConstantKeywordMapperPlugin());
    }

    public void testDefaults() throws Exception {
        XContentBuilder mapping = fieldMapping(b -> b.field("type", "constant_keyword").field("value", "foo"));
        DocumentMapper mapper = createDocumentMapper(mapping);
        assertEquals(Strings.toString(mapping), mapper.mappingSource().toString());

        ParsedDocument doc = mapper.parse(source(b -> {}));
        assertNull(doc.rootDoc().getField("field"));

        doc = mapper.parse(source(b -> b.field("field", "foo")));
        assertNull(doc.rootDoc().getField("field"));

        MapperParsingException e = expectThrows(
            MapperParsingException.class,
            () -> mapper.parse(source(b -> b.field("field", "bar")))
        );
        assertEquals(
            "[constant_keyword] field [field] only accepts values that are equal to the value defined in the mappings [foo], "
                + "but got [bar]",
            e.getCause().getMessage()
        );
    }

    public void testDynamicValue() throws Exception {
        MapperService mapperService = createMapperService(fieldMapping(b -> b.field("type", "constant_keyword")));

        ParsedDocument doc = mapperService.documentMapper().parse(source(b -> b.field("field", "foo")));
        assertNull(doc.rootDoc().getField("field"));
        assertNotNull(doc.dynamicMappingsUpdate());

        CompressedXContent mappingUpdate = new CompressedXContent(Strings.toString(doc.dynamicMappingsUpdate()));
        DocumentMapper updatedMapper = mapperService.merge("_doc", mappingUpdate, MergeReason.MAPPING_UPDATE);
        String expectedMapping = Strings.toString(fieldMapping(b -> b.field("type", "constant_keyword").field("value", "foo")));
        assertEquals(expectedMapping, updatedMapper.mappingSource().toString());

        doc = updatedMapper.parse(source(b -> b.field("field", "foo")));
        assertNull(doc.rootDoc().getField("field"));
        assertNull(doc.dynamicMappingsUpdate());
    }

    public void testBadValues() {
        {
            MapperParsingException e = expectThrows(MapperParsingException.class, () -> createMapperService(fieldMapping(b -> {
                b.field("type", "constant_keyword");
                b.nullField("value");
            })));
            assertEquals(e.getMessage(),
                "Failed to parse mapping: [value] on mapper [field] of type [constant_keyword] must not have a [null] value");
        }
        {
            MapperParsingException e = expectThrows(MapperParsingException.class, () -> createMapperService(fieldMapping(b -> {
                b.field("type", "constant_keyword");
                b.startObject("value").field("foo", "bar").endObject();
            })));
            assertEquals(e.getMessage(),
                "Failed to parse mapping: Property [value] on field [field] must be a number or a string, but got [{foo=bar}]");
        }
    }

    public void testNumericValue() throws IOException {
        MapperService mapperService = createMapperService(fieldMapping(b -> {
            b.field("type", "constant_keyword");
            b.field("value", 74);
        }));
        ConstantKeywordFieldType ft
            = (ConstantKeywordFieldType) mapperService.fieldType("field");
        assertEquals("74", ft.value());
    }

    public void testUpdate() throws IOException {
        MapperService mapperService = createMapperService(fieldMapping(b -> {
            b.field("type", "constant_keyword");
            b.field("value", "foo");
        }));

        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> merge(mapperService, fieldMapping(b -> {
            b.field("type", "constant_keyword");
            b.field("value", "bar");
        })));
        assertEquals(e.getMessage(),
            "Mapper for [field] conflicts with existing mapper:\n" +
            "\tCannot update parameter [value] from [foo] to [bar]");
    }

    @Override
    protected void minimalMapping(XContentBuilder b) throws IOException {
        b.field("type", "constant_keyword");
    }

    @Override
    protected void registerParameters(ParameterChecker checker) throws IOException {
        checker.registerUpdateCheck(b -> b.field("value", "foo"), m -> {
            ConstantKeywordFieldType ft = (ConstantKeywordFieldType) m.fieldType();
            assertEquals("foo", ft.value());
        });
        checker.registerConflictCheck("value",
            fieldMapping(b -> {
                b.field("type", "constant_keyword");
                b.field("value", "foo");
            }),
            fieldMapping(b -> {
                b.field("type", "constant_keyword");
                b.field("value", "bar");
            }));
    }
}
