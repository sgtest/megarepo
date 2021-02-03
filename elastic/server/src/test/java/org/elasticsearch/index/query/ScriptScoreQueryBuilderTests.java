/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.query;

import org.apache.lucene.search.MatchNoDocsQuery;
import org.apache.lucene.search.Query;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.common.lucene.search.function.ScriptScoreQuery;
import org.elasticsearch.index.query.functionscore.ScriptScoreQueryBuilder;
import org.elasticsearch.script.MockScriptEngine;
import org.elasticsearch.script.Script;
import org.elasticsearch.script.ScriptType;
import org.elasticsearch.test.AbstractQueryTestCase;

import java.io.IOException;
import java.util.Collections;

import static org.elasticsearch.index.query.QueryBuilders.matchAllQuery;
import static org.hamcrest.CoreMatchers.instanceOf;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class ScriptScoreQueryBuilderTests extends AbstractQueryTestCase<ScriptScoreQueryBuilder> {

    @Override
    protected ScriptScoreQueryBuilder doCreateTestQueryBuilder() {
        String scriptStr = "1";
        Script script = new Script(ScriptType.INLINE, MockScriptEngine.NAME, scriptStr, Collections.emptyMap());
        ScriptScoreQueryBuilder queryBuilder = new ScriptScoreQueryBuilder(RandomQueryBuilder.createQuery(random()), script);
        if (randomBoolean()) {
            queryBuilder.setMinScore(randomFloat());
        }
        return queryBuilder;
    }

    @Override
    protected void doAssertLuceneQuery(ScriptScoreQueryBuilder queryBuilder, Query query,
                                       SearchExecutionContext context) throws IOException {
        Query wrappedQuery = queryBuilder.query().rewrite(context).toQuery(context);
        if (wrappedQuery instanceof MatchNoDocsQuery) {
            assertThat(query, instanceOf(MatchNoDocsQuery.class));
        } else {
            assertThat(query, instanceOf(ScriptScoreQuery.class));
        }
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

        IllegalArgumentException e = expectThrows(
            IllegalArgumentException.class,
            () -> new ScriptScoreQueryBuilder(matchAllQuery(), null)
        );
        assertEquals("script_score: script must not be null" , e.getMessage());

        e = expectThrows(
            IllegalArgumentException.class,
            () -> new ScriptScoreQueryBuilder(null, script)
        );
        assertEquals("script_score: query must not be null" , e.getMessage());
    }

    /**
     * Check that this query is cacheable
     */
    @Override
    public void testCacheability() throws IOException {
        Script script = new Script(ScriptType.INLINE, MockScriptEngine.NAME, "1", Collections.emptyMap());
        ScriptScoreQueryBuilder queryBuilder = new ScriptScoreQueryBuilder(
            new TermQueryBuilder(KEYWORD_FIELD_NAME, "value"), script);

        SearchExecutionContext context = createSearchExecutionContext();
        QueryBuilder rewriteQuery = rewriteQuery(queryBuilder, new SearchExecutionContext(context));
        assertNotNull(rewriteQuery.toQuery(context));
        assertTrue("query should be cacheable: " + queryBuilder.toString(), context.isCacheable());
    }

    @Override
    public void testMustRewrite() throws IOException {
        SearchExecutionContext context = createSearchExecutionContext();
        context.setAllowUnmappedFields(true);
        TermQueryBuilder termQueryBuilder = new TermQueryBuilder("unmapped_field", "foo");
        String scriptStr = "1";
        Script script = new Script(ScriptType.INLINE, MockScriptEngine.NAME, scriptStr, Collections.emptyMap());
        ScriptScoreQueryBuilder scriptScoreQueryBuilder = new ScriptScoreQueryBuilder(termQueryBuilder, script);
        IllegalStateException e = expectThrows(IllegalStateException.class,
                () -> scriptScoreQueryBuilder.toQuery(context));
        assertEquals("Rewrite first", e.getMessage());
    }

    public void testRewriteToMatchNone() throws IOException {
        Script script = new Script(ScriptType.INLINE, MockScriptEngine.NAME, "1", Collections.emptyMap());
        ScriptScoreQueryBuilder builder = new ScriptScoreQueryBuilder(new TermQueryBuilder("unmapped_field", "value"), script);
        QueryBuilder rewrite = builder.rewrite(createSearchExecutionContext());
        assertThat(rewrite, instanceOf(MatchNoneQueryBuilder.class));
    }

    public void testDisallowExpensiveQueries() {
        SearchExecutionContext searchExecutionContext = mock(SearchExecutionContext.class);
        when(searchExecutionContext.allowExpensiveQueries()).thenReturn(false);

        ScriptScoreQueryBuilder queryBuilder = doCreateTestQueryBuilder();
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
                () -> queryBuilder.toQuery(searchExecutionContext));
        assertEquals("[script score] queries cannot be executed when 'search.allow_expensive_queries' is set to false.",
                e.getMessage());
    }
}
