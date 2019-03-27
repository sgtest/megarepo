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

import org.apache.lucene.document.BinaryDocValuesField;
import org.apache.lucene.index.IndexableField;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.compress.CompressedXContent;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.IndexService;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.test.ESSingleNodeTestCase;
import org.junit.Before;

import java.io.IOException;
import java.util.Collection;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.instanceOf;

public class DenseVectorFieldMapperTests extends ESSingleNodeTestCase {
    private DocumentMapper mapper;

    @Before
    public void setUpMapper() throws Exception {
        IndexService indexService =  createIndex("test-index");
        DocumentMapperParser parser = indexService.mapperService().documentMapperParser();
        String mapping = Strings.toString(XContentFactory.jsonBuilder()
            .startObject()
                .startObject("_doc")
                    .startObject("properties")
                        .startObject("my-dense-vector").field("type", "dense_vector")
                        .endObject()
                    .endObject()
                .endObject()
            .endObject());
        mapper = parser.parse("_doc", new CompressedXContent(mapping));
    }

    @Override
    protected Collection<Class<? extends Plugin>> getPlugins() {
        return pluginList(MapperExtrasPlugin.class);
    }

    public void testDefaults() throws Exception {
        float[] expectedArray = {-12.1f, 100.7f, -4};
        ParsedDocument doc1 = mapper.parse(new SourceToParse("test-index", "_doc", "1", BytesReference
            .bytes(XContentFactory.jsonBuilder()
                .startObject()
                    .startArray("my-dense-vector").value(expectedArray[0]).value(expectedArray[1]).value(expectedArray[2]).endArray()
                .endObject()),
            XContentType.JSON));
        IndexableField[] fields = doc1.rootDoc().getFields("my-dense-vector");
        assertEquals(1, fields.length);
        assertThat(fields[0], instanceOf(BinaryDocValuesField.class));

        // assert that after decoding the indexed value is equal to expected
        BytesRef vectorBR = ((BinaryDocValuesField) fields[0]).binaryValue();
        float[] decodedValues = VectorEncoderDecoder.decodeDenseVector(vectorBR);
        assertArrayEquals(
            "Decoded dense vector values is not equal to the indexed one.",
            expectedArray,
            decodedValues,
            0.001f
        );
    }

    public void testDimensionLimit() throws IOException {
        float[] validVector = new float[DenseVectorFieldMapper.MAX_DIMS_COUNT];
        BytesReference validDoc = BytesReference.bytes(
            XContentFactory.jsonBuilder().startObject()
                .array("my-dense-vector", validVector)
            .endObject());
        mapper.parse(new SourceToParse("test-index", "_doc", "1", validDoc, XContentType.JSON));

        float[] invalidVector = new float[DenseVectorFieldMapper.MAX_DIMS_COUNT + 1];
        BytesReference invalidDoc = BytesReference.bytes(
            XContentFactory.jsonBuilder().startObject()
                .array("my-dense-vector", invalidVector)
                .endObject());
        MapperParsingException e = expectThrows(MapperParsingException.class, () -> mapper.parse(
            new SourceToParse("test-index", "_doc", "1", invalidDoc, XContentType.JSON)));
        assertThat(e.getDetailedMessage(), containsString("has exceeded the maximum allowed number of dimensions"));
    }
}
