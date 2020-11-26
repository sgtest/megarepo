/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.runtimefields.mapper;

import org.apache.lucene.index.LeafReaderContext;
import org.elasticsearch.painless.spi.Whitelist;
import org.elasticsearch.painless.spi.WhitelistLoader;
import org.elasticsearch.script.ScriptContext;
import org.elasticsearch.script.ScriptFactory;
import org.elasticsearch.search.lookup.SearchLookup;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;

public abstract class StringFieldScript extends AbstractFieldScript {
    /**
     * The maximum number of chars a script should be allowed to emit.
     */
    public static final long MAX_CHARS = 1024 * 1024;

    public static final ScriptContext<Factory> CONTEXT = newContext("string_script_field", Factory.class);

    static List<Whitelist> whitelist() {
        return List.of(WhitelistLoader.loadFromResourceFiles(RuntimeFieldsPainlessExtension.class, "string_whitelist.txt"));
    }

    @SuppressWarnings("unused")
    public static final String[] PARAMETERS = {};

    public interface Factory extends ScriptFactory {
        LeafFactory newFactory(String fieldName, Map<String, Object> params, SearchLookup searchLookup);
    }

    public interface LeafFactory {
        StringFieldScript newInstance(LeafReaderContext ctx);
    }

    public static final Factory PARSE_FROM_SOURCE = (field, params, lookup) -> (LeafFactory) ctx -> new StringFieldScript(
        field,
        params,
        lookup,
        ctx
    ) {
        @Override
        public void execute() {
            for (Object v : extractFromSource(field)) {
                if (v != null) {
                    emit(v.toString());
                }
            }
        }
    };

    private final List<String> results = new ArrayList<>();
    private long chars;

    public StringFieldScript(String fieldName, Map<String, Object> params, SearchLookup searchLookup, LeafReaderContext ctx) {
        super(fieldName, params, searchLookup, ctx);
    }

    /**
     * Execute the script for the provided {@code docId}.
     * <p>
     * @return a mutable {@link List} that contains the results of the script
     * and will be modified the next time you call {@linkplain #resultsForDoc}.
     */
    public final List<String> resultsForDoc(int docId) {
        results.clear();
        chars = 0;
        setDocument(docId);
        execute();
        return results;
    }

    protected final void emit(String v) {
        checkMaxSize(results.size());
        chars += v.length();
        if (chars > MAX_CHARS) {
            throw new IllegalArgumentException(
                String.format(
                    Locale.ROOT,
                    "Runtime field [%s] is emitting [%s] characters while the maximum number of values allowed is [%s]",
                    fieldName,
                    chars,
                    MAX_CHARS
                )
            );
        }
        results.add(v);
    }

    public static class Emit {
        private final StringFieldScript script;

        public Emit(StringFieldScript script) {
            this.script = script;
        }

        public void emit(String v) {
            script.emit(v);
        }
    }
}
