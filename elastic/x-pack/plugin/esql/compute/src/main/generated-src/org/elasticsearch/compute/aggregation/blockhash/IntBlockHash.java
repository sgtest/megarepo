/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.compute.aggregation.blockhash;

import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.common.util.BitArray;
import org.elasticsearch.common.util.LongHash;
import org.elasticsearch.compute.aggregation.GroupingAggregatorFunction;
import org.elasticsearch.compute.aggregation.SeenGroupIds;
import org.elasticsearch.compute.data.Block;
import org.elasticsearch.compute.data.BlockFactory;
import org.elasticsearch.compute.data.IntBlock;
import org.elasticsearch.compute.data.IntVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.MultivalueDedupe;
import org.elasticsearch.compute.operator.MultivalueDedupeInt;

import java.util.BitSet;

/**
 * Maps a {@link IntBlock} column to group ids.
 */
final class IntBlockHash extends BlockHash {
    private final int channel;
    private final LongHash hash;

    /**
     * Have we seen any {@code null} values?
     * <p>
     *     We reserve the 0 ordinal for the {@code null} key so methods like
     *     {@link #nonEmpty} need to skip 0 if we haven't seen any null values.
     * </p>
     */
    private boolean seenNull;

    IntBlockHash(int channel, BlockFactory blockFactory) {
        super(blockFactory);
        this.channel = channel;
        this.hash = new LongHash(1, blockFactory.bigArrays());
    }

    @Override
    public void add(Page page, GroupingAggregatorFunction.AddInput addInput) {
        var block = page.getBlock(channel);
        if (block.areAllValuesNull()) {
            seenNull = true;
            try (IntVector groupIds = blockFactory.newConstantIntVector(0, block.getPositionCount())) {
                addInput.add(0, groupIds);
            }
            return;
        }
        IntBlock castBlock = (IntBlock) block;
        IntVector vector = castBlock.asVector();
        if (vector == null) {
            try (IntBlock groupIds = add(castBlock)) {
                addInput.add(0, groupIds);
            }
            return;
        }
        try (IntVector groupIds = add(vector)) {
            addInput.add(0, groupIds);
        }
    }

    private IntVector add(IntVector vector) {
        int positions = vector.getPositionCount();
        try (var builder = blockFactory.newIntVectorFixedBuilder(positions)) {
            for (int i = 0; i < positions; i++) {
                int v = vector.getInt(i);
                builder.appendInt(Math.toIntExact(hashOrdToGroupNullReserved(hash.add(v))));
            }
            return builder.build();
        }
    }

    private IntBlock add(IntBlock block) {
        MultivalueDedupe.HashResult result = new MultivalueDedupeInt(block).hashAdd(blockFactory, hash);
        seenNull |= result.sawNull();
        return result.ords();
    }

    @Override
    public IntBlock[] getKeys() {
        if (seenNull) {
            final int size = Math.toIntExact(hash.size() + 1);
            final int[] keys = new int[size];
            for (int i = 1; i < size; i++) {
                keys[i] = (int) hash.get(i - 1);
            }
            BitSet nulls = new BitSet(1);
            nulls.set(0);
            return new IntBlock[] {
                blockFactory.newIntArrayBlock(keys, keys.length, null, nulls, Block.MvOrdering.DEDUPLICATED_AND_SORTED_ASCENDING) };
        }
        final int size = Math.toIntExact(hash.size());
        final int[] keys = new int[size];
        for (int i = 0; i < size; i++) {
            keys[i] = (int) hash.get(i);
        }
        return new IntBlock[] { blockFactory.newIntArrayVector(keys, keys.length).asBlock() };
    }

    @Override
    public IntVector nonEmpty() {
        return IntVector.range(seenNull ? 0 : 1, Math.toIntExact(hash.size() + 1), blockFactory);
    }

    @Override
    public BitArray seenGroupIds(BigArrays bigArrays) {
        return new SeenGroupIds.Range(seenNull ? 0 : 1, Math.toIntExact(hash.size() + 1)).seenGroupIds(bigArrays);
    }

    @Override
    public void close() {
        hash.close();
    }

    @Override
    public String toString() {
        StringBuilder b = new StringBuilder();
        b.append("IntBlockHash{channel=").append(channel);
        b.append(", entries=").append(hash.size());
        b.append(", seenNull=").append(seenNull);
        return b.append('}').toString();
    }
}
