/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.compute.data;

/**
 * Filter block for DoubleBlocks.
 * This class is generated. Do not edit it.
 */
final class FilterDoubleBlock extends AbstractFilterBlock implements DoubleBlock {

    private final DoubleBlock block;

    FilterDoubleBlock(DoubleBlock block, int... positions) {
        super(block, positions);
        this.block = block;
    }

    @Override
    public DoubleVector asVector() {
        return null;
    }

    @Override
    public double getDouble(int valueIndex) {
        return block.getDouble(valueIndex);
    }

    @Override
    public ElementType elementType() {
        return ElementType.DOUBLE;
    }

    @Override
    public DoubleBlock filter(int... positions) {
        return new FilterDoubleBlock(this, positions);
    }

    @Override
    public DoubleBlock expand() {
        if (false == block.mayHaveMultivaluedFields()) {
            return this;
        }
        /*
         * Build a copy of the target block, selecting only the positions
         * we've been assigned and expanding all multivalued fields
         * into single valued fields.
         */
        DoubleBlock.Builder builder = DoubleBlock.newBlockBuilder(positions.length);
        for (int p : positions) {
            if (block.isNull(p)) {
                builder.appendNull();
                continue;
            }
            int start = block.getFirstValueIndex(p);
            int end = start + block.getValueCount(p);
            for (int i = start; i < end; i++) {
                builder.appendDouble(block.getDouble(i));
            }
        }
        return builder.build();
    }

    @Override
    public boolean equals(Object obj) {
        if (obj instanceof DoubleBlock that) {
            return DoubleBlock.equals(this, that);
        }
        return false;
    }

    @Override
    public int hashCode() {
        return DoubleBlock.hash(this);
    }

    @Override
    public String toString() {
        StringBuilder sb = new StringBuilder();
        sb.append(this.getClass().getSimpleName());
        sb.append("[positions=" + getPositionCount() + ", values=[");
        appendValues(sb);
        sb.append("]]");
        return sb.toString();
    }

    private void appendValues(StringBuilder sb) {
        final int positions = getPositionCount();
        for (int p = 0; p < positions; p++) {
            if (p > 0) {
                sb.append(", ");
            }
            int start = getFirstValueIndex(p);
            int count = getValueCount(p);
            if (count == 1) {
                sb.append(getDouble(start));
                continue;
            }
            sb.append('[');
            int end = start + count;
            for (int i = start; i < end; i++) {
                if (i > start) {
                    sb.append(", ");
                }
                sb.append(getDouble(i));
            }
            sb.append(']');
        }
    }
}
