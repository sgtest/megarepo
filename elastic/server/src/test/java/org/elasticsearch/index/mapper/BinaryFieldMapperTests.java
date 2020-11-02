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

package org.elasticsearch.index.mapper;

import org.apache.lucene.index.IndexOptions;
import org.apache.lucene.index.IndexableField;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.compress.CompressorFactory;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.xcontent.XContentBuilder;

import java.io.IOException;
import java.io.OutputStream;
import java.util.Arrays;

import static org.hamcrest.Matchers.instanceOf;

public class BinaryFieldMapperTests extends MapperTestCase {

    @Override
    protected Object getSampleValueForDocument() {
        final byte[] binaryValue = new byte[100];
        binaryValue[56] = 1;
        return binaryValue;
    }

    @Override
    protected void minimalMapping(XContentBuilder b) throws IOException {
        b.field("type", "binary");
    }

    @Override
    protected void registerParameters(ParameterChecker checker) throws IOException {
        checker.registerConflictCheck("doc_values", b -> b.field("doc_values", true));
        checker.registerConflictCheck("store", b -> b.field("store", true));
    }

    public void testExistsQueryDocValuesEnabled() throws IOException {
        MapperService mapperService = createMapperService(fieldMapping(b -> {
            minimalMapping(b);
            b.field("doc_values", true);
            if (randomBoolean()) {
                b.field("store", randomBoolean());
            }
        }));
        assertExistsQuery(mapperService);
        assertParseMinimalWarnings();
    }

    public void testExistsQueryStoreEnabled() throws IOException {
        MapperService mapperService = createMapperService(fieldMapping(b -> {
            minimalMapping(b);
            b.field("store", true);
            if (randomBoolean()) {
                b.field("doc_values", false);
            }
        }));
        assertExistsQuery(mapperService);
    }

    public void testExistsQueryStoreAndDocValuesDiabled() throws IOException {
        MapperService mapperService = createMapperService(fieldMapping(b -> {
            minimalMapping(b);
            b.field("store", false);
            b.field("doc_values", false);
        }));
        assertExistsQuery(mapperService);
    }

    public void testDefaultMapping() throws Exception {
        MapperService mapperService = createMapperService(fieldMapping(this::minimalMapping));
        FieldMapper mapper = (FieldMapper) mapperService.documentMapper().mappers().getMapper("field");

        assertThat(mapper, instanceOf(BinaryFieldMapper.class));

        byte[] value = new byte[100];
        ParsedDocument doc = mapperService.documentMapper().parse(source(b -> b.field("field", value)));
        assertEquals(0, doc.rootDoc().getFields("field").length);
    }

    public void testStoredValue() throws IOException {

        MapperService mapperService = createMapperService(fieldMapping(b -> {
            minimalMapping(b);
            b.field("store", "true");
        }));

        // case 1: a simple binary value
        final byte[] binaryValue1 = new byte[100];
        binaryValue1[56] = 1;

        // case 2: a value that looks compressed: this used to fail in 1.x
        BytesStreamOutput out = new BytesStreamOutput();
        try (OutputStream compressed = CompressorFactory.COMPRESSOR.threadLocalOutputStream(out)) {
            new BytesArray(binaryValue1).writeTo(compressed);
        }
        final byte[] binaryValue2 = BytesReference.toBytes(out.bytes());
        assertTrue(CompressorFactory.isCompressed(new BytesArray(binaryValue2)));

        for (byte[] value : Arrays.asList(binaryValue1, binaryValue2)) {
            ParsedDocument doc = mapperService.documentMapper().parse(source(b -> b.field("field", value)));
            BytesRef indexedValue = doc.rootDoc().getBinaryValue("field");
            assertEquals(new BytesRef(value), indexedValue);
            IndexableField field = doc.rootDoc().getField("field");
            assertTrue(field.fieldType().stored());
            assertEquals(IndexOptions.NONE, field.fieldType().indexOptions());

            MappedFieldType fieldType = mapperService.fieldType("field");
            Object originalValue = fieldType.valueForDisplay(indexedValue);
            assertEquals(new BytesArray(value), originalValue);
        }
    }
}
