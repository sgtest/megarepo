/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.cluster;

import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;

import java.io.IOException;

/**
 * Abstract diffable object with simple diffs implementation that sends the entire object if object has changed or
 * nothing if object remained the same.
 */
public abstract class AbstractDiffable<T extends Diffable<T>> implements Diffable<T> {

    private static final Diff<?> EMPTY = new CompleteDiff<>();

    @SuppressWarnings("unchecked")
    @Override
    public Diff<T> diff(T previousState) {
        if (this.equals(previousState)) {
            return (Diff<T>) EMPTY;
        } else {
            return new CompleteDiff<>((T) this);
        }
    }

    @SuppressWarnings("unchecked")
    public static <T extends Diffable<T>> Diff<T> readDiffFrom(Reader<T> reader, StreamInput in) throws IOException {
        if (in.readBoolean()) {
            return new CompleteDiff<>(reader.read(in));
        }
        return (Diff<T>) EMPTY;
    }

    private static class CompleteDiff<T extends Diffable<T>> implements Diff<T> {

        @Nullable
        private final T part;

        /**
         * Creates simple diff with changes
         */
        CompleteDiff(T part) {
            this.part = part;
        }

        /**
         * Creates simple diff without changes
         */
        CompleteDiff() {
            this.part = null;
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            if (part != null) {
                out.writeBoolean(true);
                part.writeTo(out);
            } else {
                out.writeBoolean(false);
            }
        }

        @Override
        public T apply(T part) {
            if (this.part != null) {
                return this.part;
            } else {
                return part;
            }
        }
    }
}

