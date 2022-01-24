/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.apache.lucene.document.SortedSetDocValuesField;
import org.apache.lucene.index.Term;
import org.apache.lucene.sandbox.search.DocValuesTermsQuery;
import org.apache.lucene.search.Query;
import org.apache.lucene.search.TermInSetQuery;
import org.apache.lucene.search.TermQuery;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.common.lucene.BytesRefs;
import org.elasticsearch.common.lucene.search.AutomatonQueries;
import org.elasticsearch.index.query.SearchExecutionContext;

import java.util.Collection;
import java.util.Map;

/** Base {@link MappedFieldType} implementation for a field that is indexed
 *  with the inverted index. */
public abstract class TermBasedFieldType extends SimpleMappedFieldType {

    public TermBasedFieldType(
        String name,
        boolean isIndexed,
        boolean isStored,
        boolean hasDocValues,
        TextSearchInfo textSearchInfo,
        Map<String, String> meta
    ) {
        super(name, isIndexed, isStored, hasDocValues, textSearchInfo, meta);
    }

    protected boolean allowDocValueBasedQueries() {
        return false;
    }

    /** Returns the indexed value used to construct search "values".
     *  This method is used for the default implementations of most
     *  query factory methods such as {@link #termQuery}. */
    protected BytesRef indexedValueForSearch(Object value) {
        return BytesRefs.toBytesRef(value);
    }

    @Override
    public Query termQueryCaseInsensitive(Object value, SearchExecutionContext context) {
        failIfNotIndexed();
        return AutomatonQueries.caseInsensitiveTermQuery(new Term(name(), indexedValueForSearch(value)));
    }

    @Override
    public boolean mayExistInIndex(SearchExecutionContext context) {
        return context.fieldExistsInIndex(name());
    }

    @Override
    public Query termQuery(Object value, SearchExecutionContext context) {
        if (allowDocValueBasedQueries()) {
            failIfNotIndexedNorDocValuesFallback(context);
        } else {
            failIfNotIndexed();
        }
        if (isIndexed()) {
            return new TermQuery(new Term(name(), indexedValueForSearch(value)));
        } else {
            return SortedSetDocValuesField.newSlowExactQuery(name(), indexedValueForSearch(value));
        }
    }

    @Override
    public Query termsQuery(Collection<?> values, SearchExecutionContext context) {
        if (allowDocValueBasedQueries()) {
            failIfNotIndexedNorDocValuesFallback(context);
        } else {
            failIfNotIndexed();
        }
        BytesRef[] bytesRefs = values.stream().map(this::indexedValueForSearch).toArray(BytesRef[]::new);
        if (isIndexed()) {
            return new TermInSetQuery(name(), bytesRefs);
        } else {
            return new DocValuesTermsQuery(name(), bytesRefs);
        }
    }

}
