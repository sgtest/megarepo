/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.join.mapper;

import org.apache.lucene.document.Field;
import org.apache.lucene.document.FieldType;
import org.apache.lucene.document.SortedDocValuesField;
import org.apache.lucene.index.IndexOptions;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.common.lucene.Lucene;
import org.elasticsearch.index.fielddata.IndexFieldData;
import org.elasticsearch.index.fielddata.plain.SortedSetOrdinalsIndexFieldData;
import org.elasticsearch.index.mapper.FieldMapper;
import org.elasticsearch.index.mapper.ParseContext;
import org.elasticsearch.index.mapper.StringFieldType;
import org.elasticsearch.index.mapper.TextSearchInfo;
import org.elasticsearch.index.mapper.ValueFetcher;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.search.aggregations.support.CoreValuesSourceType;
import org.elasticsearch.search.lookup.SearchLookup;

import java.util.Collections;
import java.util.function.Supplier;

/**
 * A field mapper used internally by the {@link ParentJoinFieldMapper} to index
 * the value that link documents in the index (parent _id or _id if the document is a parent).
 */
public final class ParentIdFieldMapper extends FieldMapper {
    static final String CONTENT_TYPE = "parent";

    static class Defaults {
        static final FieldType FIELD_TYPE = new FieldType();

        static {
            FIELD_TYPE.setTokenized(false);
            FIELD_TYPE.setOmitNorms(true);
            FIELD_TYPE.setIndexOptions(IndexOptions.DOCS);
            FIELD_TYPE.freeze();
        }
    }

    public static final class ParentIdFieldType extends StringFieldType {
        public ParentIdFieldType(String name, boolean eagerGlobalOrdinals) {
            super(name, true, false, true, TextSearchInfo.SIMPLE_MATCH_ONLY, Collections.emptyMap());
            setEagerGlobalOrdinals(eagerGlobalOrdinals);
        }

        @Override
        public String typeName() {
            return CONTENT_TYPE;
        }

        @Override
        public IndexFieldData.Builder fielddataBuilder(String fullyQualifiedIndexName, Supplier<SearchLookup> searchLookup) {
            failIfNoDocValues();
            return new SortedSetOrdinalsIndexFieldData.Builder(name(), CoreValuesSourceType.KEYWORD);
        }

        @Override
        public ValueFetcher valueFetcher(SearchExecutionContext context, String format) {
            throw new UnsupportedOperationException("Cannot fetch values for internal field [" + typeName() + "].");
        }

        @Override
        public Object valueForDisplay(Object value) {
            if (value == null) {
                return null;
            }
            BytesRef binaryValue = (BytesRef) value;
            return binaryValue.utf8ToString();
        }
    }

    protected ParentIdFieldMapper(String name, boolean eagerGlobalOrdinals) {
        super(name, new ParentIdFieldType(name, eagerGlobalOrdinals), Lucene.KEYWORD_ANALYZER, MultiFields.empty(), CopyTo.empty());
    }

    @Override
    protected void parseCreateField(ParseContext context) {
        if (context.externalValueSet() == false) {
            throw new IllegalStateException("external value not set");
        }
        String refId = (String) context.externalValue();
        BytesRef binaryValue = new BytesRef(refId);
        Field field = new Field(fieldType().name(), binaryValue, Defaults.FIELD_TYPE);
        context.doc().add(field);
        context.doc().add(new SortedDocValuesField(fieldType().name(), binaryValue));
    }

    @Override
    protected String contentType() {
        return CONTENT_TYPE;
    }

    @Override
    public Builder getMergeBuilder() {
        return null;    // always constructed by ParentJoinFieldMapper, not through type parsers
    }
}
