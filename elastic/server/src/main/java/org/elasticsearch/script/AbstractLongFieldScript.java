/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.script;

import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.util.ArrayUtil;
import org.elasticsearch.search.lookup.SearchLookup;

import java.util.Arrays;
import java.util.Map;
import java.util.function.LongConsumer;

/**
 * Common base class for script field scripts that return long values.
 */
public abstract class AbstractLongFieldScript extends AbstractFieldScript {
    private long[] values = new long[1];
    private int count;

    public AbstractLongFieldScript(String fieldName, Map<String, Object> params, SearchLookup searchLookup, LeafReaderContext ctx) {
        super(fieldName, params, searchLookup, ctx);
    }

    /**
     * Execute the script for the provided {@code docId}.
     */
    public final void runForDoc(int docId) {
        count = 0;
        setDocument(docId);
        execute();
    }

    /**
     * Execute the script for the provided {@code docId}, passing results to the {@code consumer}
     */
    public final void runForDoc(int docId, LongConsumer consumer) {
        runForDoc(docId);
        for (int i = 0; i < count; i++) {
            consumer.accept(values[i]);
        }
    }

    /**
     * Values from the last time {@link #runForDoc(int)} was called. This array
     * is mutable and will change with the next call of {@link #runForDoc(int)}.
     * It is also oversized and will contain garbage at all indices at and
     * above {@link #count()}.
     */
    public final long[] values() {
        return values;
    }

    /**
     * Reorders the values from the last time {@link #values()} was called to
     * how this would appear in doc-values order. Truncates garbage values
     * based on {@link #count()}.
     */
    public final long[] asDocValues() {
        long[] truncated = Arrays.copyOf(values, count());
        Arrays.sort(truncated);
        return truncated;
    }

    /**
     * The number of results produced the last time {@link #runForDoc(int)} was called.
     */
    public final int count() {
        return count;
    }

    public final void emit(long v) {
        checkMaxSize(count);
        if (values.length < count + 1) {
            values = ArrayUtil.grow(values, count + 1);
        }
        values[count++] = v;
    }
}
