/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.script;

import org.apache.lucene.index.LeafReaderContext;
import org.elasticsearch.search.lookup.SearchLookup;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.function.Consumer;

public abstract class StringFieldScript extends AbstractFieldScript {
    /**
     * The maximum number of chars a script should be allowed to emit.
     */
    public static final long MAX_CHARS = 1024 * 1024;

    public static final ScriptContext<Factory> CONTEXT = newContext("keyword_field", Factory.class);

    @SuppressWarnings("unused")
    public static final String[] PARAMETERS = {};

    public interface Factory extends ScriptFactory {
        LeafFactory newFactory(String fieldName, Map<String, Object> params, SearchLookup searchLookup);
    }

    public interface LeafFactory {
        StringFieldScript newInstance(LeafReaderContext ctx);
    }

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

    public final void runForDoc(int docId, Consumer<String> consumer) {
        resultsForDoc(docId).forEach(consumer);
    }

    /**
     * Reorders the values from the last time {@link #resultsForDoc(int)} was called to
     * how this would appear in doc-values order.
     */
    public final String[] asDocValues() {
        String[] values = new String[results.size()];
        results.toArray(values);
        Arrays.sort(values);
        return values;
    }

    public final void emit(String v) {
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
