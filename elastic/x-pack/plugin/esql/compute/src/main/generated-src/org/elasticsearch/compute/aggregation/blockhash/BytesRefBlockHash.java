/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.compute.aggregation.blockhash;

import org.apache.lucene.util.BytesRef;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.common.util.BitArray;
import org.elasticsearch.common.util.BytesRefArray;
import org.elasticsearch.common.util.BytesRefHash;
import org.elasticsearch.compute.aggregation.GroupingAggregatorFunction;
import org.elasticsearch.compute.aggregation.SeenGroupIds;
import org.elasticsearch.compute.data.BlockFactory;
import org.elasticsearch.compute.data.BytesRefBlock;
import org.elasticsearch.compute.data.BytesRefVector;
import org.elasticsearch.compute.data.IntBlock;
import org.elasticsearch.compute.data.IntVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.MultivalueDedupe;
import org.elasticsearch.compute.operator.MultivalueDedupeBytesRef;

import java.io.IOException;

/**
 * Maps a {@link BytesRefBlock} column to group ids.
 */
final class BytesRefBlockHash extends BlockHash {
    private final int channel;
    private final BytesRefHash hash;

    /**
     * Have we seen any {@code null} values?
     * <p>
     *     We reserve the 0 ordinal for the {@code null} key so methods like
     *     {@link #nonEmpty} need to skip 0 if we haven't seen any null values.
     * </p>
     */
    private boolean seenNull;

    BytesRefBlockHash(int channel, BlockFactory blockFactory) {
        super(blockFactory);
        this.channel = channel;
        this.hash = new BytesRefHash(1, blockFactory.bigArrays());
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
        BytesRefBlock castBlock = (BytesRefBlock) block;
        BytesRefVector vector = castBlock.asVector();
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

    private IntVector add(BytesRefVector vector) {
        BytesRef scratch = new BytesRef();
        int positions = vector.getPositionCount();
        try (var builder = blockFactory.newIntVectorFixedBuilder(positions)) {
            for (int i = 0; i < positions; i++) {
                BytesRef v = vector.getBytesRef(i, scratch);
                builder.appendInt(Math.toIntExact(hashOrdToGroupNullReserved(hash.add(v))));
            }
            return builder.build();
        }
    }

    private IntBlock add(BytesRefBlock block) {
        MultivalueDedupe.HashResult result = new MultivalueDedupeBytesRef(block).hashAdd(blockFactory, hash);
        seenNull |= result.sawNull();
        return result.ords();
    }

    @Override
    public BytesRefBlock[] getKeys() {
        /*
         * Create an un-owned copy of the data so we can close our BytesRefHash
         * without and still read from the block.
         */
        // TODO replace with takeBytesRefsOwnership ?!
        if (seenNull) {
            try (var builder = blockFactory.newBytesRefBlockBuilder(Math.toIntExact(hash.size() + 1))) {
                builder.appendNull();
                BytesRef spare = new BytesRef();
                for (long i = 0; i < hash.size(); i++) {
                    builder.appendBytesRef(hash.get(i, spare));
                }
                return new BytesRefBlock[] { builder.build() };
            }
        }

        final int size = Math.toIntExact(hash.size());
        try (BytesStreamOutput out = new BytesStreamOutput()) {
            hash.getBytesRefs().writeTo(out);
            try (StreamInput in = out.bytes().streamInput()) {
                return new BytesRefBlock[] {
                    blockFactory.newBytesRefArrayVector(new BytesRefArray(in, BigArrays.NON_RECYCLING_INSTANCE), size).asBlock() };
            }
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
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
        b.append("BytesRefBlockHash{channel=").append(channel);
        b.append(", entries=").append(hash.size());
        b.append(", size=").append(ByteSizeValue.ofBytes(hash.ramBytesUsed()));
        b.append(", seenNull=").append(seenNull);
        return b.append('}').toString();
    }
}
