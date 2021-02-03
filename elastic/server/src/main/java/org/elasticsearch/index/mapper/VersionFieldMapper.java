/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.apache.lucene.document.Field;
import org.apache.lucene.document.NumericDocValuesField;
import org.apache.lucene.search.Query;
import org.elasticsearch.index.mapper.ParseContext.Document;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.index.query.QueryShardException;

import java.util.Collections;

/** Mapper for the _version field. */
public class VersionFieldMapper extends MetadataFieldMapper {

    public static final String NAME = "_version";
    public static final String CONTENT_TYPE = "_version";

    public static final TypeParser PARSER = new FixedTypeParser(c -> new VersionFieldMapper());

    static final class VersionFieldType extends MappedFieldType {

        public static final VersionFieldType INSTANCE = new VersionFieldType();

        private VersionFieldType() {
            super(NAME, false, false, true, TextSearchInfo.NONE, Collections.emptyMap());
        }

        @Override
        public String typeName() {
            return CONTENT_TYPE;
        }

        @Override
        public Query termQuery(Object value, SearchExecutionContext context) {
            throw new QueryShardException(context, "The _version field is not searchable");
        }

        @Override
        public ValueFetcher valueFetcher(SearchExecutionContext context, String format) {
            throw new UnsupportedOperationException("Cannot fetch values for internal field [" + name() + "].");
        }
    }

    private VersionFieldMapper() {
        super(VersionFieldType.INSTANCE);
    }

    @Override
    public void preParse(ParseContext context) {
        // see InternalEngine.updateVersion to see where the real version value is set
        final Field version = new NumericDocValuesField(NAME, -1L);
        context.version(version);
        context.doc().add(version);
    }

    @Override
    public void postParse(ParseContext context) {
        // In the case of nested docs, let's fill nested docs with version=1 so that Lucene doesn't write a Bitset for documents
        // that don't have the field. This is consistent with the default value for efficiency.
        Field version = context.version();
        assert version != null;
        for (Document doc : context.nonRootDocuments()) {
            doc.add(version);
        }
    }

    @Override
    protected String contentType() {
        return CONTENT_TYPE;
    }
}
