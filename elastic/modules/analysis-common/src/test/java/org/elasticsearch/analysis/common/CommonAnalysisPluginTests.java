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

import org.apache.lucene.analysis.Analyzer;
import org.elasticsearch.Version;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.env.Environment;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.analysis.IndexAnalyzers;
import org.elasticsearch.index.analysis.NamedAnalyzer;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.IndexSettingsModule;
import org.elasticsearch.test.VersionUtils;

import java.io.IOException;

public class CommonAnalysisPluginTests extends ESTestCase {

    /**
     * Check that the deprecated analyzer name "standard_html_strip" throws exception for indices created since 7.0.0
     */
    public void testStandardHtmlStripAnalyzerDeprecationError() throws IOException {
        Settings settings = Settings.builder().put(Environment.PATH_HOME_SETTING.getKey(), createTempDir())
            .put(IndexMetaData.SETTING_VERSION_CREATED,
                VersionUtils.randomVersionBetween(random(), Version.V_7_0_0, Version.CURRENT))
            .put("index.analysis.analyzer.custom_analyzer.type", "standard_html_strip")
            .putList("index.analysis.analyzer.custom_analyzer.stopwords", "a", "b")
            .build();

        IndexSettings idxSettings = IndexSettingsModule.newIndexSettings("index", settings);
        CommonAnalysisPlugin commonAnalysisPlugin = new CommonAnalysisPlugin();
        IllegalArgumentException ex = expectThrows(IllegalArgumentException.class,
            () -> createTestAnalysis(idxSettings, settings, commonAnalysisPlugin));
        assertEquals("[standard_html_strip] analyzer is not supported for new indices, " +
            "use a custom analyzer using [standard] tokenizer and [html_strip] char_filter, plus [lowercase] filter", ex.getMessage());
    }

    /**
     * Check that the deprecated analyzer name "standard_html_strip" issues a deprecation warning for indices created since 6.5.0 until 7
     */
    public void testStandardHtmlStripAnalyzerDeprecationWarning() throws IOException {
        Settings settings = Settings.builder().put(Environment.PATH_HOME_SETTING.getKey(), createTempDir())
            .put(IndexMetaData.SETTING_VERSION_CREATED,
                VersionUtils.randomVersionBetween(random(), Version.V_6_0_0,
                    VersionUtils.getPreviousVersion(Version.V_7_0_0)))
            .put("index.analysis.analyzer.custom_analyzer.type", "standard_html_strip")
            .putList("index.analysis.analyzer.custom_analyzer.stopwords", "a", "b")
            .build();

        IndexSettings idxSettings = IndexSettingsModule.newIndexSettings("index", settings);
        try (CommonAnalysisPlugin commonAnalysisPlugin = new CommonAnalysisPlugin()) {
            IndexAnalyzers analyzers = createTestAnalysis(idxSettings, settings, commonAnalysisPlugin).indexAnalyzers;
            Analyzer analyzer = analyzers.get("custom_analyzer");
            assertNotNull(((NamedAnalyzer) analyzer).analyzer());
            assertWarnings(
                "Deprecated analyzer [standard_html_strip] used, " +
                    "replace it with a custom analyzer using [standard] tokenizer and [html_strip] char_filter, plus [lowercase] filter");
        }
    }
}
