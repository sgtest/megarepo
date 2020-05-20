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

import com.ibm.icu.text.Collator;
import com.ibm.icu.text.RawCollationKey;
import com.ibm.icu.util.ULocale;
import org.apache.lucene.index.DocValuesType;
import org.apache.lucene.index.IndexOptions;
import org.apache.lucene.index.IndexableField;
import org.apache.lucene.index.IndexableFieldType;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.compress.CompressedXContent;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.IndexService;
import org.elasticsearch.index.mapper.MapperService.MergeReason;
import org.elasticsearch.plugin.analysis.icu.AnalysisICUPlugin;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.test.InternalSettingsPlugin;
import org.junit.Before;

import java.io.IOException;
import java.util.Arrays;
import java.util.Collection;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;

public class ICUCollationKeywordFieldMapperTests extends FieldMapperTestCase<ICUCollationKeywordFieldMapper.Builder> {

    private static final String FIELD_TYPE = "icu_collation_keyword";

    @Override
    protected Collection<Class<? extends Plugin>> getPlugins() {
        return Arrays.asList(AnalysisICUPlugin.class, InternalSettingsPlugin.class);
    }

    @Override
    protected ICUCollationKeywordFieldMapper.Builder newBuilder() {
        return new ICUCollationKeywordFieldMapper.Builder("icu");
    }

    IndexService indexService;
    DocumentMapperParser parser;

    @Before
    public void setup() {
        indexService = createIndex("test");
        parser = indexService.mapperService().documentMapperParser();
        addModifier("strength", false, (a, b) -> {
            a.strength("primary");
            b.strength("secondary");
        });
        addModifier("decomposition", false, (a, b) -> {
            a.decomposition("no");
            b.decomposition("canonical");
        });
        addModifier("alternate", false, (a, b) -> {
            a.alternate("shifted");
            b.alternate("non-ignorable");
        });
        addBooleanModifier("case_level", false, ICUCollationKeywordFieldMapper.Builder::caseLevel);
        addModifier("case_first", false, (a, b) -> {
            a.caseFirst("upper");
            a.caseFirst("lower");
        });
        addBooleanModifier("numeric", false, ICUCollationKeywordFieldMapper.Builder::numeric);
        addModifier("variable_top", false, (a, b) -> {
            a.variableTop(";");
            b.variableTop(":");
        });
        addBooleanModifier("hiragana_quaternary_mode", false, ICUCollationKeywordFieldMapper.Builder::hiraganaQuaternaryMode);
    }

    public void testDefaults() throws Exception {
        String mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("field").field("type", FIELD_TYPE).endObject().endObject()
            .endObject().endObject());

        DocumentMapper mapper = parser.parse("type", new CompressedXContent(mapping));

        assertEquals(mapping, mapper.mappingSource().toString());

        ParsedDocument doc = mapper.parse(new SourceToParse("test", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .field("field", "1234")
                        .endObject()),
            XContentType.JSON));

        IndexableField[] fields = doc.rootDoc().getFields("field");
        assertEquals(2, fields.length);

        Collator collator = Collator.getInstance(ULocale.ROOT);
        RawCollationKey key = collator.getRawCollationKey("1234", null);
        BytesRef expected = new BytesRef(key.bytes, 0, key.size);

        assertEquals(expected, fields[0].binaryValue());
        IndexableFieldType fieldType = fields[0].fieldType();
        assertThat(fieldType.omitNorms(), equalTo(true));
        assertFalse(fieldType.tokenized());
        assertFalse(fieldType.stored());
        assertThat(fieldType.indexOptions(), equalTo(IndexOptions.DOCS));
        assertThat(fieldType.storeTermVectors(), equalTo(false));
        assertThat(fieldType.storeTermVectorOffsets(), equalTo(false));
        assertThat(fieldType.storeTermVectorPositions(), equalTo(false));
        assertThat(fieldType.storeTermVectorPayloads(), equalTo(false));
        assertEquals(DocValuesType.NONE, fieldType.docValuesType());

        assertEquals(expected, fields[1].binaryValue());
        fieldType = fields[1].fieldType();
        assertThat(fieldType.indexOptions(), equalTo(IndexOptions.NONE));
        assertEquals(DocValuesType.SORTED_SET, fieldType.docValuesType());
    }

    public void testNullValue() throws IOException {
        String mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("field").field("type", FIELD_TYPE).endObject().endObject()
            .endObject().endObject());

        DocumentMapper mapper = parser.parse("type", new CompressedXContent(mapping));
        assertEquals(mapping, mapper.mappingSource().toString());

        ParsedDocument doc = mapper.parse(new SourceToParse("test", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .nullField("field")
                        .endObject()),
            XContentType.JSON));
        assertArrayEquals(new IndexableField[0], doc.rootDoc().getFields("field"));

        mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("field").field("type", FIELD_TYPE)
            .field("null_value", "1234").endObject().endObject()
            .endObject().endObject());

        mapper = parser.parse("type", new CompressedXContent(mapping));

        assertEquals(mapping, mapper.mappingSource().toString());

        doc = mapper.parse(new SourceToParse("test", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .endObject()),
            XContentType.JSON));

        IndexableField[] fields = doc.rootDoc().getFields("field");
        assertEquals(0, fields.length);

        doc = mapper.parse(new SourceToParse("test", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .nullField("field")
                        .endObject()),
            XContentType.JSON));

        Collator collator = Collator.getInstance(ULocale.ROOT);
        RawCollationKey key = collator.getRawCollationKey("1234", null);
        BytesRef expected = new BytesRef(key.bytes, 0, key.size);

        fields = doc.rootDoc().getFields("field");
        assertEquals(2, fields.length);
        assertEquals(expected, fields[0].binaryValue());
    }

    public void testEnableStore() throws IOException {
        String mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("field").field("type", FIELD_TYPE)
            .field("store", true).endObject().endObject()
            .endObject().endObject());

        DocumentMapper mapper = parser.parse("type", new CompressedXContent(mapping));

        assertEquals(mapping, mapper.mappingSource().toString());

        ParsedDocument doc = mapper.parse(new SourceToParse("test", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .field("field", "1234")
                        .endObject()),
            XContentType.JSON));

        IndexableField[] fields = doc.rootDoc().getFields("field");
        assertEquals(2, fields.length);
        assertTrue(fields[0].fieldType().stored());
    }

    public void testDisableIndex() throws IOException {
        String mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("field").field("type", FIELD_TYPE)
            .field("index", false).endObject().endObject()
            .endObject().endObject());

        DocumentMapper mapper = parser.parse("type", new CompressedXContent(mapping));

        assertEquals(mapping, mapper.mappingSource().toString());

        ParsedDocument doc = mapper.parse(new SourceToParse("test", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .field("field", "1234")
                        .endObject()),
            XContentType.JSON));

        IndexableField[] fields = doc.rootDoc().getFields("field");
        assertEquals(1, fields.length);
        assertEquals(IndexOptions.NONE, fields[0].fieldType().indexOptions());
        assertEquals(DocValuesType.SORTED_SET, fields[0].fieldType().docValuesType());
    }

    public void testDisableDocValues() throws IOException {
        String mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("field").field("type", FIELD_TYPE)
            .field("doc_values", false).endObject().endObject()
            .endObject().endObject());

        DocumentMapper mapper = parser.parse("type", new CompressedXContent(mapping));

        assertEquals(mapping, mapper.mappingSource().toString());

        ParsedDocument doc = mapper.parse(new SourceToParse("test", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .field("field", "1234")
                        .endObject()),
            XContentType.JSON));

        IndexableField[] fields = doc.rootDoc().getFields("field");
        assertEquals(1, fields.length);
        assertEquals(DocValuesType.NONE, fields[0].fieldType().docValuesType());
    }

    public void testMultipleValues() throws IOException {
        String mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("field").field("type", FIELD_TYPE).endObject().endObject()
            .endObject().endObject());

        DocumentMapper mapper = parser.parse("type", new CompressedXContent(mapping));

        assertEquals(mapping, mapper.mappingSource().toString());

        ParsedDocument doc = mapper.parse(new SourceToParse("test", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .field("field", Arrays.asList("1234", "5678"))
                        .endObject()),
            XContentType.JSON));

        IndexableField[] fields = doc.rootDoc().getFields("field");
        assertEquals(4, fields.length);

        Collator collator = Collator.getInstance(ULocale.ROOT);
        RawCollationKey key = collator.getRawCollationKey("1234", null);
        BytesRef expected = new BytesRef(key.bytes, 0, key.size);

        assertEquals(expected, fields[0].binaryValue());
        IndexableFieldType fieldType = fields[0].fieldType();
        assertThat(fieldType.omitNorms(), equalTo(true));
        assertFalse(fieldType.tokenized());
        assertFalse(fieldType.stored());
        assertThat(fieldType.indexOptions(), equalTo(IndexOptions.DOCS));
        assertThat(fieldType.storeTermVectors(), equalTo(false));
        assertThat(fieldType.storeTermVectorOffsets(), equalTo(false));
        assertThat(fieldType.storeTermVectorPositions(), equalTo(false));
        assertThat(fieldType.storeTermVectorPayloads(), equalTo(false));
        assertEquals(DocValuesType.NONE, fieldType.docValuesType());

        assertEquals(expected, fields[1].binaryValue());
        fieldType = fields[1].fieldType();
        assertThat(fieldType.indexOptions(), equalTo(IndexOptions.NONE));
        assertEquals(DocValuesType.SORTED_SET, fieldType.docValuesType());

        collator = Collator.getInstance(ULocale.ROOT);
        key = collator.getRawCollationKey("5678", null);
        expected = new BytesRef(key.bytes, 0, key.size);

        assertEquals(expected, fields[2].binaryValue());
        fieldType = fields[2].fieldType();
        assertThat(fieldType.omitNorms(), equalTo(true));
        assertFalse(fieldType.tokenized());
        assertFalse(fieldType.stored());
        assertThat(fieldType.indexOptions(), equalTo(IndexOptions.DOCS));
        assertThat(fieldType.storeTermVectors(), equalTo(false));
        assertThat(fieldType.storeTermVectorOffsets(), equalTo(false));
        assertThat(fieldType.storeTermVectorPositions(), equalTo(false));
        assertThat(fieldType.storeTermVectorPayloads(), equalTo(false));
        assertEquals(DocValuesType.NONE, fieldType.docValuesType());

        assertEquals(expected, fields[3].binaryValue());
        fieldType = fields[3].fieldType();
        assertThat(fieldType.indexOptions(), equalTo(IndexOptions.NONE));
        assertEquals(DocValuesType.SORTED_SET, fieldType.docValuesType());
    }

    public void testIndexOptions() throws IOException {
        String mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("field").field("type", FIELD_TYPE)
            .field("index_options", "freqs").endObject().endObject()
            .endObject().endObject());

        DocumentMapper mapper = parser.parse("type", new CompressedXContent(mapping));

        assertEquals(mapping, mapper.mappingSource().toString());

        ParsedDocument doc = mapper.parse(new SourceToParse("test", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .field("field", "1234")
                        .endObject()),
            XContentType.JSON));

        IndexableField[] fields = doc.rootDoc().getFields("field");
        assertEquals(2, fields.length);
        assertEquals(IndexOptions.DOCS_AND_FREQS, fields[0].fieldType().indexOptions());

        for (String indexOptions : Arrays.asList("positions", "offsets")) {
            final String mapping2 = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
                .startObject("properties").startObject("field").field("type", FIELD_TYPE)
                .field("index_options", indexOptions).endObject().endObject()
                .endObject().endObject());
            IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
                () -> parser.parse("type", new CompressedXContent(mapping2)));
            assertEquals("The [" + FIELD_TYPE + "] field does not support positions, got [index_options]=" + indexOptions,
                e.getMessage());
        }
    }

    public void testEnableNorms() throws IOException {
        String mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("field").field("type", FIELD_TYPE)
            .field("norms", true).endObject().endObject()
            .endObject().endObject());

        DocumentMapper mapper = parser.parse("type", new CompressedXContent(mapping));

        assertEquals(mapping, mapper.mappingSource().toString());

        ParsedDocument doc = mapper.parse(new SourceToParse("test", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .field("field", "1234")
                        .endObject()),
            XContentType.JSON));

        IndexableField[] fields = doc.rootDoc().getFields("field");
        assertEquals(2, fields.length);
        assertFalse(fields[0].fieldType().omitNorms());
    }

    public void testCollator() throws IOException {
        String mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("field")
            .field("type", FIELD_TYPE)
            .field("language", "tr")
            .field("strength", "primary")
            .endObject().endObject().endObject().endObject());

        DocumentMapper mapper = parser.parse("type", new CompressedXContent(mapping));

        assertEquals(mapping, mapper.mappingSource().toString());

        ParsedDocument doc = mapper.parse(new SourceToParse("test", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .field("field", "I WİLL USE TURKİSH CASING")
                        .endObject()),
            XContentType.JSON));

        Collator collator = Collator.getInstance(new ULocale("tr"));
        collator.setStrength(Collator.PRIMARY);
        RawCollationKey key = collator.getRawCollationKey("ı will use turkish casıng", null); // should collate to same value
        BytesRef expected = new BytesRef(key.bytes, 0, key.size);

        IndexableField[] fields = doc.rootDoc().getFields("field");
        assertEquals(2, fields.length);

        assertEquals(expected, fields[0].binaryValue());
        IndexableFieldType fieldType = fields[0].fieldType();
        assertThat(fieldType.omitNorms(), equalTo(true));
        assertFalse(fieldType.tokenized());
        assertFalse(fieldType.stored());
        assertThat(fieldType.indexOptions(), equalTo(IndexOptions.DOCS));
        assertThat(fieldType.storeTermVectors(), equalTo(false));
        assertThat(fieldType.storeTermVectorOffsets(), equalTo(false));
        assertThat(fieldType.storeTermVectorPositions(), equalTo(false));
        assertThat(fieldType.storeTermVectorPayloads(), equalTo(false));
        assertEquals(DocValuesType.NONE, fieldType.docValuesType());

        assertEquals(expected, fields[1].binaryValue());
        fieldType = fields[1].fieldType();
        assertThat(fieldType.indexOptions(), equalTo(IndexOptions.NONE));
        assertEquals(DocValuesType.SORTED_SET, fieldType.docValuesType());
    }

    public void testUpdateCollator() throws IOException {
        String mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("field")
            .field("type", FIELD_TYPE)
            .field("language", "tr")
            .field("strength", "primary")
            .endObject().endObject().endObject().endObject());
        indexService.mapperService().merge("type", new CompressedXContent(mapping), MergeReason.MAPPING_UPDATE);

        String mapping2 = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("field")
            .field("type", FIELD_TYPE)
            .field("language", "en")
            .endObject().endObject().endObject().endObject());
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
            () -> indexService.mapperService().merge("type",
                new CompressedXContent(mapping2), MergeReason.MAPPING_UPDATE));
        assertThat(e.getMessage(), containsString("mapper [field] has different [collator]"));
    }


    public void testIgnoreAbove() throws IOException {
        String mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("field").field("type", FIELD_TYPE)
            .field("ignore_above", 5).endObject().endObject()
            .endObject().endObject());

        DocumentMapper mapper = parser.parse("type", new CompressedXContent(mapping));

        assertEquals(mapping, mapper.mappingSource().toString());

        ParsedDocument doc = mapper.parse(new SourceToParse("test", "1", BytesReference
            .bytes(XContentFactory.jsonBuilder()
                .startObject()
                .field("field", "elk")
                .endObject()),
            XContentType.JSON));

        IndexableField[] fields = doc.rootDoc().getFields("field");
        assertEquals(2, fields.length);

        doc = mapper.parse(new SourceToParse("test", "1", BytesReference
            .bytes(XContentFactory.jsonBuilder()
                .startObject()
                .field("field", "elasticsearch")
                .endObject()),
            XContentType.JSON));

        fields = doc.rootDoc().getFields("field");
        assertEquals(0, fields.length);
    }

    public void testUpdateIgnoreAbove() throws IOException {
        String mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("field").field("type", FIELD_TYPE).endObject().endObject()
            .endObject().endObject());

        indexService.mapperService().merge("type", new CompressedXContent(mapping), MergeReason.MAPPING_UPDATE);

        mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("field").field("type", FIELD_TYPE)
            .field("ignore_above", 5).endObject().endObject()
            .endObject().endObject());
        indexService.mapperService().merge("type", new CompressedXContent(mapping), MergeReason.MAPPING_UPDATE);
    }

}
