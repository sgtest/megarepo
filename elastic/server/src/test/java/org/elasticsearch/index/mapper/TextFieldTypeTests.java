/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.index.mapper;

import org.apache.lucene.index.Term;
import org.apache.lucene.search.AutomatonQuery;
import org.apache.lucene.search.BooleanClause;
import org.apache.lucene.search.BooleanQuery;
import org.apache.lucene.search.ConstantScoreQuery;
import org.apache.lucene.search.FuzzyQuery;
import org.apache.lucene.search.PrefixQuery;
import org.apache.lucene.search.Query;
import org.apache.lucene.search.RegexpQuery;
import org.apache.lucene.search.TermInSetQuery;
import org.apache.lucene.search.TermQuery;
import org.apache.lucene.search.TermRangeQuery;
import org.apache.lucene.util.BytesRef;
import org.apache.lucene.util.automaton.Automata;
import org.apache.lucene.util.automaton.Automaton;
import org.apache.lucene.util.automaton.Operations;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.common.lucene.BytesRefs;
import org.elasticsearch.common.lucene.search.AutomatonQueries;
import org.elasticsearch.common.unit.Fuzziness;
import org.elasticsearch.index.mapper.TextFieldMapper.TextFieldType;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;

import static org.apache.lucene.search.MultiTermQuery.CONSTANT_SCORE_REWRITE;
import static org.hamcrest.Matchers.equalTo;

public class TextFieldTypeTests extends FieldTypeTestCase {

    private static TextFieldType createFieldType() {
        return new TextFieldType("field");
    }

    public void testIsAggregatableDependsOnFieldData() {
        TextFieldType ft = createFieldType();
        assertFalse(ft.isAggregatable());
        ft.setFielddata(true);
        assertTrue(ft.isAggregatable());
    }

    public void testTermQuery() {
        MappedFieldType ft = createFieldType();
        assertEquals(new TermQuery(new Term("field", "foo")), ft.termQuery("foo", null));
        assertEquals(AutomatonQueries.caseInsensitiveTermQuery(new Term("field", "fOo")), ft.termQueryCaseInsensitive("fOo", null));

        MappedFieldType unsearchable = new TextFieldType("field", false, false, Collections.emptyMap());
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
                () -> unsearchable.termQuery("bar", null));
        assertEquals("Cannot search on field [field] since it is not indexed.", e.getMessage());
    }

    public void testTermsQuery() {
        MappedFieldType ft = createFieldType();
        List<BytesRef> terms = new ArrayList<>();
        terms.add(new BytesRef("foo"));
        terms.add(new BytesRef("bar"));
        assertEquals(new TermInSetQuery("field", terms),
                ft.termsQuery(Arrays.asList("foo", "bar"), null));

        MappedFieldType unsearchable = new TextFieldType("field", false, false, Collections.emptyMap());
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
                () -> unsearchable.termsQuery(Arrays.asList("foo", "bar"), null));
        assertEquals("Cannot search on field [field] since it is not indexed.", e.getMessage());
    }

    public void testRangeQuery() {
        MappedFieldType ft = createFieldType();
        assertEquals(new TermRangeQuery("field", BytesRefs.toBytesRef("foo"), BytesRefs.toBytesRef("bar"), true, false),
                ft.rangeQuery("foo", "bar", true, false, null, null, null, MOCK_CONTEXT));

        ElasticsearchException ee = expectThrows(ElasticsearchException.class,
                () -> ft.rangeQuery("foo", "bar", true, false, null, null, null, MOCK_CONTEXT_DISALLOW_EXPENSIVE));
        assertEquals("[range] queries on [text] or [keyword] fields cannot be executed when " +
                        "'search.allow_expensive_queries' is set to false.", ee.getMessage());
    }

    public void testRegexpQuery() {
        MappedFieldType ft = createFieldType();
        assertEquals(new RegexpQuery(new Term("field","foo.*")),
                ft.regexpQuery("foo.*", 0, 0, 10, null, MOCK_CONTEXT));

        MappedFieldType unsearchable = new TextFieldType("field", false, false, Collections.emptyMap());
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
                () -> unsearchable.regexpQuery("foo.*", 0, 0, 10, null, MOCK_CONTEXT));
        assertEquals("Cannot search on field [field] since it is not indexed.", e.getMessage());

        ElasticsearchException ee = expectThrows(ElasticsearchException.class,
                () -> ft.regexpQuery("foo.*", randomInt(10), 0, randomInt(10) + 1, null, MOCK_CONTEXT_DISALLOW_EXPENSIVE));
        assertEquals("[regexp] queries cannot be executed when 'search.allow_expensive_queries' is set to false.",
                ee.getMessage());
    }

    public void testFuzzyQuery() {
        MappedFieldType ft = createFieldType();
        assertEquals(new FuzzyQuery(new Term("field","foo"), 2, 1, 50, true),
                ft.fuzzyQuery("foo", Fuzziness.fromEdits(2), 1, 50, true, MOCK_CONTEXT));

        MappedFieldType unsearchable = new TextFieldType("field", false, false, Collections.emptyMap());
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
                () -> unsearchable.fuzzyQuery("foo", Fuzziness.fromEdits(2), 1, 50, true, MOCK_CONTEXT));
        assertEquals("Cannot search on field [field] since it is not indexed.", e.getMessage());

        ElasticsearchException ee = expectThrows(ElasticsearchException.class,
                () -> ft.fuzzyQuery("foo", Fuzziness.AUTO, randomInt(10) + 1, randomInt(10) + 1,
                        randomBoolean(), MOCK_CONTEXT_DISALLOW_EXPENSIVE));
        assertEquals("[fuzzy] queries cannot be executed when 'search.allow_expensive_queries' is set to false.",
                ee.getMessage());
    }

    public void testIndexPrefixes() {
        TextFieldType ft = createFieldType();
        ft.setIndexPrefixes(2, 10);

        Query q = ft.prefixQuery("goin", CONSTANT_SCORE_REWRITE, false, randomMockContext());
        assertEquals(new ConstantScoreQuery(new TermQuery(new Term("field._index_prefix", "goin"))), q);

        q = ft.prefixQuery("internationalisatio", CONSTANT_SCORE_REWRITE, false, MOCK_CONTEXT);
        assertEquals(new PrefixQuery(new Term("field", "internationalisatio")), q);

        q = ft.prefixQuery("Internationalisatio", CONSTANT_SCORE_REWRITE, true, MOCK_CONTEXT);
        assertEquals(AutomatonQueries.caseInsensitivePrefixQuery(new Term("field", "Internationalisatio")), q);


        ElasticsearchException ee = expectThrows(ElasticsearchException.class,
                () -> ft.prefixQuery("internationalisatio", null, false, MOCK_CONTEXT_DISALLOW_EXPENSIVE));
        assertEquals("[prefix] queries cannot be executed when 'search.allow_expensive_queries' is set to false. " +
                "For optimised prefix queries on text fields please enable [index_prefixes].", ee.getMessage());

        q = ft.prefixQuery("g", CONSTANT_SCORE_REWRITE, false, randomMockContext());
        Automaton automaton
            = Operations.concatenate(Arrays.asList(Automata.makeChar('g'), Automata.makeAnyChar()));

        Query expected = new ConstantScoreQuery(new BooleanQuery.Builder()
            .add(new AutomatonQuery(new Term("field._index_prefix", "g*"), automaton), BooleanClause.Occur.SHOULD)
            .add(new TermQuery(new Term("field", "g")), BooleanClause.Occur.SHOULD)
            .build());

        assertThat(q, equalTo(expected));
    }

    public void testFetchSourceValue() throws IOException {
        TextFieldType fieldType = createFieldType();

        assertEquals(List.of("value"), fetchSourceValue(fieldType, "value"));
        assertEquals(List.of("42"), fetchSourceValue(fieldType, 42L));
        assertEquals(List.of("true"), fetchSourceValue(fieldType, true));
    }
}
