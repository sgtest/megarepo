/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.compute.data;

import org.apache.lucene.util.RamUsageEstimator;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.core.Releasables;

import java.io.IOException;

/**
 * Wrapper around {@link DocVector} to make a valid {@link Block}.
 */
public class DocBlock extends AbstractVectorBlock implements Block {

    private static final long BASE_RAM_BYTES_USED = RamUsageEstimator.shallowSizeOfInstance(DocBlock.class);

    private final DocVector vector;

    DocBlock(DocVector vector) {
        super(vector.getPositionCount(), vector.blockFactory());
        this.vector = vector;
    }

    @Override
    public String getWriteableName() {
        throw new UnsupportedOperationException();
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        throw new UnsupportedOperationException();
    }

    @Override
    public DocVector asVector() {
        return vector;
    }

    @Override
    public ElementType elementType() {
        return ElementType.DOC;
    }

    @Override
    public Block filter(int... positions) {
        return new DocBlock(asVector().filter(positions));
    }

    @Override
    public int hashCode() {
        return vector.hashCode();
    }

    @Override
    public boolean equals(Object obj) {
        if (obj instanceof DocBlock == false) {
            return false;
        }
        return this == obj || vector.equals(((DocBlock) obj).vector);
    }

    @Override
    public long ramBytesUsed() {
        return BASE_RAM_BYTES_USED + RamUsageEstimator.sizeOf(vector);
    }

    @Override
    public void close() {
        if (released) {
            throw new IllegalStateException("can't release already released block [" + this + "]");
        }
        released = true;
        Releasables.closeExpectNoException(vector);
    }

    /**
     * A builder the for {@link DocBlock}.
     */
    public static Builder newBlockBuilder(int estimatedSize, BlockFactory blockFactory) {
        return new Builder(estimatedSize, blockFactory);
    }

    public static class Builder implements Block.Builder {
        private final IntVector.Builder shards;
        private final IntVector.Builder segments;
        private final IntVector.Builder docs;

        private Builder(int estimatedSize, BlockFactory blockFactory) {
            shards = IntVector.newVectorBuilder(estimatedSize, blockFactory);
            segments = IntVector.newVectorBuilder(estimatedSize, blockFactory);
            docs = IntVector.newVectorBuilder(estimatedSize, blockFactory);
        }

        public Builder appendShard(int shard) {
            shards.appendInt(shard);
            return this;
        }

        public Builder appendSegment(int segment) {
            segments.appendInt(segment);
            return this;
        }

        public Builder appendDoc(int doc) {
            docs.appendInt(doc);
            return this;
        }

        @Override
        public Builder appendNull() {
            throw new UnsupportedOperationException("doc blocks can't contain null");
        }

        @Override
        public Builder beginPositionEntry() {
            throw new UnsupportedOperationException("doc blocks only contain one value per position");
        }

        @Override
        public Builder endPositionEntry() {
            throw new UnsupportedOperationException("doc blocks only contain one value per position");
        }

        @Override
        public Builder copyFrom(Block block, int beginInclusive, int endExclusive) {
            DocVector docVector = ((DocBlock) block).asVector();
            for (int i = beginInclusive; i < endExclusive; i++) {
                shards.appendInt(docVector.shards().getInt(i));
                segments.appendInt(docVector.segments().getInt(i));
                docs.appendInt(docVector.docs().getInt(i));
            }
            return this;
        }

        @Override
        public Block.Builder appendAllValuesToCurrentPosition(Block block) {
            throw new UnsupportedOperationException("DocBlock doesn't support appendBlockAndMerge");
        }

        @Override
        public Block.Builder mvOrdering(MvOrdering mvOrdering) {
            throw new UnsupportedOperationException("doc blocks only contain one value per position");
        }

        @Override
        public DocBlock build() {
            // Pass null for singleSegmentNonDecreasing so we calculate it when we first need it.
            return new DocVector(shards.build(), segments.build(), docs.build(), null).asBlock();
        }

        @Override
        public void close() {
            Releasables.closeExpectNoException(shards, segments, docs);
        }
    }
}
