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

import org.apache.lucene.analysis.MockSynonymAnalyzer;
import org.apache.lucene.index.Term;
import org.apache.lucene.search.BooleanClause;
import org.apache.lucene.search.BooleanClause.Occur;
import org.apache.lucene.search.BooleanQuery;
import org.apache.lucene.search.BoostQuery;
import org.apache.lucene.search.ConstantScoreQuery;
import org.apache.lucene.search.DisjunctionMaxQuery;
import org.apache.lucene.search.FuzzyQuery;
import org.apache.lucene.search.MatchAllDocsQuery;
import org.apache.lucene.search.MatchNoDocsQuery;
import org.apache.lucene.search.MultiTermQuery;
import org.apache.lucene.search.NormsFieldExistsQuery;
import org.apache.lucene.search.PhraseQuery;
import org.apache.lucene.search.PrefixQuery;
import org.apache.lucene.search.Query;
import org.apache.lucene.search.RegexpQuery;
import org.apache.lucene.search.SynonymQuery;
import org.apache.lucene.search.TermQuery;
import org.apache.lucene.search.TermRangeQuery;
import org.apache.lucene.search.WildcardQuery;
import org.apache.lucene.search.spans.SpanNearQuery;
import org.apache.lucene.search.spans.SpanOrQuery;
import org.apache.lucene.search.spans.SpanTermQuery;
import org.apache.lucene.util.BytesRef;
import org.apache.lucene.util.automaton.TooComplexToDeterminizeException;
import org.elasticsearch.Version;
import org.elasticsearch.action.admin.indices.mapping.put.PutMappingRequest;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.compress.CompressedXContent;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.Fuzziness;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.index.search.QueryStringQueryParser;
import org.elasticsearch.search.internal.SearchContext;
import org.elasticsearch.test.AbstractQueryTestCase;
import org.hamcrest.Matchers;
import org.joda.time.DateTimeZone;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.elasticsearch.index.query.AbstractQueryBuilder.parseInnerQueryBuilder;
import static org.elasticsearch.index.query.QueryBuilders.queryStringQuery;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertBooleanSubQuery;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertDisjunctionSubQuery;
import static org.hamcrest.CoreMatchers.equalTo;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.instanceOf;

public class QueryStringQueryBuilderTests extends AbstractQueryTestCase<QueryStringQueryBuilder> {
    @Override
    protected QueryStringQueryBuilder doCreateTestQueryBuilder() {
        int numTerms = randomIntBetween(0, 5);
        String query = "";
        for (int i = 0; i < numTerms; i++) {
            //min length 4 makes sure that the text is not an operator (AND/OR) so toQuery won't break
            query += (randomBoolean() ? STRING_FIELD_NAME + ":" : "") + randomAlphaOfLengthBetween(4, 10) + " ";
        }
        QueryStringQueryBuilder queryStringQueryBuilder = new QueryStringQueryBuilder(query);
        if (randomBoolean()) {
            String defaultFieldName = randomFrom(STRING_FIELD_NAME,
                STRING_ALIAS_FIELD_NAME,
                randomAlphaOfLengthBetween(1, 10));
            queryStringQueryBuilder.defaultField(defaultFieldName);
        } else {
            int numFields = randomIntBetween(1, 5);
            for (int i = 0; i < numFields; i++) {
                String fieldName = randomFrom(STRING_FIELD_NAME,
                    STRING_ALIAS_FIELD_NAME,
                    randomAlphaOfLengthBetween(1, 10));
                if (randomBoolean()) {
                    queryStringQueryBuilder.field(fieldName);
                } else {
                    queryStringQueryBuilder.field(fieldName, randomFloat());
                }
            }
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.defaultOperator(randomFrom(Operator.values()));
        }
        if (randomBoolean()) {
            //we only use string fields (either mapped or unmapped)
            queryStringQueryBuilder.fuzziness(randomFuzziness(STRING_FIELD_NAME));
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.analyzer(randomAnalyzer());
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.quoteAnalyzer(randomAnalyzer());
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.allowLeadingWildcard(randomBoolean());
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.analyzeWildcard(randomBoolean());
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.maxDeterminizedStates(randomIntBetween(1, 100));
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.autoGeneratePhraseQueries(randomBoolean());
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.enablePositionIncrements(randomBoolean());
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.escape(randomBoolean());
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.phraseSlop(randomIntBetween(0, 10));
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.fuzzyMaxExpansions(randomIntBetween(0, 100));
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.fuzzyPrefixLength(randomIntBetween(0, 10));
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.fuzzyRewrite(getRandomRewriteMethod());
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.rewrite(getRandomRewriteMethod());
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.quoteFieldSuffix(randomAlphaOfLengthBetween(1, 3));
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.tieBreaker(randomFloat());
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.minimumShouldMatch(randomMinimumShouldMatch());
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.useDisMax(randomBoolean());
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.timeZone(randomDateTimeZone().getID());
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.autoGenerateSynonymsPhraseQuery(randomBoolean());
        }
        if (randomBoolean()) {
            queryStringQueryBuilder.fuzzyTranspositions(randomBoolean());
        }
        queryStringQueryBuilder.type(randomFrom(MultiMatchQueryBuilder.Type.values()));
        return queryStringQueryBuilder;
    }

    @Override
    public QueryStringQueryBuilder mutateInstance(QueryStringQueryBuilder instance) throws IOException {
        String query = instance.queryString();
        String defaultField = instance.defaultField();
        Map<String, Float> fields = instance.fields();
        Operator operator = instance.defaultOperator();
        Fuzziness fuzziness = instance.fuzziness();
        String analyzer = instance.analyzer();
        String quoteAnalyzer = instance.quoteAnalyzer();
        Boolean allowLeadingWildCard = instance.allowLeadingWildcard();
        Boolean analyzeWildcard = instance.analyzeWildcard();
        int maxDeterminizedStates = instance.maxDeterminizedStates();
        boolean enablePositionIncrements = instance.enablePositionIncrements();
        boolean escape = instance.escape();
        int phraseSlop = instance.phraseSlop();
        int fuzzyMaxExpansions = instance.fuzzyMaxExpansions();
        int fuzzyPrefixLength = instance.fuzzyPrefixLength();
        String fuzzyRewrite = instance.fuzzyRewrite();
        String rewrite = instance.rewrite();
        String quoteFieldSuffix = instance.quoteFieldSuffix();
        Float tieBreaker = instance.tieBreaker();
        String minimumShouldMatch = instance.minimumShouldMatch();
        String timeZone = instance.timeZone() == null ? null : instance.timeZone().getID();
        boolean autoGenerateSynonymsPhraseQuery = instance.autoGenerateSynonymsPhraseQuery();
        boolean fuzzyTranspositions = instance.fuzzyTranspositions();

        switch (between(0, 23)) {
        case 0:
            query = query + " foo";
            break;
        case 1:
            if (defaultField == null) {
                defaultField = randomAlphaOfLengthBetween(1, 10);
            } else {
                defaultField = defaultField + randomAlphaOfLength(5);
            }
            break;
        case 2:
            fields = new HashMap<>(fields);
            fields.put(randomAlphaOfLength(10), 1.0f);
            break;
        case 3:
            operator = randomValueOtherThan(operator, () -> randomFrom(Operator.values()));
            break;
        case 4:
            fuzziness = randomValueOtherThan(fuzziness, () -> randomFrom(Fuzziness.AUTO, Fuzziness.ZERO, Fuzziness.ONE, Fuzziness.TWO));
            break;
        case 5:
            if (analyzer == null) {
                analyzer = randomAnalyzer();
            } else {
                analyzer = null;
            }
            break;
        case 6:
            if (quoteAnalyzer == null) {
                quoteAnalyzer = randomAnalyzer();
            } else {
                quoteAnalyzer = null;
            }
            break;
        case 7:
            if (allowLeadingWildCard == null) {
                allowLeadingWildCard = randomBoolean();
            } else {
                allowLeadingWildCard = randomBoolean() ? null : (allowLeadingWildCard == false);
            }
            break;
        case 8:
            if (analyzeWildcard == null) {
                analyzeWildcard = randomBoolean();
            } else {
                analyzeWildcard = randomBoolean() ? null : (analyzeWildcard == false);
            }
            break;
        case 9:
            maxDeterminizedStates += 5;
            break;
        case 10:
            enablePositionIncrements = (enablePositionIncrements == false);
            break;
        case 11:
            escape = (escape == false);
            break;
        case 12:
            phraseSlop += 5;
            break;
        case 13:
            fuzzyMaxExpansions += 5;
            break;
        case 14:
            fuzzyPrefixLength += 5;
            break;
        case 15:
            if (fuzzyRewrite == null) {
                fuzzyRewrite = getRandomRewriteMethod();
            } else {
                fuzzyRewrite = null;
            }
            break;
        case 16:
            if (rewrite == null) {
                rewrite = getRandomRewriteMethod();
            } else {
                rewrite = null;
            }
            break;
        case 17:
            if (quoteFieldSuffix == null) {
                quoteFieldSuffix = randomAlphaOfLengthBetween(1, 3);
            } else {
                quoteFieldSuffix = quoteFieldSuffix + randomAlphaOfLength(1);
            }
            break;
        case 18:
            if (tieBreaker == null) {
                tieBreaker = randomFloat();
            } else {
                tieBreaker += 0.05f;
            }
            break;
        case 19:
            if (minimumShouldMatch == null) {
                minimumShouldMatch = randomMinimumShouldMatch();
            } else {
                minimumShouldMatch = null;
            }
            break;
        case 20:
            if (timeZone == null) {
                timeZone = randomDateTimeZone().getID();
            } else {
                if (randomBoolean()) {
                    timeZone = null;
                } else {
                    timeZone = randomValueOtherThan(timeZone, () -> randomDateTimeZone().getID());
                }
            }
            break;
        case 21:
            autoGenerateSynonymsPhraseQuery = (autoGenerateSynonymsPhraseQuery == false);
            break;
        case 22:
            fuzzyTranspositions = (fuzzyTranspositions == false);
            break;
        case 23:
            return changeNameOrBoost(instance);
        default:
            throw new AssertionError("Illegal randomisation branch");
        }

        QueryStringQueryBuilder newInstance = new QueryStringQueryBuilder(query);
        if (defaultField != null) {
            newInstance.defaultField(defaultField);
        }
        newInstance.fields(fields);
        newInstance.defaultOperator(operator);
        newInstance.fuzziness(fuzziness);
        if (analyzer != null) {
            newInstance.analyzer(analyzer);
        }
        if (quoteAnalyzer != null) {
            newInstance.quoteAnalyzer(quoteAnalyzer);
        }
        if (allowLeadingWildCard != null) {
            newInstance.allowLeadingWildcard(allowLeadingWildCard);
        }
        if (analyzeWildcard != null) {
            newInstance.analyzeWildcard(analyzeWildcard);
        }
        newInstance.maxDeterminizedStates(maxDeterminizedStates);
        newInstance.enablePositionIncrements(enablePositionIncrements);
        newInstance.escape(escape);
        newInstance.phraseSlop(phraseSlop);
        newInstance.fuzzyMaxExpansions(fuzzyMaxExpansions);
        newInstance.fuzzyPrefixLength(fuzzyPrefixLength);
        if (fuzzyRewrite != null) {
            newInstance.fuzzyRewrite(fuzzyRewrite);
        }
        if (rewrite != null) {
            newInstance.rewrite(rewrite);
        }
        if (quoteFieldSuffix != null) {
            newInstance.quoteFieldSuffix(quoteFieldSuffix);
        }
        if (tieBreaker != null) {
            newInstance.tieBreaker(tieBreaker);
        }
        if (minimumShouldMatch != null) {
            newInstance.minimumShouldMatch(minimumShouldMatch);
        }
        if (timeZone != null) {
            newInstance.timeZone(timeZone);
        }
        newInstance.autoGenerateSynonymsPhraseQuery(autoGenerateSynonymsPhraseQuery);
        newInstance.fuzzyTranspositions(fuzzyTranspositions);

        return newInstance;
    }

    @Override
    protected void doAssertLuceneQuery(QueryStringQueryBuilder queryBuilder,
                                       Query query, SearchContext context) throws IOException {
        // nothing yet, put additional assertions here.
    }

    // Tests fix for https://github.com/elastic/elasticsearch/issues/29403
    public void testTimezoneEquals() {
        QueryStringQueryBuilder builder1 = new QueryStringQueryBuilder("bar");
        QueryStringQueryBuilder builder2 = new QueryStringQueryBuilder("foo");
        assertNotEquals(builder1, builder2);
        builder1.timeZone("Europe/London");
        builder2.timeZone("Europe/London");
        assertNotEquals(builder1, builder2);
    }

    public void testIllegalArguments() {
        expectThrows(IllegalArgumentException.class, () -> new QueryStringQueryBuilder((String) null));
    }

    public void testToQueryMatchAllQuery() throws Exception {
        Query query = queryStringQuery("*:*").toQuery(createShardContext());
        assertThat(query, instanceOf(MatchAllDocsQuery.class));
    }

    public void testToQueryTermQuery() throws IOException {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        Query query = queryStringQuery("test").defaultField(STRING_FIELD_NAME).toQuery(createShardContext());
        assertThat(query, instanceOf(TermQuery.class));
        TermQuery termQuery = (TermQuery) query;
        assertThat(termQuery.getTerm(), equalTo(new Term(STRING_FIELD_NAME, "test")));
    }

    public void testToQueryPhraseQuery() throws IOException {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        Query query = queryStringQuery("\"term1 term2\"")
            .defaultField(STRING_FIELD_NAME)
            .phraseSlop(3)
            .toQuery(createShardContext());
        assertThat(query, instanceOf(PhraseQuery.class));
        PhraseQuery phraseQuery = (PhraseQuery) query;
        assertThat(phraseQuery.getTerms().length, equalTo(2));
        assertThat(phraseQuery.getTerms()[0], equalTo(new Term(STRING_FIELD_NAME, "term1")));
        assertThat(phraseQuery.getTerms()[1], equalTo(new Term(STRING_FIELD_NAME, "term2")));
        assertThat(phraseQuery.getSlop(), equalTo(3));
    }

    public void testToQueryBoosts() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        QueryShardContext shardContext = createShardContext();
        QueryStringQueryBuilder queryStringQuery = queryStringQuery(STRING_FIELD_NAME + ":boosted^2");
        Query query = queryStringQuery.toQuery(shardContext);
        assertThat(query, instanceOf(BoostQuery.class));
        BoostQuery boostQuery = (BoostQuery) query;
        assertThat(boostQuery.getBoost(), equalTo(2.0f));
        assertThat(boostQuery.getQuery(), instanceOf(TermQuery.class));
        assertThat(((TermQuery) boostQuery.getQuery()).getTerm(), equalTo(new Term(STRING_FIELD_NAME, "boosted")));
        queryStringQuery.boost(2.0f);
        query = queryStringQuery.toQuery(shardContext);
        assertThat(query, instanceOf(BoostQuery.class));
        boostQuery = (BoostQuery) query;
        assertThat(boostQuery.getBoost(), equalTo(2.0f));
        assertThat(boostQuery   .getQuery(), instanceOf(BoostQuery.class));
        boostQuery = (BoostQuery) boostQuery.getQuery();
        assertThat(boostQuery.getBoost(), equalTo(2.0f));

        queryStringQuery =
            queryStringQuery("((" + STRING_FIELD_NAME + ":boosted^2) AND (" + STRING_FIELD_NAME + ":foo^1.5))^3");
        query = queryStringQuery.toQuery(shardContext);
        assertThat(query, instanceOf(BoostQuery.class));
        boostQuery = (BoostQuery) query;
        assertThat(boostQuery.getBoost(), equalTo(3.0f));
        BoostQuery boostQuery1 = assertBooleanSubQuery(boostQuery.getQuery(), BoostQuery.class, 0);
        assertThat(boostQuery1.getBoost(), equalTo(2.0f));
        assertThat(boostQuery1.getQuery(), instanceOf(TermQuery.class));
        assertThat(((TermQuery)boostQuery1.getQuery()).getTerm(), equalTo(new Term(STRING_FIELD_NAME, "boosted")));
        BoostQuery boostQuery2 = assertBooleanSubQuery(boostQuery.getQuery(), BoostQuery.class, 1);
        assertThat(boostQuery2.getBoost(), equalTo(1.5f));
        assertThat(boostQuery2.getQuery(), instanceOf(TermQuery.class));
        assertThat(((TermQuery)boostQuery2.getQuery()).getTerm(), equalTo(new Term(STRING_FIELD_NAME, "foo")));
        queryStringQuery.boost(2.0f);
        query = queryStringQuery.toQuery(shardContext);
        assertThat(query, instanceOf(BoostQuery.class));
        boostQuery = (BoostQuery) query;
        assertThat(boostQuery.getBoost(), equalTo(2.0f));
    }

    public void testToQueryMultipleTermsBooleanQuery() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        Query query = queryStringQuery("test1 test2").field(STRING_FIELD_NAME)
            .toQuery(createShardContext());
        assertThat(query, instanceOf(BooleanQuery.class));
        BooleanQuery bQuery = (BooleanQuery) query;
        assertThat(bQuery.clauses().size(), equalTo(2));
        assertThat(assertBooleanSubQuery(query, TermQuery.class, 0).getTerm(),
            equalTo(new Term(STRING_FIELD_NAME, "test1")));
        assertThat(assertBooleanSubQuery(query, TermQuery.class, 1).getTerm(),
            equalTo(new Term(STRING_FIELD_NAME, "test2")));
    }

    public void testToQueryMultipleFieldsBooleanQuery() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        Query query = queryStringQuery("test").field(STRING_FIELD_NAME)
            .field(STRING_FIELD_NAME_2)
            .toQuery(createShardContext());
        assertThat(query, instanceOf(DisjunctionMaxQuery.class));
        DisjunctionMaxQuery bQuery = (DisjunctionMaxQuery) query;
        assertThat(bQuery.getDisjuncts().size(), equalTo(2));
        assertThat(assertDisjunctionSubQuery(query, TermQuery.class, 0).getTerm(),
            equalTo(new Term(STRING_FIELD_NAME, "test")));
        assertThat(assertDisjunctionSubQuery(query, TermQuery.class, 1).getTerm(),
            equalTo(new Term(STRING_FIELD_NAME_2, "test")));
    }

    public void testToQueryMultipleFieldsDisMaxQuery() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        Query query = queryStringQuery("test").field(STRING_FIELD_NAME).field(STRING_FIELD_NAME_2)
            .toQuery(createShardContext());
        assertThat(query, instanceOf(DisjunctionMaxQuery.class));
        DisjunctionMaxQuery disMaxQuery = (DisjunctionMaxQuery) query;
        List<Query> disjuncts = disMaxQuery.getDisjuncts();
        assertThat(((TermQuery) disjuncts.get(0)).getTerm(), equalTo(new Term(STRING_FIELD_NAME, "test")));
        assertThat(((TermQuery) disjuncts.get(1)).getTerm(), equalTo(new Term(STRING_FIELD_NAME_2, "test")));
    }

    public void testToQueryFieldsWildcard() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        Query query = queryStringQuery("test").field("mapped_str*").toQuery(createShardContext());
        assertThat(query, instanceOf(DisjunctionMaxQuery.class));
        DisjunctionMaxQuery dQuery = (DisjunctionMaxQuery) query;
        assertThat(dQuery.getDisjuncts().size(), equalTo(3));
        assertThat(assertDisjunctionSubQuery(query, TermQuery.class, 0).getTerm(),
            equalTo(new Term(STRING_FIELD_NAME, "test")));
        assertThat(assertDisjunctionSubQuery(query, TermQuery.class, 1).getTerm(),
            equalTo(new Term(STRING_FIELD_NAME_2, "test")));
        assertThat(assertDisjunctionSubQuery(query, TermQuery.class, 2).getTerm(),
            equalTo(new Term(STRING_FIELD_NAME, "test")));
    }

    public void testToQueryDisMaxQuery() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        Query query = queryStringQuery("test").field(STRING_FIELD_NAME, 2.2f)
            .field(STRING_FIELD_NAME_2)
            .toQuery(createShardContext());
        assertThat(query, instanceOf(DisjunctionMaxQuery.class));
        DisjunctionMaxQuery disMaxQuery = (DisjunctionMaxQuery) query;
        List<Query> disjuncts = disMaxQuery.getDisjuncts();
        assertTermOrBoostQuery(disjuncts.get(0), STRING_FIELD_NAME, "test", 2.2f);
        assertTermOrBoostQuery(disjuncts.get(1), STRING_FIELD_NAME_2, "test", 1.0f);
    }

    public void testToQueryWildcardQuery() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        for (Operator op : Operator.values()) {
            BooleanClause.Occur defaultOp = op.toBooleanClauseOccur();
            QueryStringQueryParser queryParser = new QueryStringQueryParser(createShardContext(), STRING_FIELD_NAME);
            queryParser.setAnalyzeWildcard(true);
            queryParser.setMultiTermRewriteMethod(MultiTermQuery.CONSTANT_SCORE_REWRITE);
            queryParser.setDefaultOperator(op.toQueryParserOperator());
            Query query = queryParser.parse("first foo-bar-foobar* last");
            Query expectedQuery =
                new BooleanQuery.Builder()
                    .add(new BooleanClause(new TermQuery(new Term(STRING_FIELD_NAME, "first")), defaultOp))
                    .add(new BooleanQuery.Builder()
                        .add(new BooleanClause(new TermQuery(new Term(STRING_FIELD_NAME, "foo")), defaultOp))
                        .add(new BooleanClause(new TermQuery(new Term(STRING_FIELD_NAME, "bar")), defaultOp))
                        .add(new BooleanClause(new PrefixQuery(new Term(STRING_FIELD_NAME, "foobar")), defaultOp))
                        .build(), defaultOp)
                    .add(new BooleanClause(new TermQuery(new Term(STRING_FIELD_NAME, "last")), defaultOp))
                    .build();
            assertThat(query, Matchers.equalTo(expectedQuery));
        }
    }

    public void testToQueryWilcardQueryWithSynonyms() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        for (Operator op : Operator.values()) {
            BooleanClause.Occur defaultOp = op.toBooleanClauseOccur();
            QueryStringQueryParser queryParser = new QueryStringQueryParser(createShardContext(), STRING_FIELD_NAME);
            queryParser.setAnalyzeWildcard(true);
            queryParser.setMultiTermRewriteMethod(MultiTermQuery.CONSTANT_SCORE_REWRITE);
            queryParser.setDefaultOperator(op.toQueryParserOperator());
            queryParser.setForceAnalyzer(new MockRepeatAnalyzer());
            Query query = queryParser.parse("first foo-bar-foobar* last");

            Query expectedQuery = new BooleanQuery.Builder()
                .add(new BooleanClause(new SynonymQuery(new Term(STRING_FIELD_NAME, "first"),
                    new Term(STRING_FIELD_NAME, "first")), defaultOp))
                .add(new BooleanQuery.Builder()
                    .add(new BooleanClause(new SynonymQuery(new Term(STRING_FIELD_NAME, "foo"),
                        new Term(STRING_FIELD_NAME, "foo")), defaultOp))
                    .add(new BooleanClause(new SynonymQuery(new Term(STRING_FIELD_NAME, "bar"),
                        new Term(STRING_FIELD_NAME, "bar")), defaultOp))
                    .add(new BooleanQuery.Builder()
                        .add(new BooleanClause(new PrefixQuery(new Term(STRING_FIELD_NAME, "foobar")),
                            BooleanClause.Occur.SHOULD))
                        .add(new BooleanClause(new PrefixQuery(new Term(STRING_FIELD_NAME, "foobar")),
                            BooleanClause.Occur.SHOULD))
                        .build(), defaultOp)
                    .build(), defaultOp)
                .add(new BooleanClause(new SynonymQuery(new Term(STRING_FIELD_NAME, "last"),
                    new Term(STRING_FIELD_NAME, "last")), defaultOp))
                .build();
            assertThat(query, Matchers.equalTo(expectedQuery));
        }
    }

    public void testToQueryWithGraph() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        for (Operator op : Operator.values()) {
            BooleanClause.Occur defaultOp = op.toBooleanClauseOccur();
            QueryStringQueryParser queryParser = new QueryStringQueryParser(createShardContext(), STRING_FIELD_NAME);
            queryParser.setAnalyzeWildcard(true);
            queryParser.setMultiTermRewriteMethod(MultiTermQuery.CONSTANT_SCORE_REWRITE);
            queryParser.setDefaultOperator(op.toQueryParserOperator());
            queryParser.setAnalyzeWildcard(true);
            queryParser.setMultiTermRewriteMethod(MultiTermQuery.CONSTANT_SCORE_REWRITE);
            queryParser.setDefaultOperator(op.toQueryParserOperator());
            queryParser.setForceAnalyzer(new MockSynonymAnalyzer());
            queryParser.setAutoGenerateMultiTermSynonymsPhraseQuery(false);

            // simple multi-term
            Query query = queryParser.parse("guinea pig");

            Query guineaPig = new BooleanQuery.Builder()
                    .add(new TermQuery(new Term(STRING_FIELD_NAME, "guinea")), Occur.MUST)
                    .add(new TermQuery(new Term(STRING_FIELD_NAME, "pig")), Occur.MUST)
                    .build();
            TermQuery cavy = new TermQuery(new Term(STRING_FIELD_NAME, "cavy"));

            Query expectedQuery = new BooleanQuery.Builder()
                    .add(new BooleanQuery.Builder()
                            .add(guineaPig, Occur.SHOULD)
                            .add(cavy, Occur.SHOULD)
                            .build(),
                            defaultOp).build();
            assertThat(query, Matchers.equalTo(expectedQuery));

            queryParser.setAutoGenerateMultiTermSynonymsPhraseQuery(true);
            // simple multi-term with phrase query
            query = queryParser.parse("guinea pig");
            expectedQuery = new BooleanQuery.Builder()
                    .add(new BooleanQuery.Builder()
                            .add(new PhraseQuery.Builder()
                                .add(new Term(STRING_FIELD_NAME, "guinea"))
                                .add(new Term(STRING_FIELD_NAME, "pig"))
                                .build(), Occur.SHOULD)
                            .add(new TermQuery(new Term(STRING_FIELD_NAME, "cavy")), Occur.SHOULD)
                            .build(), defaultOp)
                    .build();
            assertThat(query, Matchers.equalTo(expectedQuery));
            queryParser.setAutoGenerateMultiTermSynonymsPhraseQuery(false);

            // simple with additional tokens
            query = queryParser.parse("that guinea pig smells");
            expectedQuery = new BooleanQuery.Builder()
                    .add(new TermQuery(new Term(STRING_FIELD_NAME, "that")), defaultOp)
                    .add(new BooleanQuery.Builder()
                         .add(guineaPig, Occur.SHOULD)
                         .add(cavy, Occur.SHOULD).build(), defaultOp)
                    .add(new TermQuery(new Term(STRING_FIELD_NAME, "smells")), defaultOp)
                    .build();
            assertThat(query, Matchers.equalTo(expectedQuery));

            // complex
            query = queryParser.parse("+that -(guinea pig) +smells");
            expectedQuery = new BooleanQuery.Builder()
                    .add(new TermQuery(new Term(STRING_FIELD_NAME, "that")), Occur.MUST)
                    .add(new BooleanQuery.Builder()
                            .add(new BooleanQuery.Builder()
                                    .add(guineaPig, Occur.SHOULD)
                                    .add(cavy, Occur.SHOULD)
                                    .build(), defaultOp)
                            .build(), Occur.MUST_NOT)
                    .add(new TermQuery(new Term(STRING_FIELD_NAME, "smells")), Occur.MUST)
                    .build();

            assertThat(query, Matchers.equalTo(expectedQuery));

            // no parent should cause guinea and pig to be treated as separate tokens
            query = queryParser.parse("+that -guinea pig +smells");
            expectedQuery = new BooleanQuery.Builder()
                .add(new TermQuery(new Term(STRING_FIELD_NAME, "that")), BooleanClause.Occur.MUST)
                .add(new TermQuery(new Term(STRING_FIELD_NAME, "guinea")), BooleanClause.Occur.MUST_NOT)
                .add(new TermQuery(new Term(STRING_FIELD_NAME, "pig")), defaultOp)
                .add(new TermQuery(new Term(STRING_FIELD_NAME, "smells")), BooleanClause.Occur.MUST)
                .build();

            assertThat(query, Matchers.equalTo(expectedQuery));

            // span query
            query = queryParser.parse("\"that guinea pig smells\"");

            expectedQuery = new SpanNearQuery.Builder(STRING_FIELD_NAME, true)
                .addClause(new SpanTermQuery(new Term(STRING_FIELD_NAME, "that")))
                .addClause(
                    new SpanOrQuery(
                        new SpanNearQuery.Builder(STRING_FIELD_NAME, true)
                            .addClause(new SpanTermQuery(new Term(STRING_FIELD_NAME, "guinea")))
                            .addClause(new SpanTermQuery(new Term(STRING_FIELD_NAME, "pig"))).build(),
                        new SpanTermQuery(new Term(STRING_FIELD_NAME, "cavy"))))
                    .addClause(new SpanTermQuery(new Term(STRING_FIELD_NAME, "smells")))
                    .build();
            assertThat(query, Matchers.equalTo(expectedQuery));

            // span query with slop
            query = queryParser.parse("\"that guinea pig smells\"~2");
            expectedQuery = new SpanNearQuery.Builder(STRING_FIELD_NAME, true)
                .addClause(new SpanTermQuery(new Term(STRING_FIELD_NAME, "that")))
                .addClause(
                    new SpanOrQuery(
                        new SpanNearQuery.Builder(STRING_FIELD_NAME, true)
                            .addClause(new SpanTermQuery(new Term(STRING_FIELD_NAME, "guinea")))
                            .addClause(new SpanTermQuery(new Term(STRING_FIELD_NAME, "pig"))).build(),
                        new SpanTermQuery(new Term(STRING_FIELD_NAME, "cavy"))))
                .addClause(new SpanTermQuery(new Term(STRING_FIELD_NAME, "smells")))
                .setSlop(2)
                .build();
            assertThat(query, Matchers.equalTo(expectedQuery));
        }
    }

    public void testToQueryRegExpQuery() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        Query query = queryStringQuery("/foo*bar/").defaultField(STRING_FIELD_NAME)
            .maxDeterminizedStates(5000)
            .toQuery(createShardContext());
        assertThat(query, instanceOf(RegexpQuery.class));
        RegexpQuery regexpQuery = (RegexpQuery) query;
        assertTrue(regexpQuery.toString().contains("/foo*bar/"));
    }

    public void testToQueryRegExpQueryTooComplex() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        QueryStringQueryBuilder queryBuilder = queryStringQuery("/[ac]*a[ac]{50,200}/").defaultField(STRING_FIELD_NAME);

        TooComplexToDeterminizeException e = expectThrows(TooComplexToDeterminizeException.class,
                () -> queryBuilder.toQuery(createShardContext()));
        assertThat(e.getMessage(), containsString("Determinizing [ac]*"));
        assertThat(e.getMessage(), containsString("would result in more than 10000 states"));
    }

    /**
     * Validates that {@code max_determinized_states} can be parsed and lowers the allowed number of determinized states.
     */
    public void testToQueryRegExpQueryMaxDeterminizedStatesParsing() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        XContentBuilder builder = JsonXContent.contentBuilder();
        builder.startObject(); {
            builder.startObject("query_string"); {
                builder.field("query", "/[ac]*a[ac]{1,10}/");
                builder.field("default_field", STRING_FIELD_NAME);
                builder.field("max_determinized_states", 10);
            }
            builder.endObject();
        }
        builder.endObject();

        QueryBuilder queryBuilder = parseInnerQueryBuilder(createParser(builder));
        TooComplexToDeterminizeException e = expectThrows(TooComplexToDeterminizeException.class,
                () -> queryBuilder.toQuery(createShardContext()));
        assertThat(e.getMessage(), containsString("Determinizing [ac]*"));
        assertThat(e.getMessage(), containsString("would result in more than 10 states"));
    }

    /**
     * Validates that {@code max_determinized_states} can be parsed and lowers the allowed number of determinized states.
     */
    public void testEnabledPositionIncrements() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);

        XContentBuilder builder = JsonXContent.contentBuilder();
        builder.startObject(); {
            builder.startObject("query_string"); {
                builder.field("query", "text");
                builder.field("default_field", STRING_FIELD_NAME);
                builder.field("enable_position_increments", false);
            }
            builder.endObject();
        }
        builder.endObject();

        QueryStringQueryBuilder queryBuilder = (QueryStringQueryBuilder) parseInnerQueryBuilder(createParser(builder));
        assertFalse(queryBuilder.enablePositionIncrements());
    }

    public void testToQueryFuzzyQueryAutoFuziness() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);

        int length = randomIntBetween(1, 10);
        StringBuilder queryString = new StringBuilder();
        for (int i = 0; i < length; i++) {
            queryString.append("a");
        }
        queryString.append("~");

        int expectedEdits;
        if (length <= 2) {
            expectedEdits = 0;
        } else if (3 <= length && length <= 5) {
            expectedEdits = 1;
        } else {
            expectedEdits = 2;
        }

        Query query = queryStringQuery(queryString.toString()).defaultField(STRING_FIELD_NAME).fuzziness(Fuzziness.AUTO)
            .toQuery(createShardContext());
        assertThat(query, instanceOf(FuzzyQuery.class));
        FuzzyQuery fuzzyQuery = (FuzzyQuery) query;
        assertEquals(expectedEdits, fuzzyQuery.getMaxEdits());
    }

    public void testFuzzyNumeric() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        QueryStringQueryBuilder query = queryStringQuery("12~0.2").defaultField(INT_FIELD_NAME);
        QueryShardContext context = createShardContext();
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
                () -> query.toQuery(context));
        assertEquals("Can only use fuzzy queries on keyword and text fields - not on [mapped_int] which is of type [integer]",
                e.getMessage());
        query.lenient(true);
        query.toQuery(context); // no exception
    }

    public void testPrefixNumeric() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        QueryStringQueryBuilder query = queryStringQuery("12*").defaultField(INT_FIELD_NAME);
        QueryShardContext context = createShardContext();
        QueryShardException e = expectThrows(QueryShardException.class,
                () -> query.toQuery(context));
        assertEquals("Can only use prefix queries on keyword and text fields - not on [mapped_int] which is of type [integer]",
                e.getMessage());
        query.lenient(true);
        query.toQuery(context); // no exception
    }

    public void testExactGeo() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        QueryStringQueryBuilder query = queryStringQuery("2,3").defaultField(GEO_POINT_FIELD_NAME);
        QueryShardContext context = createShardContext();
        QueryShardException e = expectThrows(QueryShardException.class,
                () -> query.toQuery(context));
        assertEquals("Geo fields do not support exact searching, use dedicated geo queries instead: [mapped_geo_point]",
                e.getMessage());
        query.lenient(true);
        query.toQuery(context); // no exception
    }

    public void testTimezone() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        String queryAsString = "{\n" +
                "    \"query_string\":{\n" +
                "        \"time_zone\":\"Europe/Paris\",\n" +
                "        \"query\":\"" + DATE_FIELD_NAME + ":[2012 TO 2014]\"\n" +
                "    }\n" +
                "}";
        QueryBuilder queryBuilder = parseQuery(queryAsString);
        assertThat(queryBuilder, instanceOf(QueryStringQueryBuilder.class));
        QueryStringQueryBuilder queryStringQueryBuilder = (QueryStringQueryBuilder) queryBuilder;
        assertThat(queryStringQueryBuilder.timeZone(), equalTo(DateTimeZone.forID("Europe/Paris")));

        String invalidQueryAsString = "{\n" +
                "    \"query_string\":{\n" +
                "        \"time_zone\":\"This timezone does not exist\",\n" +
                "        \"query\":\"" + DATE_FIELD_NAME + ":[2012 TO 2014]\"\n" +
                "    }\n" +
                "}";
        expectThrows(IllegalArgumentException.class, () -> parseQuery(invalidQueryAsString));
    }

    public void testToQueryBooleanQueryMultipleBoosts() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        int numBoosts = randomIntBetween(2, 10);
        float[] boosts = new float[numBoosts + 1];
        String queryStringPrefix = "";
        String queryStringSuffix = "";
        for (int i = 0; i < boosts.length - 1; i++) {
            float boost = 2.0f / randomIntBetween(3, 20);
            boosts[i] = boost;
            queryStringPrefix += "(";
            queryStringSuffix += ")^" + boost;
        }
        String queryString = queryStringPrefix + "foo bar" + queryStringSuffix;

        float mainBoost = 2.0f / randomIntBetween(3, 20);
        boosts[boosts.length - 1] = mainBoost;
        QueryStringQueryBuilder queryStringQueryBuilder =
            new QueryStringQueryBuilder(queryString).field(STRING_FIELD_NAME)
                .minimumShouldMatch("2").boost(mainBoost);
        Query query = queryStringQueryBuilder.toQuery(createShardContext());

        for (int i = boosts.length - 1; i >= 0; i--) {
            assertThat(query, instanceOf(BoostQuery.class));
            BoostQuery boostQuery = (BoostQuery) query;
            assertThat(boostQuery.getBoost(), equalTo(boosts[i]));
            query = boostQuery.getQuery();
        }

        assertThat(query, instanceOf(BooleanQuery.class));
        BooleanQuery booleanQuery = (BooleanQuery) query;
        assertThat(booleanQuery.getMinimumNumberShouldMatch(), equalTo(2));
        assertThat(booleanQuery.clauses().get(0).getOccur(), equalTo(BooleanClause.Occur.SHOULD));
        assertThat(booleanQuery.clauses().get(0).getQuery(),
            equalTo(new TermQuery(new Term(STRING_FIELD_NAME, "foo"))));
        assertThat(booleanQuery.clauses().get(1).getOccur(), equalTo(BooleanClause.Occur.SHOULD));
        assertThat(booleanQuery.clauses().get(1).getQuery(),
            equalTo(new TermQuery(new Term(STRING_FIELD_NAME, "bar"))));
    }

    public void testToQueryPhraseQueryBoostAndSlop() throws IOException {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        QueryStringQueryBuilder queryStringQueryBuilder =
            new QueryStringQueryBuilder("\"test phrase\"~2").field(STRING_FIELD_NAME, 5f);
        Query query = queryStringQueryBuilder.toQuery(createShardContext());
        assertThat(query, instanceOf(BoostQuery.class));
        BoostQuery boostQuery = (BoostQuery) query;
        assertThat(boostQuery.getBoost(), equalTo(5f));
        assertThat(boostQuery.getQuery(), instanceOf(PhraseQuery.class));
        PhraseQuery phraseQuery = (PhraseQuery) boostQuery.getQuery();
        assertThat(phraseQuery.getSlop(), Matchers.equalTo(2));
        assertThat(phraseQuery.getTerms().length, equalTo(2));
    }

    public void testToQueryWildcardNonExistingFields() throws IOException {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        QueryStringQueryBuilder queryStringQueryBuilder =
            new QueryStringQueryBuilder("foo bar").field("invalid*");
        Query query = queryStringQueryBuilder.toQuery(createShardContext());

        Query expectedQuery = new MatchNoDocsQuery("empty fields");
        assertThat(expectedQuery, equalTo(query));

        queryStringQueryBuilder =
            new QueryStringQueryBuilder(STRING_FIELD_NAME + ":foo bar").field("invalid*");
        query = queryStringQueryBuilder.toQuery(createShardContext());
        expectedQuery = new BooleanQuery.Builder()
            .add(new TermQuery(new Term(STRING_FIELD_NAME, "foo")), Occur.SHOULD)
            .add(new MatchNoDocsQuery("empty fields"), Occur.SHOULD)
            .build();
        assertThat(expectedQuery, equalTo(query));
    }

    public void testToQueryTextParsing() throws IOException {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        {
            QueryStringQueryBuilder queryBuilder =
                new QueryStringQueryBuilder("foo bar")
                    .field(STRING_FIELD_NAME).field(STRING_FIELD_NAME_2);
            Query query = queryBuilder.toQuery(createShardContext());
            BooleanQuery bq1 =
                new BooleanQuery.Builder()
                    .add(new BooleanClause(new TermQuery(new Term(STRING_FIELD_NAME, "foo")), BooleanClause.Occur.SHOULD))
                    .add(new BooleanClause(new TermQuery(new Term(STRING_FIELD_NAME, "bar")), BooleanClause.Occur.SHOULD))
                    .build();
            List<Query> disjuncts = new ArrayList<>();
            disjuncts.add(bq1);
            disjuncts.add(new TermQuery(new Term(STRING_FIELD_NAME_2, "foo bar")));
            DisjunctionMaxQuery expectedQuery = new DisjunctionMaxQuery(disjuncts, 0.0f);
            assertThat(query, equalTo(expectedQuery));
        }

        //  type=phrase
        {
            QueryStringQueryBuilder queryBuilder =
                new QueryStringQueryBuilder("foo bar")
                    .field(STRING_FIELD_NAME).field(STRING_FIELD_NAME_2);
            queryBuilder.type(MultiMatchQueryBuilder.Type.PHRASE);
            Query query = queryBuilder.toQuery(createShardContext());

            List<Query> disjuncts = new ArrayList<>();
            PhraseQuery pq = new PhraseQuery.Builder()
                .add(new Term(STRING_FIELD_NAME, "foo"))
                .add(new Term(STRING_FIELD_NAME, "bar"))
                .build();
            disjuncts.add(pq);
            disjuncts.add(new TermQuery(new Term(STRING_FIELD_NAME_2, "foo bar")));
            DisjunctionMaxQuery expectedQuery = new DisjunctionMaxQuery(disjuncts, 0.0f);
            assertThat(query, equalTo(expectedQuery));
        }

        {
            QueryStringQueryBuilder queryBuilder =
                new QueryStringQueryBuilder("mapped_string:other foo bar")
                    .field(STRING_FIELD_NAME).field(STRING_FIELD_NAME_2);
            Query query = queryBuilder.toQuery(createShardContext());
            BooleanQuery bq1 =
                new BooleanQuery.Builder()
                    .add(new BooleanClause(new TermQuery(new Term(STRING_FIELD_NAME, "foo")), BooleanClause.Occur.SHOULD))
                    .add(new BooleanClause(new TermQuery(new Term(STRING_FIELD_NAME, "bar")), BooleanClause.Occur.SHOULD))
                    .build();
            List<Query> disjuncts = new ArrayList<>();
            disjuncts.add(bq1);
            disjuncts.add(new TermQuery(new Term(STRING_FIELD_NAME_2, "foo bar")));
            DisjunctionMaxQuery disjunctionMaxQuery = new DisjunctionMaxQuery(disjuncts, 0.0f);
            BooleanQuery expectedQuery =
                new BooleanQuery.Builder()
                    .add(disjunctionMaxQuery, BooleanClause.Occur.SHOULD)
                    .add(new TermQuery(new Term(STRING_FIELD_NAME, "other")), BooleanClause.Occur.SHOULD)
                    .build();
            assertThat(query, equalTo(expectedQuery));
        }

        {
            QueryStringQueryBuilder queryBuilder =
                new QueryStringQueryBuilder("foo OR bar")
                    .field(STRING_FIELD_NAME).field(STRING_FIELD_NAME_2);
            Query query = queryBuilder.toQuery(createShardContext());

            List<Query> disjuncts1 = new ArrayList<>();
            disjuncts1.add(new TermQuery(new Term(STRING_FIELD_NAME, "foo")));
            disjuncts1.add(new TermQuery(new Term(STRING_FIELD_NAME_2, "foo")));
            DisjunctionMaxQuery maxQuery1 = new DisjunctionMaxQuery(disjuncts1, 0.0f);

            List<Query> disjuncts2 = new ArrayList<>();
            disjuncts2.add(new TermQuery(new Term(STRING_FIELD_NAME, "bar")));
            disjuncts2.add(new TermQuery(new Term(STRING_FIELD_NAME_2, "bar")));
            DisjunctionMaxQuery maxQuery2 = new DisjunctionMaxQuery(disjuncts2, 0.0f);

            BooleanQuery expectedQuery =
                new BooleanQuery.Builder()
                    .add(new BooleanClause(maxQuery1, BooleanClause.Occur.SHOULD))
                    .add(new BooleanClause(maxQuery2, BooleanClause.Occur.SHOULD))
                    .build();
            assertThat(query, equalTo(expectedQuery));
        }

        // non-prefix queries do not work with range queries simple syntax
        {
            // throws an exception when lenient is set to false
            QueryStringQueryBuilder queryBuilder =
                new QueryStringQueryBuilder(">10 foo")
                    .field(INT_FIELD_NAME);
            IllegalArgumentException exc =
                expectThrows(IllegalArgumentException.class, () -> queryBuilder.toQuery(createShardContext()));
            assertThat(exc.getMessage(), equalTo("For input string: \">10 foo\""));
        }
    }

    public void testExistsFieldQuery() throws Exception {
        QueryShardContext context = createShardContext();
        QueryStringQueryBuilder queryBuilder = new QueryStringQueryBuilder(STRING_FIELD_NAME + ":*");
        Query query = queryBuilder.toQuery(context);
        if (getCurrentTypes().length > 0) {
            if (context.getIndexSettings().getIndexVersionCreated().onOrAfter(Version.V_6_1_0)
                    && (context.fieldMapper(STRING_FIELD_NAME).omitNorms() == false)) {
                assertThat(query, equalTo(new ConstantScoreQuery(new NormsFieldExistsQuery(STRING_FIELD_NAME))));
            } else {
                assertThat(query, equalTo(new ConstantScoreQuery(new TermQuery(new Term("_field_names", STRING_FIELD_NAME)))));
            }
        } else {
            assertThat(query, equalTo(new MatchNoDocsQuery()));
        }

        queryBuilder = new QueryStringQueryBuilder("*:*");
        query = queryBuilder.toQuery(context);
        Query expected = new MatchAllDocsQuery();
        assertThat(query, equalTo(expected));

        queryBuilder = new QueryStringQueryBuilder("*");
        query = queryBuilder.toQuery(context);
        expected = new MatchAllDocsQuery();
        assertThat(query, equalTo(expected));
    }

    public void testDisabledFieldNamesField() throws Exception {
        assumeTrue("No types", getCurrentTypes().length > 0);
        QueryShardContext context = createShardContext();
        context.getMapperService().merge("_doc",
            new CompressedXContent(
                Strings.toString(PutMappingRequest.buildFromSimplifiedDef("_doc",
                    "foo", "type=text",
                    "_field_names", "enabled=false"))),
            MapperService.MergeReason.MAPPING_UPDATE);
        try {
            QueryStringQueryBuilder queryBuilder = new QueryStringQueryBuilder("foo:*");
            Query query = queryBuilder.toQuery(context);
            Query expected = new WildcardQuery(new Term("foo", "*"));
            assertThat(query, equalTo(expected));
        } finally {
            // restore mappings as they were before
            context.getMapperService().merge("_doc",
                new CompressedXContent(
                    Strings.toString(PutMappingRequest.buildFromSimplifiedDef("_doc",
                        "foo", "type=text",
                        "_field_names", "enabled=true"))),
                MapperService.MergeReason.MAPPING_UPDATE);
        }
    }



    public void testFromJson() throws IOException {
        String json =
                "{\n" +
                "  \"query_string\" : {\n" +
                "    \"query\" : \"this AND that OR thus\",\n" +
                "    \"default_field\" : \"content\",\n" +
                "    \"fields\" : [ ],\n" +
                "    \"type\" : \"best_fields\",\n" +
                "    \"tie_breaker\" : 0.0,\n" +
                "    \"default_operator\" : \"or\",\n" +
                "    \"max_determinized_states\" : 10000,\n" +
                "    \"enable_position_increments\" : true,\n" +
                "    \"fuzziness\" : \"AUTO\",\n" +
                "    \"fuzzy_prefix_length\" : 0,\n" +
                "    \"fuzzy_max_expansions\" : 50,\n" +
                "    \"phrase_slop\" : 0,\n" +
                "    \"escape\" : false,\n" +
                "    \"auto_generate_synonyms_phrase_query\" : true,\n" +
                "    \"fuzzy_transpositions\" : false,\n" +
                "    \"boost\" : 1.0\n" +
                "  }\n" +
                "}";

        QueryStringQueryBuilder parsed = (QueryStringQueryBuilder) parseQuery(json);
        checkGeneratedJson(json, parsed);

        assertEquals(json, "this AND that OR thus", parsed.queryString());
        assertEquals(json, "content", parsed.defaultField());
        assertEquals(json, false, parsed.fuzzyTranspositions());
    }

    public void testExpandedTerms() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        // Prefix
        Query query = new QueryStringQueryBuilder("aBc*")
                .field(STRING_FIELD_NAME)
                .analyzer("whitespace")
                .toQuery(createShardContext());
        assertEquals(new PrefixQuery(new Term(STRING_FIELD_NAME, "aBc")), query);
        query = new QueryStringQueryBuilder("aBc*")
                .field(STRING_FIELD_NAME)
                .analyzer("standard")
                .toQuery(createShardContext());
        assertEquals(new PrefixQuery(new Term(STRING_FIELD_NAME, "abc")), query);

        // Wildcard
        query = new QueryStringQueryBuilder("aBc*D")
                .field(STRING_FIELD_NAME)
                .analyzer("whitespace")
                .toQuery(createShardContext());
        assertEquals(new WildcardQuery(new Term(STRING_FIELD_NAME, "aBc*D")), query);
        query = new QueryStringQueryBuilder("aBc*D")
                .field(STRING_FIELD_NAME)
                .analyzer("standard")
                .toQuery(createShardContext());
        assertEquals(new WildcardQuery(new Term(STRING_FIELD_NAME, "abc*d")), query);

        // Fuzzy
        query = new QueryStringQueryBuilder("aBc~1")
                .field(STRING_FIELD_NAME)
                .analyzer("whitespace")
                .toQuery(createShardContext());
        FuzzyQuery fuzzyQuery = (FuzzyQuery) query;
        assertEquals(new Term(STRING_FIELD_NAME, "aBc"), fuzzyQuery.getTerm());
        query = new QueryStringQueryBuilder("aBc~1")
                .field(STRING_FIELD_NAME)
                .analyzer("standard")
                .toQuery(createShardContext());
        fuzzyQuery = (FuzzyQuery) query;
        assertEquals(new Term(STRING_FIELD_NAME, "abc"), fuzzyQuery.getTerm());

        // Range
        query = new QueryStringQueryBuilder("[aBc TO BcD]")
                .field(STRING_FIELD_NAME)
                .analyzer("whitespace")
                .toQuery(createShardContext());
        assertEquals(new TermRangeQuery(STRING_FIELD_NAME, new BytesRef("aBc"), new BytesRef("BcD"), true, true), query);
        query = new QueryStringQueryBuilder("[aBc TO BcD]")
                .field(STRING_FIELD_NAME)
                .analyzer("standard")
                .toQuery(createShardContext());
        assertEquals(new TermRangeQuery(STRING_FIELD_NAME, new BytesRef("abc"), new BytesRef("bcd"), true, true), query);
    }

    public void testDefaultFieldsWithFields() throws IOException {
        QueryShardContext context = createShardContext();
        QueryStringQueryBuilder builder = new QueryStringQueryBuilder("aBc*")
            .field("field")
            .defaultField("*");
        QueryValidationException e = expectThrows(QueryValidationException.class, () -> builder.toQuery(context));
        assertThat(e.getMessage(),
            containsString("cannot use [fields] parameter in conjunction with [default_field]"));
    }

    public void testLenientRewriteToMatchNoDocs() throws IOException {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        // Term
        Query query = new QueryStringQueryBuilder("hello")
            .field(INT_FIELD_NAME)
            .lenient(true)
            .toQuery(createShardContext());
        assertEquals(new MatchNoDocsQuery(""), query);

        // prefix
        query = new QueryStringQueryBuilder("hello*")
            .field(INT_FIELD_NAME)
            .lenient(true)
            .toQuery(createShardContext());
        assertEquals(new MatchNoDocsQuery(""), query);

        // Fuzzy
        query = new QueryStringQueryBuilder("hello~2")
            .field(INT_FIELD_NAME)
            .lenient(true)
            .toQuery(createShardContext());
        assertEquals(new MatchNoDocsQuery(""), query);
    }

    public void testUnmappedFieldRewriteToMatchNoDocs() throws IOException {
        // Default unmapped field
        Query query = new QueryStringQueryBuilder("hello")
            .field("unmapped_field")
            .lenient(true)
            .toQuery(createShardContext());
        assertEquals(new MatchNoDocsQuery(""), query);

        // Unmapped prefix field
        query = new QueryStringQueryBuilder("unmapped_field:hello")
            .lenient(true)
            .toQuery(createShardContext());
        assertEquals(new MatchNoDocsQuery(""), query);

        // Unmapped fields
        query = new QueryStringQueryBuilder("hello")
            .lenient(true)
            .field("unmapped_field")
            .toQuery(createShardContext());
        assertEquals(new MatchNoDocsQuery(""), query);
    }

    public void testDefaultField() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        QueryShardContext context = createShardContext();
        context.getIndexSettings().updateIndexMetaData(
            newIndexMeta("index", context.getIndexSettings().getSettings(), Settings.builder().putList("index.query.default_field",
                STRING_FIELD_NAME, STRING_FIELD_NAME_2 + "^5").build())
        );
        Query query = new QueryStringQueryBuilder("hello")
            .toQuery(context);
        Query expected = new DisjunctionMaxQuery(
            Arrays.asList(
                new TermQuery(new Term(STRING_FIELD_NAME, "hello")),
                new BoostQuery(new TermQuery(new Term(STRING_FIELD_NAME_2, "hello")), 5.0f)
            ), 0.0f
        );
        assertEquals(expected, query);
        // Reset the default value
        context.getIndexSettings().updateIndexMetaData(
            newIndexMeta("index",
                context.getIndexSettings().getSettings(), Settings.builder().putList("index.query.default_field", "*").build())
        );
    }

    /**
     * the quote analyzer should overwrite any other forced analyzer in quoted parts of the query
     */
    public void testQuoteAnalyzer() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        // Prefix
        Query query = new QueryStringQueryBuilder("ONE \"TWO THREE\"")
                .field(STRING_FIELD_NAME)
                .analyzer("whitespace")
                .quoteAnalyzer("simple")
                .toQuery(createShardContext());
        Query expectedQuery =
                new BooleanQuery.Builder()
                        .add(new BooleanClause(new TermQuery(new Term(STRING_FIELD_NAME, "ONE")), Occur.SHOULD))
                        .add(new BooleanClause(new PhraseQuery.Builder()
                                .add(new Term(STRING_FIELD_NAME, "two"), 0)
                                .add(new Term(STRING_FIELD_NAME, "three"), 1)
                                .build(), Occur.SHOULD))
                    .build();
        assertEquals(expectedQuery, query);
    }

    public void testQuoteFieldSuffix() throws IOException {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        QueryShardContext context = createShardContext();
        assertEquals(new TermQuery(new Term(STRING_FIELD_NAME, "bar")),
            new QueryStringQueryBuilder("bar")
                .quoteFieldSuffix("_2")
                .field(STRING_FIELD_NAME)
                .doToQuery(context)
        );
        assertEquals(new TermQuery(new Term(STRING_FIELD_NAME_2, "bar")),
            new QueryStringQueryBuilder("\"bar\"")
                .quoteFieldSuffix("_2")
                .field(STRING_FIELD_NAME)
                .doToQuery(context)
        );

        // Now check what happens if the quote field does not exist
        assertEquals(new TermQuery(new Term(STRING_FIELD_NAME, "bar")),
            new QueryStringQueryBuilder("bar")
                .quoteFieldSuffix(".quote")
                .field(STRING_FIELD_NAME)
                .doToQuery(context)
        );
        assertEquals(new TermQuery(new Term(STRING_FIELD_NAME, "bar")),
            new QueryStringQueryBuilder("\"bar\"")
                .quoteFieldSuffix(".quote")
                .field(STRING_FIELD_NAME)
                .doToQuery(context)
        );
    }

    public void testToFuzzyQuery() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);

        Query query = new QueryStringQueryBuilder("text~2")
            .field(STRING_FIELD_NAME)
            .fuzzyPrefixLength(2)
            .fuzzyMaxExpansions(5)
            .fuzzyTranspositions(false)
            .toQuery(createShardContext());
        FuzzyQuery expected = new FuzzyQuery(new Term(STRING_FIELD_NAME, "text"), 2, 2, 5, false);
        assertEquals(expected, query);
    }

    public void testWithStopWords() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        Query query = new QueryStringQueryBuilder("the quick fox")
            .field(STRING_FIELD_NAME)
            .analyzer("stop")
            .toQuery(createShardContext());
        BooleanQuery expected = new BooleanQuery.Builder()
            .add(new TermQuery(new Term(STRING_FIELD_NAME, "quick")), Occur.SHOULD)
            .add(new TermQuery(new Term(STRING_FIELD_NAME, "fox")), Occur.SHOULD)
            .build();
        assertEquals(expected, query);
    }

    public void testWithPrefixStopWords() throws Exception {
        assumeTrue("test runs only when at least a type is registered", getCurrentTypes().length > 0);
        Query query = new QueryStringQueryBuilder("the* quick fox")
            .field(STRING_FIELD_NAME)
            .analyzer("stop")
            .toQuery(createShardContext());
        BooleanQuery expected = new BooleanQuery.Builder()
            .add(new PrefixQuery(new Term(STRING_FIELD_NAME, "the")), Occur.SHOULD)
            .add(new TermQuery(new Term(STRING_FIELD_NAME, "quick")), Occur.SHOULD)
            .add(new TermQuery(new Term(STRING_FIELD_NAME, "fox")), Occur.SHOULD)
            .build();
        assertEquals(expected, query);
    }

    private static IndexMetaData newIndexMeta(String name, Settings oldIndexSettings, Settings indexSettings) {
        Settings build = Settings.builder().put(oldIndexSettings)
            .put(indexSettings)
            .build();
        return IndexMetaData.builder(name).settings(build).build();
    }
}
