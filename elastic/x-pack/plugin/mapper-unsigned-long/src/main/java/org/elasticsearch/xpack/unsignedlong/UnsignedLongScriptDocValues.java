/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.unsignedlong;

import org.apache.lucene.index.SortedNumericDocValues;
import org.apache.lucene.util.ArrayUtil;
import org.elasticsearch.index.fielddata.ScriptDocValues;

import java.io.IOException;

import static org.elasticsearch.search.DocValueFormat.MASK_2_63;

public class UnsignedLongScriptDocValues extends ScriptDocValues<Long> {

    private final SortedNumericDocValues in;
    private long[] values = new long[0];
    private int count;

    /**
     * Standard constructor.
     */
    public UnsignedLongScriptDocValues(SortedNumericDocValues in) {
        this.in = in;
    }

    @Override
    public void setNextDocId(int docId) throws IOException {
        if (in.advanceExact(docId)) {
            resize(in.docValueCount());
            for (int i = 0; i < count; i++) {
                values[i] = in.nextValue();
            }
        } else {
            resize(0);
        }
    }

    /**
     * Set the {@link #size()} and ensure that the {@link #values} array can
     * store at least that many entries.
     */
    protected void resize(int newSize) {
        count = newSize;
        values = ArrayUtil.grow(values, count);
    }

    public long getValue() {
        return get(0);
    }

    @Override
    public Long get(int index) {
        throwIfEmpty();
        return format(index);
    }

    /**
     * Applies the formatting from {@link org.elasticsearch.search.DocValueFormat.UnsignedLongShiftedDocValueFormat#format(long)} so
     * that the underlying value can be treated as a primitive long as that method returns either a {@code long} or a {@code BigInteger}.
     */
    protected long format(int index) {
        return values[index] ^ MASK_2_63;
    }

    @Override
    public int size() {
        return count;
    }
}
