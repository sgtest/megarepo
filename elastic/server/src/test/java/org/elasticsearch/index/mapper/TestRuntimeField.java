/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.apache.lucene.search.Query;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.index.query.SearchExecutionContext;

import java.io.IOException;
import java.util.Collection;
import java.util.Collections;

public final class TestRuntimeField implements RuntimeField {
    private final String name;
    private final Collection<MappedFieldType> subfields;

    public TestRuntimeField(String name, String type) {
        this(name, Collections.singleton(new TestRuntimeFieldType(name, type)));
    }

    public TestRuntimeField(String name, Collection<MappedFieldType> subfields) {
        this.name = name;
        this.subfields = subfields;
    }

    @Override
    public String name() {
        return name;
    }

    @Override
    public Collection<MappedFieldType> asMappedFieldTypes() {
        return subfields;
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        throw new UnsupportedOperationException();
    }

    public static class TestRuntimeFieldType extends MappedFieldType {
        private final String type;

        public TestRuntimeFieldType(String name, String type) {
            super(name, false, false, false, TextSearchInfo.NONE, Collections.emptyMap());
            this.type = type;
        }

        @Override
        public ValueFetcher valueFetcher(SearchExecutionContext context, String format) {
            throw new UnsupportedOperationException();
        }

        @Override
        public String typeName() {
            return type;
        }

        @Override
        public Query termQuery(Object value, SearchExecutionContext context) {
            return null;
        }
    }
}
