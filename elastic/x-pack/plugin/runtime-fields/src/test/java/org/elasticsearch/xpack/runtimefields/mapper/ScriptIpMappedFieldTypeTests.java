/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.runtimefields.mapper;

import org.apache.lucene.document.StoredField;
import org.apache.lucene.index.DirectoryReader;
import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.index.RandomIndexWriter;
import org.apache.lucene.search.Collector;
import org.apache.lucene.search.IndexSearcher;
import org.apache.lucene.search.LeafCollector;
import org.apache.lucene.search.MatchAllDocsQuery;
import org.apache.lucene.search.Scorable;
import org.apache.lucene.search.ScoreMode;
import org.apache.lucene.search.Sort;
import org.apache.lucene.search.SortField;
import org.apache.lucene.search.TopFieldDocs;
import org.apache.lucene.store.Directory;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.Version;
import org.elasticsearch.common.lucene.search.function.ScriptScoreQuery;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.fielddata.SortedBinaryDocValues;
import org.elasticsearch.index.query.QueryShardContext;
import org.elasticsearch.plugins.ScriptPlugin;
import org.elasticsearch.script.ScoreScript;
import org.elasticsearch.script.Script;
import org.elasticsearch.script.ScriptContext;
import org.elasticsearch.script.ScriptEngine;
import org.elasticsearch.script.ScriptModule;
import org.elasticsearch.script.ScriptService;
import org.elasticsearch.script.ScriptType;
import org.elasticsearch.search.DocValueFormat;
import org.elasticsearch.search.MultiValueMode;
import org.elasticsearch.xpack.runtimefields.IpScriptFieldScript;
import org.elasticsearch.xpack.runtimefields.RuntimeFields;
import org.elasticsearch.xpack.runtimefields.StringScriptFieldScript;
import org.elasticsearch.xpack.runtimefields.fielddata.ScriptBinaryFieldData;
import org.elasticsearch.xpack.runtimefields.fielddata.ScriptIpFieldData;

import java.io.IOException;
import java.time.ZoneId;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.BiConsumer;

import static java.util.Collections.emptyMap;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.sameInstance;

public class ScriptIpMappedFieldTypeTests extends AbstractScriptMappedFieldTypeTestCase {
    public void testFormat() throws IOException {
        assertThat(simpleMappedFieldType().docValueFormat(null, null), sameInstance(DocValueFormat.IP));
        Exception e = expectThrows(IllegalArgumentException.class, () -> simpleMappedFieldType().docValueFormat("ASDFA", null));
        assertThat(e.getMessage(), equalTo("Field [test] of type [runtime_script] with runtime type [ip] does not support custom formats"));
        e = expectThrows(IllegalArgumentException.class, () -> simpleMappedFieldType().docValueFormat(null, ZoneId.of("America/New_York")));
        assertThat(
            e.getMessage(),
            equalTo("Field [test] of type [runtime_script] with runtime type [ip] does not support custom time zones")
        );
    }

    @Override
    public void testDocValues() throws IOException {
        try (Directory directory = newDirectory(); RandomIndexWriter iw = new RandomIndexWriter(random(), directory)) {
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"192.168.0\"]}"))));
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"192.168.2\", \"192.168.1\"]}"))));
            List<Object> results = new ArrayList<>();
            try (DirectoryReader reader = iw.getReader()) {
                IndexSearcher searcher = newSearcher(reader);
                ScriptIpMappedFieldType ft = build("append_param", Map.of("param", ".1"));
                ScriptBinaryFieldData ifd = ft.fielddataBuilder("test", mockContext()::lookup).build(null, null, null);
                DocValueFormat format = ft.docValueFormat(null, null);
                searcher.search(new MatchAllDocsQuery(), new Collector() {
                    @Override
                    public ScoreMode scoreMode() {
                        return ScoreMode.COMPLETE_NO_SCORES;
                    }

                    @Override
                    public LeafCollector getLeafCollector(LeafReaderContext context) {
                        SortedBinaryDocValues dv = ifd.load(context).getBytesValues();
                        return new LeafCollector() {
                            @Override
                            public void setScorer(Scorable scorer) {}

                            @Override
                            public void collect(int doc) throws IOException {
                                if (dv.advanceExact(doc)) {
                                    for (int i = 0; i < dv.docValueCount(); i++) {
                                        results.add(format.format(dv.nextValue()));
                                    }
                                }
                            }
                        };
                    }
                });
                assertThat(results, equalTo(List.of("192.168.0.1", "192.168.1.1", "192.168.2.1")));
            }
        }
    }

    @Override
    public void testSort() throws IOException {
        try (Directory directory = newDirectory(); RandomIndexWriter iw = new RandomIndexWriter(random(), directory)) {
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"192.168.0.1\"]}"))));
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"192.168.0.4\"]}"))));
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"192.168.0.2\"]}"))));
            try (DirectoryReader reader = iw.getReader()) {
                IndexSearcher searcher = newSearcher(reader);
                ScriptBinaryFieldData ifd = simpleMappedFieldType().fielddataBuilder("test", mockContext()::lookup).build(null, null, null);
                SortField sf = ifd.sortField(null, MultiValueMode.MIN, null, false);
                TopFieldDocs docs = searcher.search(new MatchAllDocsQuery(), 3, new Sort(sf));
                assertThat(
                    reader.document(docs.scoreDocs[0].doc).getBinaryValue("_source").utf8ToString(),
                    equalTo("{\"foo\": [\"192.168.0.1\"]}")
                );
                assertThat(
                    reader.document(docs.scoreDocs[1].doc).getBinaryValue("_source").utf8ToString(),
                    equalTo("{\"foo\": [\"192.168.0.2\"]}")
                );
                assertThat(
                    reader.document(docs.scoreDocs[2].doc).getBinaryValue("_source").utf8ToString(),
                    equalTo("{\"foo\": [\"192.168.0.4\"]}")
                );
            }
        }
    }

    @Override
    public void testUsedInScript() throws IOException {
        try (Directory directory = newDirectory(); RandomIndexWriter iw = new RandomIndexWriter(random(), directory)) {
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"192.168.0.1\"]}"))));
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"192.168.0.4\"]}"))));
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"192.168.0.2\"]}"))));
            try (DirectoryReader reader = iw.getReader()) {
                IndexSearcher searcher = newSearcher(reader);
                QueryShardContext qsc = mockContext(true, simpleMappedFieldType());
                assertThat(searcher.count(new ScriptScoreQuery(new MatchAllDocsQuery(), new Script("test"), new ScoreScript.LeafFactory() {
                    @Override
                    public boolean needs_score() {
                        return false;
                    }

                    @Override
                    public ScoreScript newInstance(LeafReaderContext ctx) {
                        return new ScoreScript(Map.of(), qsc.lookup(), ctx) {
                            @Override
                            public double execute(ExplanationHolder explanation) {
                                ScriptIpFieldData.IpScriptDocValues bytes = (ScriptIpFieldData.IpScriptDocValues) getDoc().get("test");
                                return Integer.parseInt(bytes.getValue().substring(bytes.getValue().lastIndexOf(".") + 1));
                            }
                        };
                    }
                }, 2.5f, "test", 0, Version.CURRENT)), equalTo(1));
            }
        }
    }

    @Override
    public void testExistsQuery() throws IOException {
        try (Directory directory = newDirectory(); RandomIndexWriter iw = new RandomIndexWriter(random(), directory)) {
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"192.168.0.1\"]}"))));
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": []}"))));
            try (DirectoryReader reader = iw.getReader()) {
                IndexSearcher searcher = newSearcher(reader);
                assertThat(searcher.count(simpleMappedFieldType().existsQuery(mockContext())), equalTo(1));
            }
        }
    }

    @Override
    public void testExistsQueryIsExpensive() throws IOException {
        checkExpensiveQuery(ScriptIpMappedFieldType::existsQuery);
    }

    @Override
    public void testRangeQuery() throws IOException {
        try (Directory directory = newDirectory(); RandomIndexWriter iw = new RandomIndexWriter(random(), directory)) {
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"192.168.0.1\"]}"))));
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"200.0.0.1\"]}"))));
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"1.1.1.1\"]}"))));
            try (DirectoryReader reader = iw.getReader()) {
                IndexSearcher searcher = newSearcher(reader);
                assertThat(
                    searcher.count(
                        simpleMappedFieldType().rangeQuery("192.0.0.0", "200.0.0.0", false, false, null, null, null, mockContext())
                    ),
                    equalTo(1)
                );
            }
        }
    }

    @Override
    public void testRangeQueryIsExpensive() throws IOException {
        checkExpensiveQuery((ft, ctx) -> ft.rangeQuery("192.0.0.0", "200.0.0.0", randomBoolean(), randomBoolean(), null, null, null, ctx));
    }

    @Override
    public void testTermQuery() throws IOException {
        try (Directory directory = newDirectory(); RandomIndexWriter iw = new RandomIndexWriter(random(), directory)) {
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"192.168.0\"]}"))));
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"192.168.1\"]}"))));
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"200.0.0\"]}"))));
            try (DirectoryReader reader = iw.getReader()) {
                IndexSearcher searcher = newSearcher(reader);
                ScriptIpMappedFieldType fieldType = build("append_param", Map.of("param", ".1"));
                assertThat(searcher.count(fieldType.termQuery("192.168.0.1", mockContext())), equalTo(1));
                assertThat(searcher.count(fieldType.termQuery("192.168.0.7", mockContext())), equalTo(0));
                assertThat(searcher.count(fieldType.termQuery("192.168.0.0/16", mockContext())), equalTo(2));
                assertThat(searcher.count(fieldType.termQuery("10.168.0.0/16", mockContext())), equalTo(0));
            }
        }
    }

    @Override
    public void testTermQueryIsExpensive() throws IOException {
        checkExpensiveQuery((ft, ctx) -> ft.termQuery(randomIp(randomBoolean()), ctx));
    }

    @Override
    public void testTermsQuery() throws IOException {
        try (Directory directory = newDirectory(); RandomIndexWriter iw = new RandomIndexWriter(random(), directory)) {
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"192.168.0.1\"]}"))));
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"192.168.1.1\"]}"))));
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"200.0.0.1\"]}"))));
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{\"foo\": [\"1.1.1.1\"]}"))));
            try (DirectoryReader reader = iw.getReader()) {
                IndexSearcher searcher = newSearcher(reader);
                assertThat(
                    searcher.count(simpleMappedFieldType().termsQuery(List.of("192.168.0.1", "1.1.1.1"), mockContext())),
                    equalTo(2)
                );
                assertThat(
                    searcher.count(simpleMappedFieldType().termsQuery(List.of("192.168.0.0/16", "1.1.1.1"), mockContext())),
                    equalTo(3)
                );
            }
        }
    }

    @Override
    public void testTermsQueryIsExpensive() throws IOException {
        checkExpensiveQuery((ft, ctx) -> ft.termsQuery(randomList(100, () -> randomAlphaOfLengthBetween(1, 1000)), ctx));
    }

    @Override
    protected ScriptIpMappedFieldType simpleMappedFieldType() throws IOException {
        return build("read_foo", Map.of());
    }

    @Override
    protected String runtimeType() {
        return "ip";
    }

    private static ScriptIpMappedFieldType build(String code, Map<String, Object> params) throws IOException {
        return build(new Script(ScriptType.INLINE, "test", code, params));
    }

    private static ScriptIpMappedFieldType build(Script script) throws IOException {
        ScriptPlugin scriptPlugin = new ScriptPlugin() {
            @Override
            public ScriptEngine getScriptEngine(Settings settings, Collection<ScriptContext<?>> contexts) {
                return new ScriptEngine() {
                    @Override
                    public String getType() {
                        return "test";
                    }

                    @Override
                    public Set<ScriptContext<?>> getSupportedContexts() {
                        return Set.of(StringScriptFieldScript.CONTEXT);
                    }

                    @Override
                    public <FactoryType> FactoryType compile(
                        String name,
                        String code,
                        ScriptContext<FactoryType> context,
                        Map<String, String> params
                    ) {
                        @SuppressWarnings("unchecked")
                        FactoryType factory = (FactoryType) factory(code);
                        return factory;
                    }

                    private IpScriptFieldScript.Factory factory(String code) {
                        switch (code) {
                            case "read_foo":
                                return (params, lookup) -> (ctx) -> new IpScriptFieldScript(params, lookup, ctx) {
                                    @Override
                                    public void execute() {
                                        for (Object foo : (List<?>) getSource().get("foo")) {
                                            emitValue(foo.toString());
                                        }
                                    }
                                };
                            case "append_param":
                                return (params, lookup) -> (ctx) -> new IpScriptFieldScript(params, lookup, ctx) {
                                    @Override
                                    public void execute() {
                                        for (Object foo : (List<?>) getSource().get("foo")) {
                                            emitValue(foo.toString() + getParams().get("param"));
                                        }
                                    }
                                };
                            default:
                                throw new IllegalArgumentException("unsupported script [" + code + "]");
                        }
                    }
                };
            }
        };
        ScriptModule scriptModule = new ScriptModule(Settings.EMPTY, List.of(scriptPlugin, new RuntimeFields()));
        try (ScriptService scriptService = new ScriptService(Settings.EMPTY, scriptModule.engines, scriptModule.contexts)) {
            IpScriptFieldScript.Factory factory = scriptService.compile(script, IpScriptFieldScript.CONTEXT);
            return new ScriptIpMappedFieldType("test", script, factory, emptyMap());
        }
    }

    private void checkExpensiveQuery(BiConsumer<ScriptIpMappedFieldType, QueryShardContext> queryBuilder) throws IOException {
        ScriptIpMappedFieldType ft = simpleMappedFieldType();
        Exception e = expectThrows(ElasticsearchException.class, () -> queryBuilder.accept(ft, mockContext(false)));
        assertThat(
            e.getMessage(),
            equalTo("queries cannot be executed against [runtime_script] fields while [search.allow_expensive_queries] is set to [false].")
        );
    }
}
