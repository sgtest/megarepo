/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.compute.data;

import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;

import java.io.IOException;

/**
 * Block that stores int values.
 * This class is generated. Do not edit it.
 */
public sealed interface IntBlock extends Block permits IntArrayBlock, IntVectorBlock {

    /**
     * Retrieves the int value stored at the given value index.
     *
     * <p> Values for a given position are between getFirstValueIndex(position) (inclusive) and
     * getFirstValueIndex(position) + getValueCount(position) (exclusive).
     *
     * @param valueIndex the value index
     * @return the data value (as a int)
     */
    int getInt(int valueIndex);

    @Override
    IntVector asVector();

    @Override
    IntBlock filter(int... positions);

    @Override
    default String getWriteableName() {
        return "IntBlock";
    }

    NamedWriteableRegistry.Entry ENTRY = new NamedWriteableRegistry.Entry(Block.class, "IntBlock", IntBlock::readFrom);

    private static IntBlock readFrom(StreamInput in) throws IOException {
        return readFrom((BlockStreamInput) in);
    }

    private static IntBlock readFrom(BlockStreamInput in) throws IOException {
        final boolean isVector = in.readBoolean();
        if (isVector) {
            return IntVector.readFrom(in.blockFactory(), in).asBlock();
        }
        final int positions = in.readVInt();
        try (IntBlock.Builder builder = in.blockFactory().newIntBlockBuilder(positions)) {
            for (int i = 0; i < positions; i++) {
                if (in.readBoolean()) {
                    builder.appendNull();
                } else {
                    final int valueCount = in.readVInt();
                    builder.beginPositionEntry();
                    for (int valueIndex = 0; valueIndex < valueCount; valueIndex++) {
                        builder.appendInt(in.readInt());
                    }
                    builder.endPositionEntry();
                }
            }
            return builder.build();
        }
    }

    @Override
    default void writeTo(StreamOutput out) throws IOException {
        IntVector vector = asVector();
        out.writeBoolean(vector != null);
        if (vector != null) {
            vector.writeTo(out);
        } else {
            final int positions = getPositionCount();
            out.writeVInt(positions);
            for (int pos = 0; pos < positions; pos++) {
                if (isNull(pos)) {
                    out.writeBoolean(true);
                } else {
                    out.writeBoolean(false);
                    final int valueCount = getValueCount(pos);
                    out.writeVInt(valueCount);
                    for (int valueIndex = 0; valueIndex < valueCount; valueIndex++) {
                        out.writeInt(getInt(getFirstValueIndex(pos) + valueIndex));
                    }
                }
            }
        }
    }

    /**
     * Compares the given object with this block for equality. Returns {@code true} if and only if the
     * given object is a IntBlock, and both blocks are {@link #equals(IntBlock, IntBlock) equal}.
     */
    @Override
    boolean equals(Object obj);

    /** Returns the hash code of this block, as defined by {@link #hash(IntBlock)}. */
    @Override
    int hashCode();

    /**
     * Returns {@code true} if the given blocks are equal to each other, otherwise {@code false}.
     * Two blocks are considered equal if they have the same position count, and contain the same
     * values (including absent null values) in the same order. This definition ensures that the
     * equals method works properly across different implementations of the IntBlock interface.
     */
    static boolean equals(IntBlock block1, IntBlock block2) {
        if (block1 == block2) {
            return true;
        }
        final int positions = block1.getPositionCount();
        if (positions != block2.getPositionCount()) {
            return false;
        }
        for (int pos = 0; pos < positions; pos++) {
            if (block1.isNull(pos) || block2.isNull(pos)) {
                if (block1.isNull(pos) != block2.isNull(pos)) {
                    return false;
                }
            } else {
                final int valueCount = block1.getValueCount(pos);
                if (valueCount != block2.getValueCount(pos)) {
                    return false;
                }
                final int b1ValueIdx = block1.getFirstValueIndex(pos);
                final int b2ValueIdx = block2.getFirstValueIndex(pos);
                for (int valueIndex = 0; valueIndex < valueCount; valueIndex++) {
                    if (block1.getInt(b1ValueIdx + valueIndex) != block2.getInt(b2ValueIdx + valueIndex)) {
                        return false;
                    }
                }
            }
        }
        return true;
    }

    /**
     * Generates the hash code for the given block. The hash code is computed from the block's values.
     * This ensures that {@code block1.equals(block2)} implies that {@code block1.hashCode()==block2.hashCode()}
     * for any two blocks, {@code block1} and {@code block2}, as required by the general contract of
     * {@link Object#hashCode}.
     */
    static int hash(IntBlock block) {
        final int positions = block.getPositionCount();
        int result = 1;
        for (int pos = 0; pos < positions; pos++) {
            if (block.isNull(pos)) {
                result = 31 * result - 1;
            } else {
                final int valueCount = block.getValueCount(pos);
                result = 31 * result + valueCount;
                final int firstValueIdx = block.getFirstValueIndex(pos);
                for (int valueIndex = 0; valueIndex < valueCount; valueIndex++) {
                    result = 31 * result + block.getInt(firstValueIdx + valueIndex);
                }
            }
        }
        return result;
    }

    /** Returns a builder using the {@link BlockFactory#getNonBreakingInstance block factory}. */
    // Eventually, we want to remove this entirely, always passing an explicit BlockFactory
    static Builder newBlockBuilder(int estimatedSize) {
        return newBlockBuilder(estimatedSize, BlockFactory.getNonBreakingInstance());
    }

    static Builder newBlockBuilder(int estimatedSize, BlockFactory blockFactory) {
        return blockFactory.newIntBlockBuilder(estimatedSize);
    }

    /** Returns a block using the {@link BlockFactory#getNonBreakingInstance block factory}. */
    // Eventually, we want to remove this entirely, always passing an explicit BlockFactory
    static IntBlock newConstantBlockWith(int value, int positions) {
        return newConstantBlockWith(value, positions, BlockFactory.getNonBreakingInstance());
    }

    static IntBlock newConstantBlockWith(int value, int positions, BlockFactory blockFactory) {
        return blockFactory.newConstantIntBlockWith(value, positions);
    }

    sealed interface Builder extends Block.Builder permits IntBlockBuilder {

        /**
         * Appends a int to the current entry.
         */
        Builder appendInt(int value);

        /**
         * Copy the values in {@code block} from {@code beginInclusive} to
         * {@code endExclusive} into this builder.
         */
        Builder copyFrom(IntBlock block, int beginInclusive, int endExclusive);

        @Override
        Builder appendNull();

        @Override
        Builder beginPositionEntry();

        @Override
        Builder endPositionEntry();

        @Override
        Builder copyFrom(Block block, int beginInclusive, int endExclusive);

        @Override
        Builder mvOrdering(Block.MvOrdering mvOrdering);

        // TODO boolean containsMvDups();

        /**
         * Appends the all values of the given block into a the current position
         * in this builder.
         */
        Builder appendAllValuesToCurrentPosition(Block block);

        /**
         * Appends the all values of the given block into a the current position
         * in this builder.
         */
        Builder appendAllValuesToCurrentPosition(IntBlock block);

        @Override
        IntBlock build();
    }
}
