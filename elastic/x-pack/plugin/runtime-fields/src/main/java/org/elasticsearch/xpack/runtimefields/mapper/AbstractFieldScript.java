/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.runtimefields.mapper;

import org.apache.lucene.index.LeafReaderContext;
import org.elasticsearch.common.xcontent.support.XContentMapValues;
import org.elasticsearch.index.fielddata.ScriptDocValues;
import org.elasticsearch.script.AggregationScript;
import org.elasticsearch.script.DynamicMap;
import org.elasticsearch.script.ScriptCache;
import org.elasticsearch.script.ScriptContext;
import org.elasticsearch.search.lookup.LeafSearchLookup;
import org.elasticsearch.search.lookup.SearchLookup;
import org.elasticsearch.search.lookup.SourceLookup;

import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.function.Function;

import static org.elasticsearch.common.unit.TimeValue.timeValueMillis;

/**
 * Abstract base for scripts to execute to build scripted fields. Inspired by
 * {@link AggregationScript} but hopefully with less historical baggage.
 */
public abstract class AbstractFieldScript {
    /**
     * The maximum number of values a script should be allowed to emit.
     */
    static final int MAX_VALUES = 100;

    static <F> ScriptContext<F> newContext(String name, Class<F> factoryClass) {
        return new ScriptContext<>(
            name + "_script_field",
            factoryClass,
            /*
             * We rely on the script cache in two ways:
             * 1. It caches the "heavy" part of mappings generated at runtime.
             * 2. Mapping updates tend to try to compile the script twice. Not
             *    for any good reason. They just do.
             * Thus we use the default 100.
             */
            100,
            timeValueMillis(0),
            /*
             * Disable compilation rate limits for runtime fields so we
             * don't prevent mapping updates because we've performed too
             * many recently. That'd just be lame. We also compile these
             * scripts during search requests so this could totally be a
             * source of runaway script compilations. We think folks will
             * mostly reuse scripts though.
             */
            ScriptCache.UNLIMITED_COMPILATION_RATE.asTuple()
        );
    }

    private static final Map<String, Function<Object, Object>> PARAMS_FUNCTIONS = Map.of(
        "_source",
        value -> ((SourceLookup) value).source()
    );

    protected final String fieldName;
    private final Map<String, Object> params;
    protected final LeafSearchLookup leafSearchLookup;

    public AbstractFieldScript(String fieldName, Map<String, Object> params, SearchLookup searchLookup, LeafReaderContext ctx) {
        this.fieldName = fieldName;
        this.leafSearchLookup = searchLookup.getLeafSearchLookup(ctx);
        params = new HashMap<>(params);
        params.put("_source", leafSearchLookup.source());
        params.put("_fields", leafSearchLookup.fields());
        this.params = new DynamicMap(params, PARAMS_FUNCTIONS);
    }

    /**
     * Set the document to run the script against.
     */
    public final void setDocument(int docId) {
        this.leafSearchLookup.setDocument(docId);
    }

    /**
     * Expose the {@code params} of the script to the script itself.
     */
    public final Map<String, Object> getParams() {
        return params;
    }

    /**
     * Expose field data to the script as {@code doc}.
     */
    public final Map<String, ScriptDocValues<?>> getDoc() {
        return leafSearchLookup.doc();
    }

    protected final List<Object> extractFromSource(String path) {
        return XContentMapValues.extractRawValues(path, leafSearchLookup.source().source());
    }

    /**
     * Check if the we can add another value to the list of values.
     * @param currentSize the current size of the list
     */
    protected final void checkMaxSize(int currentSize) {
        if (currentSize >= MAX_VALUES) {
            throw new IllegalArgumentException(
                String.format(
                    Locale.ROOT,
                    "Runtime field [%s] is emitting [%s] values while the maximum number of values allowed is [%s]",
                    fieldName,
                    currentSize + 1,
                    MAX_VALUES
                )
            );
        }
    }

    public abstract void execute();
}
