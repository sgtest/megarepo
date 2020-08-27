/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.runtimefields.mapper;

import org.apache.lucene.index.IndexReader;
import org.elasticsearch.common.geo.ShapeRelation;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.index.query.QueryShardContext;
import org.elasticsearch.search.lookup.SearchLookup;
import org.elasticsearch.test.ESTestCase;

import java.io.IOException;

import static org.hamcrest.Matchers.equalTo;
import static org.mockito.Matchers.any;
import static org.mockito.Matchers.anyString;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

abstract class AbstractScriptMappedFieldTypeTestCase extends ESTestCase {
    protected abstract AbstractScriptMappedFieldType simpleMappedFieldType() throws IOException;

    protected abstract String runtimeType();

    @SuppressWarnings("unused")
    public abstract void testDocValues() throws IOException;

    @SuppressWarnings("unused")
    public abstract void testSort() throws IOException;

    @SuppressWarnings("unused")
    public abstract void testUsedInScript() throws IOException;

    @SuppressWarnings("unused")
    public abstract void testExistsQuery() throws IOException;

    @SuppressWarnings("unused")
    public abstract void testExistsQueryIsExpensive() throws IOException;

    @SuppressWarnings("unused")
    public abstract void testRangeQuery() throws IOException;

    @SuppressWarnings("unused")
    public abstract void testRangeQueryIsExpensive() throws IOException;

    @SuppressWarnings("unused")
    public abstract void testTermQuery() throws IOException;

    @SuppressWarnings("unused")
    public abstract void testTermQueryIsExpensive() throws IOException;

    @SuppressWarnings("unused")
    public abstract void testTermsQuery() throws IOException;

    @SuppressWarnings("unused")
    public abstract void testTermsQueryIsExpensive() throws IOException;

    protected static QueryShardContext mockContext() {
        return mockContext(true);
    }

    protected static QueryShardContext mockContext(boolean allowExpensiveQueries) {
        return mockContext(allowExpensiveQueries, null);
    }

    protected static QueryShardContext mockContext(boolean allowExpensiveQueries, AbstractScriptMappedFieldType mappedFieldType) {
        MapperService mapperService = mock(MapperService.class);
        when(mapperService.fieldType(anyString())).thenReturn(mappedFieldType);
        QueryShardContext context = mock(QueryShardContext.class);
        when(context.getMapperService()).thenReturn(mapperService);
        if (mappedFieldType != null) {
            when(context.fieldMapper(anyString())).thenReturn(mappedFieldType);
            when(context.getSearchAnalyzer(any())).thenReturn(mappedFieldType.getTextSearchInfo().getSearchAnalyzer());
        }
        when(context.allowExpensiveQueries()).thenReturn(allowExpensiveQueries);
        SearchLookup lookup = new SearchLookup(
            mapperService,
            (mft, lookupSupplier) -> mft.fielddataBuilder("test", lookupSupplier).build(null, null, mapperService)
        );
        when(context.lookup()).thenReturn(lookup);
        return context;
    }

    public void testRangeQueryWithShapeRelationIsError() throws IOException {
        Exception e = expectThrows(
            IllegalArgumentException.class,
            () -> simpleMappedFieldType().rangeQuery(1, 2, true, true, ShapeRelation.DISJOINT, null, null, null)
        );
        assertThat(
            e.getMessage(),
            equalTo("Field [test] of type [runtime_script] with runtime type [" + runtimeType() + "] does not support DISJOINT ranges")
        );
    }

    public void testPhraseQueryIsError() {
        assertQueryOnlyOnText("phrase", () -> simpleMappedFieldType().phraseQuery(null, 1, false));
    }

    public void testPhrasePrefixQueryIsError() {
        assertQueryOnlyOnText("phrase prefix", () -> simpleMappedFieldType().phrasePrefixQuery(null, 1, 1));
    }

    public void testMultiPhraseQueryIsError() {
        assertQueryOnlyOnText("phrase", () -> simpleMappedFieldType().multiPhraseQuery(null, 1, false));
    }

    public void testSpanPrefixQueryIsError() {
        assertQueryOnlyOnText("span prefix", () -> simpleMappedFieldType().spanPrefixQuery(null, null, null));
    }

    private void assertQueryOnlyOnText(String queryName, ThrowingRunnable buildQuery) {
        Exception e = expectThrows(IllegalArgumentException.class, buildQuery);
        assertThat(
            e.getMessage(),
            equalTo(
                "Can only use "
                    + queryName
                    + " queries on text fields - not on [test] which is of type [script] with runtime_type ["
                    + runtimeType()
                    + "]"
            )
        );
    }

    protected String readSource(IndexReader reader, int docId) throws IOException {
        return reader.document(docId).getBinaryValue("_source").utf8ToString();
    }
}
