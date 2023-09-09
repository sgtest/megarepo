/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.compute.data;

import org.apache.lucene.util.RamUsageEstimator;
import org.elasticsearch.core.Nullable;

import java.util.BitSet;

public final class BlockRamUsageEstimator {

    /** Returns the size in bytes of the int[] object. Otherwise, returns 0 if null. */
    public static long sizeOf(@Nullable int[] arr) {
        return arr == null ? 0 : RamUsageEstimator.sizeOf(arr);
    }

    /** Returns the size in bytes used by the bitset. Otherwise, returns 0 if null. Not exact, but good enough */
    public static long sizeOfBitSet(@Nullable BitSet bitset) {
        return bitset == null ? 0 : RamUsageEstimator.shallowSizeOfInstance(BitSet.class) + (bitset.size() / Byte.SIZE);
    }
}
