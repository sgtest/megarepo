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

import org.apache.lucene.analysis.TokenStream;
import org.apache.lucene.analysis.ngram.NGramTokenFilter;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.env.Environment;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.analysis.AbstractTokenFilterFactory;
import org.elasticsearch.Version;



public class NGramTokenFilterFactory extends AbstractTokenFilterFactory {

    private final int minGram;

    private final int maxGram;


    NGramTokenFilterFactory(IndexSettings indexSettings, Environment environment, String name, Settings settings) {
        super(indexSettings, name, settings);
        int maxAllowedNgramDiff = indexSettings.getMaxNgramDiff();
        this.minGram = settings.getAsInt("min_gram", NGramTokenFilter.DEFAULT_MIN_NGRAM_SIZE);
        this.maxGram = settings.getAsInt("max_gram", NGramTokenFilter.DEFAULT_MAX_NGRAM_SIZE);
        int ngramDiff = maxGram - minGram;
        if (ngramDiff > maxAllowedNgramDiff) {
            if (indexSettings.getIndexVersionCreated().onOrAfter(Version.V_7_0_0_alpha1)) {
                throw new IllegalArgumentException(
                    "The difference between max_gram and min_gram in NGram Tokenizer must be less than or equal to: ["
                        + maxAllowedNgramDiff + "] but was [" + ngramDiff + "]. This limit can be set by changing the ["
                        + IndexSettings.MAX_NGRAM_DIFF_SETTING.getKey() + "] index level setting.");
            } else {
                deprecationLogger.deprecated("Deprecated big difference between max_gram and min_gram in NGram Tokenizer,"
                    + "expected difference must be less than or equal to: [" + maxAllowedNgramDiff + "]");
            }
        }
    }

    @Override
    public TokenStream create(TokenStream tokenStream) {
        return new NGramTokenFilter(tokenStream, minGram, maxGram);
    }
}
