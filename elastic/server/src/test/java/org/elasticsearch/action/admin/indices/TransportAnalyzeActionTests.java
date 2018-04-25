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
package org.elasticsearch.action.admin.indices;

import org.apache.lucene.analysis.MockTokenFilter;
import org.apache.lucene.analysis.TokenStream;
import org.elasticsearch.Version;
import org.elasticsearch.action.admin.indices.analyze.AnalyzeRequest;
import org.elasticsearch.action.admin.indices.analyze.AnalyzeResponse;
import org.elasticsearch.action.admin.indices.analyze.TransportAnalyzeAction;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.TestEnvironment;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.analysis.AbstractCharFilterFactory;
import org.elasticsearch.index.analysis.AbstractTokenFilterFactory;
import org.elasticsearch.index.analysis.AnalysisRegistry;
import org.elasticsearch.index.analysis.CharFilterFactory;
import org.elasticsearch.index.analysis.IndexAnalyzers;
import org.elasticsearch.index.analysis.PreConfiguredCharFilter;
import org.elasticsearch.index.analysis.TokenFilterFactory;
import org.elasticsearch.indices.analysis.AnalysisModule;
import org.elasticsearch.indices.analysis.AnalysisModule.AnalysisProvider;
import org.elasticsearch.indices.analysis.AnalysisModuleTests.AppendCharFilter;
import org.elasticsearch.plugins.AnalysisPlugin;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.IndexSettingsModule;

import java.io.IOException;
import java.io.Reader;
import java.util.List;
import java.util.Map;

import static java.util.Collections.singletonList;
import static java.util.Collections.singletonMap;

/**
 * Tests for {@link TransportAnalyzeAction}. See the rest tests in the {@code analysis-common} module for places where this code gets a ton
 * more exercise.
 */
public class TransportAnalyzeActionTests extends ESTestCase {

    private IndexAnalyzers indexAnalyzers;
    private AnalysisRegistry registry;
    private Environment environment;
    private int maxTokenCount;
    private int idxMaxTokenCount;

    @Override
    public void setUp() throws Exception {
        super.setUp();
        Settings settings = Settings.builder().put(Environment.PATH_HOME_SETTING.getKey(), createTempDir().toString()).build();

        Settings indexSettings = Settings.builder()
                .put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
                .put(IndexMetaData.SETTING_INDEX_UUID, UUIDs.randomBase64UUID())
                .put("index.analysis.analyzer.custom_analyzer.tokenizer", "standard")
                .put("index.analysis.analyzer.custom_analyzer.filter", "mock")
                .put("index.analysis.normalizer.my_normalizer.type", "custom")
                .put("index.analyze.max_token_count", 100)
                .putList("index.analysis.normalizer.my_normalizer.filter", "lowercase").build();
        IndexSettings idxSettings = IndexSettingsModule.newIndexSettings("index", indexSettings);
        environment = TestEnvironment.newEnvironment(settings);
        AnalysisPlugin plugin = new AnalysisPlugin() {
            class MockFactory extends AbstractTokenFilterFactory {
                MockFactory(IndexSettings indexSettings, Environment env, String name, Settings settings) {
                    super(indexSettings, name, settings);
                }

                @Override
                public TokenStream create(TokenStream tokenStream) {
                    return new MockTokenFilter(tokenStream, MockTokenFilter.ENGLISH_STOPSET);
                }
            }

            class AppendCharFilterFactory extends AbstractCharFilterFactory {
                AppendCharFilterFactory(IndexSettings indexSettings, Environment environment, String name, Settings settings) {
                    super(indexSettings, name);
                }

                @Override
                public Reader create(Reader reader) {
                    return new AppendCharFilter(reader, "bar");
                }
            }

            @Override
            public Map<String, AnalysisProvider<CharFilterFactory>> getCharFilters() {
                return singletonMap("append", AppendCharFilterFactory::new);
            }

            @Override
            public Map<String, AnalysisProvider<TokenFilterFactory>> getTokenFilters() {
                return singletonMap("mock", MockFactory::new);
            }

            @Override
            public List<PreConfiguredCharFilter> getPreConfiguredCharFilters() {
                return singletonList(PreConfiguredCharFilter.singleton("append_foo", false, reader -> new AppendCharFilter(reader, "foo")));
            }
        };
        registry = new AnalysisModule(environment, singletonList(plugin)).getAnalysisRegistry();
        indexAnalyzers = registry.build(idxSettings);
        maxTokenCount = IndexSettings.MAX_TOKEN_COUNT_SETTING.getDefault(settings);
        idxMaxTokenCount = idxSettings.getMaxTokenCount();
    }

    /**
     * Test behavior when the named analysis component isn't defined on the index. In that case we should build with defaults.
     */
    public void testNoIndexAnalyzers() throws IOException {
        // Refer to an analyzer by its type so we get its default configuration
        AnalyzeRequest request = new AnalyzeRequest();
        request.text("the quick brown fox");
        request.analyzer("standard");
        AnalyzeResponse analyze = TransportAnalyzeAction.analyze(request, "text", null, null, registry, environment, maxTokenCount);
        List<AnalyzeResponse.AnalyzeToken> tokens = analyze.getTokens();
        assertEquals(4, tokens.size());

        // Refer to a token filter by its type so we get its default configuration
        request = new AnalyzeRequest();
        request.text("the qu1ck brown fox");
        request.tokenizer("standard");
        request.addTokenFilter("mock");
        analyze = TransportAnalyzeAction.analyze(request, "text", null, randomBoolean() ? indexAnalyzers : null, registry, environment, maxTokenCount);
        tokens = analyze.getTokens();
        assertEquals(3, tokens.size());
        assertEquals("qu1ck", tokens.get(0).getTerm());
        assertEquals("brown", tokens.get(1).getTerm());
        assertEquals("fox", tokens.get(2).getTerm());

        // We can refer to a pre-configured token filter by its name to get it
        request = new AnalyzeRequest();
        request.text("the qu1ck brown fox");
        request.tokenizer("standard");
        request.addCharFilter("append_foo");
        analyze = TransportAnalyzeAction.analyze(request, "text", null, randomBoolean() ? indexAnalyzers : null, registry, environment, maxTokenCount);
        tokens = analyze.getTokens();
        assertEquals(4, tokens.size());
        assertEquals("the", tokens.get(0).getTerm());
        assertEquals("qu1ck", tokens.get(1).getTerm());
        assertEquals("brown", tokens.get(2).getTerm());
        assertEquals("foxfoo", tokens.get(3).getTerm());

        // We can refer to a token filter by its type to get its default configuration
        request = new AnalyzeRequest();
        request.text("the qu1ck brown fox");
        request.tokenizer("standard");
        request.addCharFilter("append");
        request.text("the qu1ck brown fox");
        analyze = TransportAnalyzeAction.analyze(request, "text", null, randomBoolean() ? indexAnalyzers : null, registry, environment, maxTokenCount);
        tokens = analyze.getTokens();
        assertEquals(4, tokens.size());
        assertEquals("the", tokens.get(0).getTerm());
        assertEquals("qu1ck", tokens.get(1).getTerm());
        assertEquals("brown", tokens.get(2).getTerm());
        assertEquals("foxbar", tokens.get(3).getTerm());
    }

    public void testFillsAttributes() throws IOException {
        AnalyzeRequest request = new AnalyzeRequest();
        request.analyzer("standard");
        request.text("the 1 brown fox");
        AnalyzeResponse analyze = TransportAnalyzeAction.analyze(request, "text", null, null, registry, environment, maxTokenCount);
        List<AnalyzeResponse.AnalyzeToken> tokens = analyze.getTokens();
        assertEquals(4, tokens.size());
        assertEquals("the", tokens.get(0).getTerm());
        assertEquals(0, tokens.get(0).getStartOffset());
        assertEquals(3, tokens.get(0).getEndOffset());
        assertEquals(0, tokens.get(0).getPosition());
        assertEquals("<ALPHANUM>", tokens.get(0).getType());

        assertEquals("1", tokens.get(1).getTerm());
        assertEquals(4, tokens.get(1).getStartOffset());
        assertEquals(5, tokens.get(1).getEndOffset());
        assertEquals(1, tokens.get(1).getPosition());
        assertEquals("<NUM>", tokens.get(1).getType());

        assertEquals("brown", tokens.get(2).getTerm());
        assertEquals(6, tokens.get(2).getStartOffset());
        assertEquals(11, tokens.get(2).getEndOffset());
        assertEquals(2, tokens.get(2).getPosition());
        assertEquals("<ALPHANUM>", tokens.get(2).getType());

        assertEquals("fox", tokens.get(3).getTerm());
        assertEquals(12, tokens.get(3).getStartOffset());
        assertEquals(15, tokens.get(3).getEndOffset());
        assertEquals(3, tokens.get(3).getPosition());
        assertEquals("<ALPHANUM>", tokens.get(3).getType());
    }

    public void testWithIndexAnalyzers() throws IOException {
        AnalyzeRequest request = new AnalyzeRequest();
        request.text("the quick brown fox");
        request.analyzer("custom_analyzer");
        AnalyzeResponse analyze = TransportAnalyzeAction.analyze(request, "text", null, indexAnalyzers, registry, environment, maxTokenCount);
        List<AnalyzeResponse.AnalyzeToken> tokens = analyze.getTokens();
        assertEquals(3, tokens.size());
        assertEquals("quick", tokens.get(0).getTerm());
        assertEquals("brown", tokens.get(1).getTerm());
        assertEquals("fox", tokens.get(2).getTerm());

        request.analyzer("standard");
        analyze = TransportAnalyzeAction.analyze(request, "text", null, indexAnalyzers, registry, environment, maxTokenCount);
        tokens = analyze.getTokens();
        assertEquals(4, tokens.size());
        assertEquals("the", tokens.get(0).getTerm());
        assertEquals("quick", tokens.get(1).getTerm());
        assertEquals("brown", tokens.get(2).getTerm());
        assertEquals("fox", tokens.get(3).getTerm());

        // Switch the analyzer out for just a tokenizer
        request.analyzer(null);
        request.tokenizer("standard");
        analyze = TransportAnalyzeAction.analyze(request, "text", null, indexAnalyzers, registry, environment, maxTokenCount);
        tokens = analyze.getTokens();
        assertEquals(4, tokens.size());
        assertEquals("the", tokens.get(0).getTerm());
        assertEquals("quick", tokens.get(1).getTerm());
        assertEquals("brown", tokens.get(2).getTerm());
        assertEquals("fox", tokens.get(3).getTerm());

        // Now try applying our token filter
        request.addTokenFilter("mock");
        analyze = TransportAnalyzeAction.analyze(request, "text", null, indexAnalyzers, registry, environment, maxTokenCount);
        tokens = analyze.getTokens();
        assertEquals(3, tokens.size());
        assertEquals("quick", tokens.get(0).getTerm());
        assertEquals("brown", tokens.get(1).getTerm());
        assertEquals("fox", tokens.get(2).getTerm());
    }

    public void testGetIndexAnalyserWithoutIndexAnalyzers() throws IOException {
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
            () -> TransportAnalyzeAction.analyze(
                new AnalyzeRequest()
                    .analyzer("custom_analyzer")
                    .text("the qu1ck brown fox-dog"),
                "text", null, null, registry, environment, maxTokenCount));
        assertEquals(e.getMessage(), "failed to find global analyzer [custom_analyzer]");
    }

    public void testUnknown() throws IOException {
        boolean notGlobal = randomBoolean();
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
            () -> TransportAnalyzeAction.analyze(
                new AnalyzeRequest()
                    .analyzer("foobar")
                    .text("the qu1ck brown fox"),
                "text", null, notGlobal ? indexAnalyzers : null, registry, environment, maxTokenCount));
        if (notGlobal) {
            assertEquals(e.getMessage(), "failed to find analyzer [foobar]");
        } else {
            assertEquals(e.getMessage(), "failed to find global analyzer [foobar]");
        }

        e = expectThrows(IllegalArgumentException.class,
            () -> TransportAnalyzeAction.analyze(
                new AnalyzeRequest()
                    .tokenizer("foobar")
                    .text("the qu1ck brown fox"),
                "text", null, notGlobal ? indexAnalyzers : null, registry, environment, maxTokenCount));
        if (notGlobal) {
            assertEquals(e.getMessage(), "failed to find tokenizer under [foobar]");
        } else {
            assertEquals(e.getMessage(), "failed to find global tokenizer under [foobar]");
        }

        e = expectThrows(IllegalArgumentException.class,
            () -> TransportAnalyzeAction.analyze(
                new AnalyzeRequest()
                    .tokenizer("whitespace")
                    .addTokenFilter("foobar")
                    .text("the qu1ck brown fox"),
                "text", null, notGlobal ? indexAnalyzers : null, registry, environment, maxTokenCount));
        if (notGlobal) {
            assertEquals(e.getMessage(), "failed to find token filter under [foobar]");
        } else {
            assertEquals(e.getMessage(), "failed to find global token filter under [foobar]");
        }

        e = expectThrows(IllegalArgumentException.class,
            () -> TransportAnalyzeAction.analyze(
                new AnalyzeRequest()
                    .tokenizer("whitespace")
                    .addTokenFilter("lowercase")
                    .addCharFilter("foobar")
                    .text("the qu1ck brown fox"),
                "text", null, notGlobal ? indexAnalyzers : null, registry, environment, maxTokenCount));
        if (notGlobal) {
            assertEquals(e.getMessage(), "failed to find char filter under [foobar]");
        } else {
            assertEquals(e.getMessage(), "failed to find global char filter under [foobar]");
        }

        e = expectThrows(IllegalArgumentException.class,
            () -> TransportAnalyzeAction.analyze(
                new AnalyzeRequest()
                    .normalizer("foobar")
                    .text("the qu1ck brown fox"),
                "text", null, indexAnalyzers, registry, environment, maxTokenCount));
        assertEquals(e.getMessage(), "failed to find normalizer under [foobar]");
    }

    public void testNonPreBuildTokenFilter() throws IOException {
        AnalyzeRequest request = new AnalyzeRequest();
        request.tokenizer("whitespace");
        request.addTokenFilter("stop"); // stop token filter is not prebuilt in AnalysisModule#setupPreConfiguredTokenFilters()
        request.text("the quick brown fox");
        AnalyzeResponse analyze = TransportAnalyzeAction.analyze(request, "text", null, indexAnalyzers, registry, environment, maxTokenCount);
        List<AnalyzeResponse.AnalyzeToken> tokens = analyze.getTokens();
        assertEquals(3, tokens.size());
        assertEquals("quick", tokens.get(0).getTerm());
        assertEquals("brown", tokens.get(1).getTerm());
        assertEquals("fox", tokens.get(2).getTerm());
    }

    public void testNormalizerWithIndex() throws IOException {
        AnalyzeRequest request = new AnalyzeRequest("index");
        request.normalizer("my_normalizer");
        request.text("ABc");
        AnalyzeResponse analyze = TransportAnalyzeAction.analyze(request, "text", null, indexAnalyzers, registry, environment, maxTokenCount);
        List<AnalyzeResponse.AnalyzeToken> tokens = analyze.getTokens();

        assertEquals(1, tokens.size());
        assertEquals("abc", tokens.get(0).getTerm());
    }

    /**
     * This test is equivalent of calling _analyze without a specific index.
     * The default value for the maximum token count is used.
     */
    public void testExceedDefaultMaxTokenLimit() throws IOException{
        // create a string with No. words more than maxTokenCount
        StringBuilder sbText = new StringBuilder();
        for (int i = 0; i <= maxTokenCount; i++){
            sbText.append('a');
            sbText.append(' ');
        }
        String text = sbText.toString();

        // request with explain=false to test simpleAnalyze path in TransportAnalyzeAction
        AnalyzeRequest request = new AnalyzeRequest();
        request.text(text);
        request.analyzer("standard");
        IllegalStateException e = expectThrows(IllegalStateException.class,
            () -> TransportAnalyzeAction.analyze(
                request, "text", null, null, registry, environment, maxTokenCount));
        assertEquals(e.getMessage(), "The number of tokens produced by calling _analyze has exceeded the allowed maximum of ["
            + maxTokenCount + "]." + " This limit can be set by changing the [index.analyze.max_token_count] index level setting.");

        // request with explain=true to test detailAnalyze path in TransportAnalyzeAction
        AnalyzeRequest request2 = new AnalyzeRequest();
        request2.text(text);
        request2.analyzer("standard");
        request2.explain(true);
        IllegalStateException e2 = expectThrows(IllegalStateException.class,
            () -> TransportAnalyzeAction.analyze(
                request2, "text", null, null, registry, environment, maxTokenCount));
        assertEquals(e2.getMessage(), "The number of tokens produced by calling _analyze has exceeded the allowed maximum of ["
            + maxTokenCount + "]." + " This limit can be set by changing the [index.analyze.max_token_count] index level setting.");
    }

    /**
     * This test is equivalent of calling _analyze against a specific index.
     * The index specific value for the maximum token count is used.
     */
    public void testExceedSetMaxTokenLimit() throws IOException{
        // create a string with No. words more than idxMaxTokenCount
        StringBuilder sbText = new StringBuilder();
        for (int i = 0; i <= idxMaxTokenCount; i++){
            sbText.append('a');
            sbText.append(' ');
        }
        String text = sbText.toString();

        AnalyzeRequest request = new AnalyzeRequest();
        request.text(text);
        request.analyzer("standard");
        IllegalStateException e = expectThrows(IllegalStateException.class,
            () -> TransportAnalyzeAction.analyze(
                request, "text", null, indexAnalyzers, registry, environment, idxMaxTokenCount));
        assertEquals(e.getMessage(), "The number of tokens produced by calling _analyze has exceeded the allowed maximum of ["
            + idxMaxTokenCount + "]." + " This limit can be set by changing the [index.analyze.max_token_count] index level setting.");
    }
}
