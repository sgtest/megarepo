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

package org.elasticsearch.analysis.common;

import org.apache.lucene.analysis.MockTokenizer;
import org.apache.lucene.analysis.Tokenizer;
import org.elasticsearch.Version;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.env.Environment;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.analysis.TokenFilterFactory;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.IndexSettingsModule;
import org.elasticsearch.test.VersionUtils;

import java.io.IOException;
import java.io.StringReader;
import java.util.Map;

public class CommonAnalysisPluginTests extends ESTestCase {

    /**
     * Check that the deprecated name "nGram" issues a deprecation warning for indices created since 6.3.0
     */
    public void testNGramDeprecationWarning() throws IOException {
        Settings settings = Settings.builder().put(Environment.PATH_HOME_SETTING.getKey(), createTempDir())
                .put(IndexMetaData.SETTING_VERSION_CREATED, VersionUtils.randomVersionBetween(random(), Version.V_6_4_0, Version.CURRENT))
                .build();

        IndexSettings idxSettings = IndexSettingsModule.newIndexSettings("index", settings);
        try (CommonAnalysisPlugin commonAnalysisPlugin = new CommonAnalysisPlugin()) {
            Map<String, TokenFilterFactory> tokenFilters = createTestAnalysis(idxSettings, settings, commonAnalysisPlugin).tokenFilter;
            TokenFilterFactory tokenFilterFactory = tokenFilters.get("nGram");
            Tokenizer tokenizer = new MockTokenizer();
            tokenizer.setReader(new StringReader("foo bar"));
            assertNotNull(tokenFilterFactory.create(tokenizer));
            assertWarnings(
                    "The [nGram] token filter name is deprecated and will be removed in a future version. "
                    + "Please change the filter name to [ngram] instead.");
        }
    }

    /**
     * Check that the deprecated name "nGram" does NOT issues a deprecation warning for indices created before 6.4.0
     */
    public void testNGramNoDeprecationWarningPre6_4() throws IOException {
        Settings settings = Settings.builder().put(Environment.PATH_HOME_SETTING.getKey(), createTempDir())
                .put(IndexMetaData.SETTING_VERSION_CREATED,
                        VersionUtils.randomVersionBetween(random(), Version.V_5_0_0, Version.V_6_3_0))
                .build();

        IndexSettings idxSettings = IndexSettingsModule.newIndexSettings("index", settings);
        try (CommonAnalysisPlugin commonAnalysisPlugin = new CommonAnalysisPlugin()) {
            Map<String, TokenFilterFactory> tokenFilters = createTestAnalysis(idxSettings, settings, commonAnalysisPlugin).tokenFilter;
            TokenFilterFactory tokenFilterFactory = tokenFilters.get("nGram");
            Tokenizer tokenizer = new MockTokenizer();
            tokenizer.setReader(new StringReader("foo bar"));
            assertNotNull(tokenFilterFactory.create(tokenizer));
        }
    }

    /**
     * Check that the deprecated name "edgeNGram" issues a deprecation warning for indices created since 6.3.0
     */
    public void testEdgeNGramDeprecationWarning() throws IOException {
        Settings settings = Settings.builder().put(Environment.PATH_HOME_SETTING.getKey(), createTempDir())
                .put(IndexMetaData.SETTING_VERSION_CREATED, VersionUtils.randomVersionBetween(random(), Version.V_6_4_0, Version.CURRENT))
                .build();

        IndexSettings idxSettings = IndexSettingsModule.newIndexSettings("index", settings);
        try (CommonAnalysisPlugin commonAnalysisPlugin = new CommonAnalysisPlugin()) {
            Map<String, TokenFilterFactory> tokenFilters = createTestAnalysis(idxSettings, settings, commonAnalysisPlugin).tokenFilter;
            TokenFilterFactory tokenFilterFactory = tokenFilters.get("edgeNGram");
            Tokenizer tokenizer = new MockTokenizer();
            tokenizer.setReader(new StringReader("foo bar"));
            assertNotNull(tokenFilterFactory.create(tokenizer));
            assertWarnings(
                    "The [edgeNGram] token filter name is deprecated and will be removed in a future version. "
                    + "Please change the filter name to [edge_ngram] instead.");
        }
    }

    /**
     * Check that the deprecated name "edgeNGram" does NOT issues a deprecation warning for indices created before 6.4.0
     */
    public void testEdgeNGramNoDeprecationWarningPre6_4() throws IOException {
        Settings settings = Settings.builder().put(Environment.PATH_HOME_SETTING.getKey(), createTempDir())
                .put(IndexMetaData.SETTING_VERSION_CREATED,
                        VersionUtils.randomVersionBetween(random(), Version.V_5_0_0, Version.V_6_3_0))
                .build();

        IndexSettings idxSettings = IndexSettingsModule.newIndexSettings("index", settings);
        try (CommonAnalysisPlugin commonAnalysisPlugin = new CommonAnalysisPlugin()) {
            Map<String, TokenFilterFactory> tokenFilters = createTestAnalysis(idxSettings, settings, commonAnalysisPlugin).tokenFilter;
            TokenFilterFactory tokenFilterFactory = tokenFilters.get("edgeNGram");
            Tokenizer tokenizer = new MockTokenizer();
            tokenizer.setReader(new StringReader("foo bar"));
            assertNotNull(tokenFilterFactory.create(tokenizer));
        }
    }
}
