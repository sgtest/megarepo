/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.apache.lucene.search.MatchNoDocsQuery;
import org.apache.lucene.search.Query;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.common.Booleans;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.lucene.search.Queries;
import org.elasticsearch.common.time.DateMathParser;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.index.fielddata.BooleanScriptFieldData;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.script.BooleanFieldScript;
import org.elasticsearch.script.Script;
import org.elasticsearch.search.DocValueFormat;
import org.elasticsearch.search.lookup.SearchLookup;
import org.elasticsearch.search.runtime.BooleanScriptFieldExistsQuery;
import org.elasticsearch.search.runtime.BooleanScriptFieldTermQuery;

import java.time.ZoneId;
import java.util.Collection;
import java.util.Collections;
import java.util.Map;
import java.util.function.Supplier;

public final class BooleanScriptFieldType extends AbstractScriptFieldType<BooleanFieldScript.LeafFactory> {

    private static final BooleanFieldScript.Factory PARSE_FROM_SOURCE
        = (field, params, lookup) -> (BooleanFieldScript.LeafFactory) ctx -> new BooleanFieldScript
        (
            field,
            params,
            lookup,
            ctx
        ) {
        @Override
        public void execute() {
            for (Object v : extractFromSource(field)) {
                if (v instanceof Boolean) {
                    emit((Boolean) v);
                } else if (v instanceof String) {
                    try {
                        emit(Booleans.parseBoolean((String) v));
                    } catch (IllegalArgumentException e) {
                        // ignore
                    }
                }
            }
        }
    };

    public static final RuntimeField.Parser PARSER = new RuntimeField.Parser(name ->
        new Builder<>(name, BooleanFieldScript.CONTEXT, PARSE_FROM_SOURCE) {
            @Override
            RuntimeField newRuntimeField(BooleanFieldScript.Factory scriptFactory) {
                return new BooleanScriptFieldType(name, scriptFactory, getScript(), meta(), this);
            }
        });

    public BooleanScriptFieldType(String name) {
        this(name, PARSE_FROM_SOURCE, null, Collections.emptyMap(), (builder, params) -> builder);
    }

    BooleanScriptFieldType(
        String name,
        BooleanFieldScript.Factory scriptFactory,
        Script script,
        Map<String, String> meta,
        ToXContent toXContent
    ) {
        super(name, searchLookup -> scriptFactory.newFactory(name, script.getParams(), searchLookup), script, meta, toXContent);
    }

    @Override
    public String typeName() {
        return BooleanFieldMapper.CONTENT_TYPE;
    }

    @Override
    public Object valueForDisplay(Object value) {
        if (value == null) {
            return null;
        }
        switch (value.toString()) {
            case "F":
                return false;
            case "T":
                return true;
            default:
                throw new IllegalArgumentException("Expected [T] or [F] but got [" + value + "]");
        }
    }

    @Override
    public DocValueFormat docValueFormat(String format, ZoneId timeZone) {
        if (format != null) {
            throw new IllegalArgumentException("Field [" + name() + "] of type [" + typeName() + "] does not support custom formats");
        }
        if (timeZone != null) {
            throw new IllegalArgumentException("Field [" + name() + "] of type [" + typeName() + "] does not support custom time zones");
        }
        return DocValueFormat.BOOLEAN;
    }

    @Override
    public BooleanScriptFieldData.Builder fielddataBuilder(String fullyQualifiedIndexName, Supplier<SearchLookup> searchLookup) {
        return new BooleanScriptFieldData.Builder(name(), leafFactory(searchLookup.get()));
    }

    @Override
    public Query existsQuery(SearchExecutionContext context) {
        checkAllowExpensiveQueries(context);
        return new BooleanScriptFieldExistsQuery(script, leafFactory(context), name());
    }

    @Override
    public Query rangeQuery(
        Object lowerTerm,
        Object upperTerm,
        boolean includeLower,
        boolean includeUpper,
        ZoneId timeZone,
        DateMathParser parser,
        SearchExecutionContext context
    ) {
        boolean trueAllowed;
        boolean falseAllowed;

        /*
         * gte: true --- true matches
         * gt: true ---- none match
         * gte: false -- both match
         * gt: false --- true matches
         */
        if (toBoolean(lowerTerm)) {
            if (includeLower) {
                trueAllowed = true;
                falseAllowed = false;
            } else {
                trueAllowed = false;
                falseAllowed = false;
            }
        } else {
            if (includeLower) {
                trueAllowed = true;
                falseAllowed = true;
            } else {
                trueAllowed = true;
                falseAllowed = false;
            }
        }

        /*
         * This is how the indexed version works:
         * lte: true --- both match
         * lt: true ---- false matches
         * lte: false -- false matches
         * lt: false --- none match
         */
        if (toBoolean(upperTerm)) {
            if (includeUpper) {
                trueAllowed &= true;
                falseAllowed &= true;
            } else {
                trueAllowed &= false;
                falseAllowed &= true;
            }
        } else {
            if (includeUpper) {
                trueAllowed &= false;
                falseAllowed &= true;
            } else {
                trueAllowed &= false;
                falseAllowed &= false;
            }
        }

        return termsQuery(trueAllowed, falseAllowed, context);
    }

    @Override
    public Query termQueryCaseInsensitive(Object value, SearchExecutionContext context) {
        checkAllowExpensiveQueries(context);
        return new BooleanScriptFieldTermQuery(script, leafFactory(context.lookup()), name(), toBoolean(value, true));
    }

    @Override
    public Query termQuery(Object value, SearchExecutionContext context) {
        checkAllowExpensiveQueries(context);
        return new BooleanScriptFieldTermQuery(script, leafFactory(context), name(), toBoolean(value, false));
    }

    @Override
    public Query termsQuery(Collection<?> values, SearchExecutionContext context) {
        if (values.isEmpty()) {
            return Queries.newMatchNoDocsQuery("Empty terms query");
        }
        boolean trueAllowed = false;
        boolean falseAllowed = false;
        for (Object value : values) {
            if (toBoolean(value, false)) {
                trueAllowed = true;
            } else {
                falseAllowed = true;
            }
        }
        return termsQuery(trueAllowed, falseAllowed, context);
    }

    private Query termsQuery(boolean trueAllowed, boolean falseAllowed, SearchExecutionContext context) {
        if (trueAllowed) {
            if (falseAllowed) {
                // Either true or false
                return existsQuery(context);
            }
            checkAllowExpensiveQueries(context);
            return new BooleanScriptFieldTermQuery(script, leafFactory(context), name(), true);
        }
        if (falseAllowed) {
            checkAllowExpensiveQueries(context);
            return new BooleanScriptFieldTermQuery(script, leafFactory(context), name(), false);
        }
        return new MatchNoDocsQuery("neither true nor false allowed");
    }

    private static boolean toBoolean(Object value) {
        return toBoolean(value, false);
    }

    /**
     * Convert the term into a boolean. Inspired by {@link BooleanFieldMapper.BooleanFieldType#indexedValueForSearch(Object)}.
     */
    private static boolean toBoolean(Object value, boolean caseInsensitive) {
        if (value == null) {
            return false;
        }
        if (value instanceof Boolean) {
            return (Boolean) value;
        }
        String sValue;
        if (value instanceof BytesRef) {
            sValue = ((BytesRef) value).utf8ToString();
        } else {
            sValue = value.toString();
        }
        if (caseInsensitive) {
            sValue = Strings.toLowercaseAscii(sValue);
        }
        return Booleans.parseBoolean(sValue);
    }
}
