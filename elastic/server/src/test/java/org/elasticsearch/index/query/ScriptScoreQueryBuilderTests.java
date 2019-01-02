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

package org.elasticsearch.index.query;

import org.apache.lucene.search.Query;
import org.elasticsearch.common.lucene.search.function.ScriptScoreQuery;
import org.elasticsearch.index.query.functionscore.ScriptScoreFunctionBuilder;
import org.elasticsearch.index.query.functionscore.ScriptScoreQueryBuilder;
import org.elasticsearch.script.MockScriptEngine;
import org.elasticsearch.script.Script;
import org.elasticsearch.script.ScriptType;
import org.elasticsearch.search.internal.SearchContext;
import org.elasticsearch.test.AbstractQueryTestCase;

import java.io.IOException;
import java.util.Collections;

import static org.elasticsearch.index.query.QueryBuilders.matchAllQuery;
import static org.hamcrest.CoreMatchers.instanceOf;

public class ScriptScoreQueryBuilderTests extends AbstractQueryTestCase<ScriptScoreQueryBuilder> {

    @Override
    protected ScriptScoreQueryBuilder doCreateTestQueryBuilder() {
        String scriptStr = "1";
        Script script = new Script(ScriptType.INLINE, MockScriptEngine.NAME, scriptStr, Collections.emptyMap());
        ScriptScoreQueryBuilder queryBuilder = new ScriptScoreQueryBuilder(
            RandomQueryBuilder.createQuery(random()),
            new ScriptScoreFunctionBuilder(script)
        );
        if (randomBoolean()) {
            queryBuilder.setMinScore(randomFloat());
        }
        return queryBuilder;
    }

    @Override
    protected void doAssertLuceneQuery(ScriptScoreQueryBuilder queryBuilder, Query query, SearchContext context) throws IOException {
        assertThat(query, instanceOf(ScriptScoreQuery.class));
    }

    public void testFromJson() throws IOException {
        String json =
            "{\n" +
                "  \"script_score\" : {\n" +
                "    \"query\" : { \"match_all\" : {} },\n" +
                "    \"script\" : {\n" +
                "      \"source\" : \"doc['field'].value\" \n" +
                "    },\n" +
                "    \"min_score\" : 2.0\n" +
                "  }\n" +
                "}";

        ScriptScoreQueryBuilder parsed = (ScriptScoreQueryBuilder) parseQuery(json);
        assertEquals(json, 2, parsed.getMinScore(), 0.0001);
    }

    public void testIllegalArguments() {
        String scriptStr = "1";
        Script script = new Script(ScriptType.INLINE, MockScriptEngine.NAME, scriptStr, Collections.emptyMap());
        ScriptScoreFunctionBuilder functionBuilder = new ScriptScoreFunctionBuilder(script);

        expectThrows(
            IllegalArgumentException.class,
            () -> new ScriptScoreQueryBuilder(matchAllQuery(), null)
        );

        expectThrows(
            IllegalArgumentException.class,
            () -> new ScriptScoreQueryBuilder(null, functionBuilder)
        );
    }

    @Override
    protected boolean isCacheable(ScriptScoreQueryBuilder queryBuilder) {
        return false;
    }
}
