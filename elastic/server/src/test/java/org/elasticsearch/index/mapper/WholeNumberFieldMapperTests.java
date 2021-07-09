/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.apache.lucene.index.IndexableField;

import java.io.IOException;

import static org.hamcrest.Matchers.containsString;

public abstract class WholeNumberFieldMapperTests extends NumberFieldMapperTests {

    protected void testDecimalCoerce() throws IOException {
        DocumentMapper mapper = createDocumentMapper(fieldMapping(this::minimalMapping));
        ParsedDocument doc = mapper.parse(source(b -> b.field("field", "7.89")));
        IndexableField[] fields = doc.rootDoc().getFields("field");
        IndexableField pointField = fields[0];
        assertEquals(7, pointField.numericValue().doubleValue(), 0d);
    }

    @Override
    public void testDimension() throws IOException {
        // Test default setting
        MapperService mapperService = createMapperService(fieldMapping(b -> minimalMapping(b)));
        NumberFieldMapper.NumberFieldType ft = (NumberFieldMapper.NumberFieldType) mapperService.fieldType("field");
        assertFalse(ft.isDimension());

        assertDimension(true, NumberFieldMapper.NumberFieldType::isDimension);
        assertDimension(false, NumberFieldMapper.NumberFieldType::isDimension);
    }

    public void testDimensionIndexedAndDocvalues() {
        {
            Exception e = expectThrows(MapperParsingException.class, () -> createDocumentMapper(fieldMapping(b -> {
                minimalMapping(b);
                b.field("dimension", true).field("index", false).field("doc_values", false);
            })));
            assertThat(e.getCause().getMessage(),
                containsString("Field [dimension] requires that [index] and [doc_values] are true"));
        }
        {
            Exception e = expectThrows(MapperParsingException.class, () -> createDocumentMapper(fieldMapping(b -> {
                minimalMapping(b);
                b.field("dimension", true).field("index", true).field("doc_values", false);
            })));
            assertThat(e.getCause().getMessage(),
                containsString("Field [dimension] requires that [index] and [doc_values] are true"));
        }
        {
            Exception e = expectThrows(MapperParsingException.class, () -> createDocumentMapper(fieldMapping(b -> {
                minimalMapping(b);
                b.field("dimension", true).field("index", false).field("doc_values", true);
            })));
            assertThat(e.getCause().getMessage(),
                containsString("Field [dimension] requires that [index] and [doc_values] are true"));
        }
    }

    public void testDimensionMultiValuedField() throws IOException {
        DocumentMapper mapper = createDocumentMapper(fieldMapping(b -> {
            minimalMapping(b);
            b.field("dimension", true);
        }));

        Exception e = expectThrows(MapperParsingException.class,
            () -> mapper.parse(source(b -> b.array("field", randomNumber(), randomNumber(), randomNumber()))));
        assertThat(e.getCause().getMessage(),
            containsString("Dimension field [field] cannot be a multi-valued field"));
    }

    @Override
    protected void registerParameters(ParameterChecker checker) throws IOException {
        super.registerParameters(checker);

        // dimension cannot be updated
        registerDimensionChecks(checker);
    }

}
