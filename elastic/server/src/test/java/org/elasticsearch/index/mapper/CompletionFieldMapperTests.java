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

import org.apache.lucene.index.IndexableField;
import org.apache.lucene.search.Query;
import org.apache.lucene.search.suggest.document.CompletionAnalyzer;
import org.apache.lucene.search.suggest.document.FuzzyCompletionQuery;
import org.apache.lucene.search.suggest.document.PrefixCompletionQuery;
import org.apache.lucene.search.suggest.document.RegexCompletionQuery;
import org.apache.lucene.search.suggest.document.SuggestField;
import org.apache.lucene.util.BytesRef;
import org.apache.lucene.util.CharsRefBuilder;
import org.apache.lucene.util.automaton.Operations;
import org.apache.lucene.util.automaton.RegExp;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.compress.CompressedXContent;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.Fuzziness;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.index.IndexService;
import org.elasticsearch.index.analysis.NamedAnalyzer;
import org.elasticsearch.test.ESSingleNodeTestCase;

import java.io.IOException;
import java.util.Map;

import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.is;

public class CompletionFieldMapperTests extends ESSingleNodeTestCase {
    public void testDefaultConfiguration() throws IOException {
        String mapping = Strings.toString(jsonBuilder().startObject().startObject("type1")
                .startObject("properties").startObject("completion")
                .field("type", "completion")
                .endObject().endObject()
                .endObject().endObject());

        DocumentMapper defaultMapper = createIndex("test").mapperService().documentMapperParser().parse("type1", new CompressedXContent(mapping));

        Mapper fieldMapper = defaultMapper.mappers().getMapper("completion");
        assertThat(fieldMapper, instanceOf(CompletionFieldMapper.class));
        MappedFieldType completionFieldType = ((CompletionFieldMapper) fieldMapper).fieldType();

        NamedAnalyzer indexAnalyzer = completionFieldType.indexAnalyzer();
        assertThat(indexAnalyzer.name(), equalTo("simple"));
        assertThat(indexAnalyzer.analyzer(), instanceOf(CompletionAnalyzer.class));
        CompletionAnalyzer analyzer = (CompletionAnalyzer) indexAnalyzer.analyzer();
        assertThat(analyzer.preservePositionIncrements(), equalTo(true));
        assertThat(analyzer.preserveSep(), equalTo(true));

        NamedAnalyzer searchAnalyzer = completionFieldType.searchAnalyzer();
        assertThat(searchAnalyzer.name(), equalTo("simple"));
        assertThat(searchAnalyzer.analyzer(), instanceOf(CompletionAnalyzer.class));
        analyzer = (CompletionAnalyzer) searchAnalyzer.analyzer();
        assertThat(analyzer.preservePositionIncrements(), equalTo(true));
        assertThat(analyzer.preserveSep(), equalTo(true));
    }

    public void testCompletionAnalyzerSettings() throws Exception {
        String mapping = Strings.toString(jsonBuilder().startObject().startObject("type1")
                .startObject("properties").startObject("completion")
                .field("type", "completion")
                .field("analyzer", "simple")
                .field("search_analyzer", "standard")
                .field("preserve_separators", false)
                .field("preserve_position_increments", true)
                .endObject().endObject()
                .endObject().endObject());

        DocumentMapper defaultMapper = createIndex("test").mapperService().documentMapperParser().parse("type1", new CompressedXContent(mapping));

        Mapper fieldMapper = defaultMapper.mappers().getMapper("completion");
        assertThat(fieldMapper, instanceOf(CompletionFieldMapper.class));
        MappedFieldType completionFieldType = ((CompletionFieldMapper) fieldMapper).fieldType();

        NamedAnalyzer indexAnalyzer = completionFieldType.indexAnalyzer();
        assertThat(indexAnalyzer.name(), equalTo("simple"));
        assertThat(indexAnalyzer.analyzer(), instanceOf(CompletionAnalyzer.class));
        CompletionAnalyzer analyzer = (CompletionAnalyzer) indexAnalyzer.analyzer();
        assertThat(analyzer.preservePositionIncrements(), equalTo(true));
        assertThat(analyzer.preserveSep(), equalTo(false));

        NamedAnalyzer searchAnalyzer = completionFieldType.searchAnalyzer();
        assertThat(searchAnalyzer.name(), equalTo("standard"));
        assertThat(searchAnalyzer.analyzer(), instanceOf(CompletionAnalyzer.class));
        analyzer = (CompletionAnalyzer) searchAnalyzer.analyzer();
        assertThat(analyzer.preservePositionIncrements(), equalTo(true));
        assertThat(analyzer.preserveSep(), equalTo(false));

    }

    public void testTypeParsing() throws Exception {
        String mapping = Strings.toString(jsonBuilder().startObject().startObject("type1")
                .startObject("properties").startObject("completion")
                .field("type", "completion")
                .field("analyzer", "simple")
                .field("search_analyzer", "standard")
                .field("preserve_separators", false)
                .field("preserve_position_increments", true)
                .field("max_input_length", 14)
                .endObject().endObject()
                .endObject().endObject());

        DocumentMapper defaultMapper = createIndex("test").mapperService().documentMapperParser().parse("type1", new CompressedXContent(mapping));

        Mapper fieldMapper = defaultMapper.mappers().getMapper("completion");
        assertThat(fieldMapper, instanceOf(CompletionFieldMapper.class));

        XContentBuilder builder = jsonBuilder().startObject();
        fieldMapper.toXContent(builder, ToXContent.EMPTY_PARAMS).endObject();
        builder.close();
        Map<String, Object> serializedMap = createParser(JsonXContent.jsonXContent, BytesReference.bytes(builder)).map();
        Map<String, Object> configMap = (Map<String, Object>) serializedMap.get("completion");
        assertThat(configMap.get("analyzer").toString(), is("simple"));
        assertThat(configMap.get("search_analyzer").toString(), is("standard"));
        assertThat(Boolean.valueOf(configMap.get("preserve_separators").toString()), is(false));
        assertThat(Boolean.valueOf(configMap.get("preserve_position_increments").toString()), is(true));
        assertThat(Integer.valueOf(configMap.get("max_input_length").toString()), is(14));
    }

    public void testParsingMinimal() throws Exception {
        String mapping = Strings.toString(jsonBuilder().startObject().startObject("type1")
                .startObject("properties").startObject("completion")
                .field("type", "completion")
                .endObject().endObject()
                .endObject().endObject());

        DocumentMapper defaultMapper = createIndex("test").mapperService().documentMapperParser().parse("type1", new CompressedXContent(mapping));
        Mapper fieldMapper = defaultMapper.mappers().getMapper("completion");

        ParsedDocument parsedDocument = defaultMapper.parse(SourceToParse.source("test", "type1", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .field("completion", "suggestion")
                        .endObject()),
                XContentType.JSON));
        IndexableField[] fields = parsedDocument.rootDoc().getFields(fieldMapper.name());
        assertSuggestFields(fields, 1);
    }

    public void testParsingFailure() throws Exception {
        String mapping = Strings.toString(jsonBuilder().startObject().startObject("type1")
            .startObject("properties").startObject("completion")
            .field("type", "completion")
            .endObject().endObject()
            .endObject().endObject());

        DocumentMapper defaultMapper = createIndex("test").mapperService().documentMapperParser().parse("type1", new CompressedXContent(mapping));

        MapperParsingException e = expectThrows(MapperParsingException.class, () ->
            defaultMapper.parse(SourceToParse.source("test", "type1", "1", BytesReference
                    .bytes(XContentFactory.jsonBuilder()
                            .startObject()
                            .field("completion", 1.0)
                            .endObject()),
                XContentType.JSON)));
        assertEquals("failed to parse [completion]: expected text or object, but got VALUE_NUMBER", e.getCause().getMessage());
    }

    public void testParsingMultiValued() throws Exception {
        String mapping = Strings.toString(jsonBuilder().startObject().startObject("type1")
                .startObject("properties").startObject("completion")
                .field("type", "completion")
                .endObject().endObject()
                .endObject().endObject());

        DocumentMapper defaultMapper = createIndex("test").mapperService().documentMapperParser().parse("type1", new CompressedXContent(mapping));
        Mapper fieldMapper = defaultMapper.mappers().getMapper("completion");

        ParsedDocument parsedDocument = defaultMapper.parse(SourceToParse.source("test", "type1", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .array("completion", "suggestion1", "suggestion2")
                        .endObject()),
                XContentType.JSON));
        IndexableField[] fields = parsedDocument.rootDoc().getFields(fieldMapper.name());
        assertSuggestFields(fields, 2);
    }

    public void testParsingWithWeight() throws Exception {
        String mapping = Strings.toString(jsonBuilder().startObject().startObject("type1")
                .startObject("properties").startObject("completion")
                .field("type", "completion")
                .endObject().endObject()
                .endObject().endObject());

        DocumentMapper defaultMapper = createIndex("test").mapperService().documentMapperParser().parse("type1", new CompressedXContent(mapping));
        Mapper fieldMapper = defaultMapper.mappers().getMapper("completion");

        ParsedDocument parsedDocument = defaultMapper.parse(SourceToParse.source("test", "type1", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .startObject("completion")
                        .field("input", "suggestion")
                        .field("weight", 2)
                        .endObject()
                        .endObject()),
                XContentType.JSON));
        IndexableField[] fields = parsedDocument.rootDoc().getFields(fieldMapper.name());
        assertSuggestFields(fields, 1);
    }

    public void testParsingMultiValueWithWeight() throws Exception {
        String mapping = Strings.toString(jsonBuilder().startObject().startObject("type1")
                .startObject("properties").startObject("completion")
                .field("type", "completion")
                .endObject().endObject()
                .endObject().endObject());

        DocumentMapper defaultMapper = createIndex("test").mapperService().documentMapperParser().parse("type1", new CompressedXContent(mapping));
        Mapper fieldMapper = defaultMapper.mappers().getMapper("completion");

        ParsedDocument parsedDocument = defaultMapper.parse(SourceToParse.source("test", "type1", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .startObject("completion")
                        .array("input", "suggestion1", "suggestion2", "suggestion3")
                        .field("weight", 2)
                        .endObject()
                        .endObject()),
                XContentType.JSON));
        IndexableField[] fields = parsedDocument.rootDoc().getFields(fieldMapper.name());
        assertSuggestFields(fields, 3);
    }

    public void testParsingWithGeoFieldAlias() throws Exception {
        XContentBuilder mapping = XContentFactory.jsonBuilder().startObject()
            .startObject("type1")
                .startObject("properties")
                    .startObject("completion")
                        .field("type", "completion")
                        .startObject("contexts")
                            .field("name", "location")
                            .field("type", "geo")
                            .field("path", "alias")
                        .endObject()
                    .endObject()
                    .startObject("birth-place")
                        .field("type", "geo_point")
                    .endObject()
                    .startObject("alias")
                        .field("type", "alias")
                        .field("path", "birth-place")
                    .endObject()
                .endObject()
            .endObject()
        .endObject();

        MapperService mapperService = createIndex("test", Settings.EMPTY, "type1", mapping).mapperService();
        Mapper fieldMapper = mapperService.documentMapper().mappers().getMapper("completion");

        ParsedDocument parsedDocument = mapperService.documentMapper().parse(SourceToParse.source("test", "type1", "1",
            BytesReference.bytes(XContentFactory.jsonBuilder()
                .startObject()
                    .startObject("completion")
                        .field("input", "suggestion")
                        .startObject("contexts")
                            .field("location", "37.77,-122.42")
                        .endObject()
                    .endObject()
                .endObject()), XContentType.JSON));
        IndexableField[] fields = parsedDocument.rootDoc().getFields(fieldMapper.name());
        assertSuggestFields(fields, 1);
    }

    public void testParsingFull() throws Exception {
        String mapping = Strings.toString(jsonBuilder().startObject().startObject("type1")
                .startObject("properties").startObject("completion")
                .field("type", "completion")
                .endObject().endObject()
                .endObject().endObject());

        DocumentMapper defaultMapper = createIndex("test").mapperService().documentMapperParser().parse("type1", new CompressedXContent(mapping));
        Mapper fieldMapper = defaultMapper.mappers().getMapper("completion");

        ParsedDocument parsedDocument = defaultMapper.parse(SourceToParse.source("test", "type1", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .startArray("completion")
                        .startObject()
                        .field("input", "suggestion1")
                        .field("weight", 3)
                        .endObject()
                        .startObject()
                        .field("input", "suggestion2")
                        .field("weight", 4)
                        .endObject()
                        .startObject()
                        .field("input", "suggestion3")
                        .field("weight", 5)
                        .endObject()
                        .endArray()
                        .endObject()),
                XContentType.JSON));
        IndexableField[] fields = parsedDocument.rootDoc().getFields(fieldMapper.name());
        assertSuggestFields(fields, 3);
    }

    public void testParsingMixed() throws Exception {
        String mapping = Strings.toString(jsonBuilder().startObject().startObject("type1")
                .startObject("properties").startObject("completion")
                .field("type", "completion")
                .endObject().endObject()
                .endObject().endObject());

        DocumentMapper defaultMapper = createIndex("test").mapperService().documentMapperParser().parse("type1", new CompressedXContent(mapping));
        Mapper fieldMapper = defaultMapper.mappers().getMapper("completion");

        ParsedDocument parsedDocument = defaultMapper.parse(SourceToParse.source("test", "type1", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                        .startObject()
                        .startArray("completion")
                        .startObject()
                        .array("input", "suggestion1", "suggestion2")
                        .field("weight", 3)
                        .endObject()
                        .startObject()
                        .field("input", "suggestion3")
                        .field("weight", 4)
                        .endObject()
                        .startObject()
                        .array("input", "suggestion4", "suggestion5", "suggestion6")
                        .field("weight", 5)
                        .endObject()
                        .endArray()
                        .endObject()),
                XContentType.JSON));
        IndexableField[] fields = parsedDocument.rootDoc().getFields(fieldMapper.name());
        assertSuggestFields(fields, 6);
    }

    public void testNonContextEnabledParsingWithContexts() throws Exception {
        String mapping = Strings.toString(jsonBuilder().startObject().startObject("type1")
                .startObject("properties").startObject("field1")
                .field("type", "completion")
                .endObject().endObject()
                .endObject().endObject());

        DocumentMapper defaultMapper = createIndex("test").mapperService().documentMapperParser().parse("type1", new CompressedXContent(mapping));
        try {
            defaultMapper.parse(SourceToParse.source("test", "type1", "1", BytesReference
                    .bytes(XContentFactory.jsonBuilder()
                            .startObject()
                            .startObject("field1")
                            .field("input", "suggestion1")
                            .startObject("contexts")
                            .field("ctx", "ctx2")
                            .endObject()
                            .field("weight", 3)
                            .endObject()
                            .endObject()),
                    XContentType.JSON));
            fail("Supplying contexts to a non context-enabled field should error");
        } catch (MapperParsingException e) {
            assertThat(e.getRootCause().getMessage(), containsString("field1"));
        }
    }

    public void testFieldValueValidation() throws Exception {
        String mapping = Strings.toString(jsonBuilder().startObject().startObject("type1")
                .startObject("properties").startObject("completion")
                .field("type", "completion")
                .endObject().endObject()
                .endObject().endObject());

        DocumentMapper defaultMapper = createIndex("test").mapperService().documentMapperParser().parse("type1", new CompressedXContent(mapping));
        CharsRefBuilder charsRefBuilder = new CharsRefBuilder();
        charsRefBuilder.append("sugg");
        charsRefBuilder.setCharAt(2, '\u001F');
        try {
            defaultMapper.parse(SourceToParse.source("test", "type1", "1", BytesReference
                    .bytes(XContentFactory.jsonBuilder()
                            .startObject()
                            .field("completion", charsRefBuilder.get().toString())
                            .endObject()),
                    XContentType.JSON));
            fail("No error indexing value with reserved character [0x1F]");
        } catch (MapperParsingException e) {
            Throwable cause = e.unwrapCause().getCause();
            assertThat(cause, instanceOf(IllegalArgumentException.class));
            assertThat(cause.getMessage(), containsString("[0x1f]"));
        }

        charsRefBuilder.setCharAt(2, '\u0000');
        try {
            defaultMapper.parse(SourceToParse.source("test", "type1", "1", BytesReference
                    .bytes(XContentFactory.jsonBuilder()
                            .startObject()
                            .field("completion", charsRefBuilder.get().toString())
                            .endObject()),
                    XContentType.JSON));
            fail("No error indexing value with reserved character [0x0]");
        } catch (MapperParsingException e) {
            Throwable cause = e.unwrapCause().getCause();
            assertThat(cause, instanceOf(IllegalArgumentException.class));
            assertThat(cause.getMessage(), containsString("[0x0]"));
        }

        charsRefBuilder.setCharAt(2, '\u001E');
        try {
            defaultMapper.parse(SourceToParse.source("test", "type1", "1", BytesReference
                    .bytes(XContentFactory.jsonBuilder()
                            .startObject()
                            .field("completion", charsRefBuilder.get().toString())
                            .endObject()),
                    XContentType.JSON));
            fail("No error indexing value with reserved character [0x1E]");
        } catch (MapperParsingException e) {
            Throwable cause = e.unwrapCause().getCause();
            assertThat(cause, instanceOf(IllegalArgumentException.class));
            assertThat(cause.getMessage(), containsString("[0x1e]"));
        }

        // empty inputs are ignored
        ParsedDocument doc = defaultMapper.parse(SourceToParse.source("test", "type1", "1", BytesReference
                .bytes(XContentFactory.jsonBuilder()
                    .startObject()
                    .array("completion", "   ", "")
                    .endObject()),
            XContentType.JSON));
        assertThat(doc.docs().size(), equalTo(1));
        assertNull(doc.docs().get(0).get("completion"));
        assertNotNull(doc.docs().get(0).getField("_ignored"));
        IndexableField ignoredFields = doc.docs().get(0).getField("_ignored");
        assertThat(ignoredFields.stringValue(), equalTo("completion"));
    }

    public void testPrefixQueryType() throws Exception {
        String mapping = Strings.toString(jsonBuilder().startObject().startObject("type1")
                .startObject("properties").startObject("completion")
                .field("type", "completion")
                .endObject().endObject()
                .endObject().endObject());

        DocumentMapper defaultMapper = createIndex("test").mapperService().documentMapperParser().parse("type1", new CompressedXContent(mapping));
        Mapper fieldMapper = defaultMapper.mappers().getMapper("completion");
        CompletionFieldMapper completionFieldMapper = (CompletionFieldMapper) fieldMapper;
        Query prefixQuery = completionFieldMapper.fieldType().prefixQuery(new BytesRef("co"));
        assertThat(prefixQuery, instanceOf(PrefixCompletionQuery.class));
    }

    public void testFuzzyQueryType() throws Exception {
        String mapping = Strings.toString(jsonBuilder().startObject().startObject("type1")
                .startObject("properties").startObject("completion")
                .field("type", "completion")
                .endObject().endObject()
                .endObject().endObject());

        DocumentMapper defaultMapper = createIndex("test").mapperService().documentMapperParser().parse("type1", new CompressedXContent(mapping));
        Mapper fieldMapper = defaultMapper.mappers().getMapper("completion");
        CompletionFieldMapper completionFieldMapper = (CompletionFieldMapper) fieldMapper;
        Query prefixQuery = completionFieldMapper.fieldType().fuzzyQuery("co",
                Fuzziness.fromEdits(FuzzyCompletionQuery.DEFAULT_MAX_EDITS), FuzzyCompletionQuery.DEFAULT_NON_FUZZY_PREFIX,
                FuzzyCompletionQuery.DEFAULT_MIN_FUZZY_LENGTH, Operations.DEFAULT_MAX_DETERMINIZED_STATES,
                FuzzyCompletionQuery.DEFAULT_TRANSPOSITIONS, FuzzyCompletionQuery.DEFAULT_UNICODE_AWARE);
        assertThat(prefixQuery, instanceOf(FuzzyCompletionQuery.class));
    }

    public void testRegexQueryType() throws Exception {
        String mapping = Strings.toString(jsonBuilder().startObject().startObject("type1")
                .startObject("properties").startObject("completion")
                .field("type", "completion")
                .endObject().endObject()
                .endObject().endObject());

        DocumentMapper defaultMapper = createIndex("test").mapperService().documentMapperParser().parse("type1", new CompressedXContent(mapping));
        Mapper fieldMapper = defaultMapper.mappers().getMapper("completion");
        CompletionFieldMapper completionFieldMapper = (CompletionFieldMapper) fieldMapper;
        Query prefixQuery = completionFieldMapper.fieldType()
                .regexpQuery(new BytesRef("co"), RegExp.ALL, Operations.DEFAULT_MAX_DETERMINIZED_STATES);
        assertThat(prefixQuery, instanceOf(RegexCompletionQuery.class));
    }

    private static void assertSuggestFields(IndexableField[] fields, int expected) {
        int actualFieldCount = 0;
        for (IndexableField field : fields) {
            if (field instanceof SuggestField) {
                actualFieldCount++;
            }
        }
        assertThat(actualFieldCount, equalTo(expected));
    }

    public void testEmptyName() throws IOException {
        IndexService indexService = createIndex("test");
        DocumentMapperParser parser = indexService.mapperService().documentMapperParser();
        String mapping = Strings.toString(XContentFactory.jsonBuilder().startObject().startObject("type")
            .startObject("properties").startObject("").field("type", "completion").endObject().endObject()
            .endObject().endObject());

        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
            () -> parser.parse("type", new CompressedXContent(mapping))
        );
        assertThat(e.getMessage(), containsString("name cannot be empty string"));
    }
}
