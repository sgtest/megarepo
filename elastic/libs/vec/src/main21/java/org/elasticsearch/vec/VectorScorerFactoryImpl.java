/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.vec;

import org.apache.lucene.index.VectorSimilarityFunction;
import org.apache.lucene.store.FilterIndexInput;
import org.apache.lucene.store.IndexInput;
import org.apache.lucene.store.MemorySegmentAccessInput;
import org.apache.lucene.util.hnsw.RandomVectorScorer;
import org.apache.lucene.util.hnsw.RandomVectorScorerSupplier;
import org.apache.lucene.util.quantization.RandomAccessQuantizedByteVectorValues;
import org.elasticsearch.nativeaccess.NativeAccess;
import org.elasticsearch.vec.internal.Int7SQVectorScorer;
import org.elasticsearch.vec.internal.Int7SQVectorScorerSupplier.DotProductSupplier;
import org.elasticsearch.vec.internal.Int7SQVectorScorerSupplier.EuclideanSupplier;
import org.elasticsearch.vec.internal.Int7SQVectorScorerSupplier.MaxInnerProductSupplier;

import java.util.Optional;

final class VectorScorerFactoryImpl implements VectorScorerFactory {

    static final VectorScorerFactoryImpl INSTANCE;

    private VectorScorerFactoryImpl() {}

    static {
        INSTANCE = NativeAccess.instance().getVectorSimilarityFunctions().map(ignore -> new VectorScorerFactoryImpl()).orElse(null);
    }

    @Override
    public Optional<RandomVectorScorerSupplier> getInt7SQVectorScorerSupplier(
        VectorSimilarityType similarityType,
        IndexInput input,
        RandomAccessQuantizedByteVectorValues values,
        float scoreCorrectionConstant
    ) {
        input = FilterIndexInput.unwrapOnlyTest(input);
        if (input instanceof MemorySegmentAccessInput == false) {
            return Optional.empty();
        }
        MemorySegmentAccessInput msInput = (MemorySegmentAccessInput) input;
        checkInvariants(values.size(), values.dimension(), input);
        return switch (similarityType) {
            case COSINE, DOT_PRODUCT -> Optional.of(new DotProductSupplier(msInput, values, scoreCorrectionConstant));
            case EUCLIDEAN -> Optional.of(new EuclideanSupplier(msInput, values, scoreCorrectionConstant));
            case MAXIMUM_INNER_PRODUCT -> Optional.of(new MaxInnerProductSupplier(msInput, values, scoreCorrectionConstant));
        };
    }

    @Override
    public Optional<RandomVectorScorer> getInt7SQVectorScorer(
        VectorSimilarityFunction sim,
        RandomAccessQuantizedByteVectorValues values,
        float[] queryVector
    ) {
        return Int7SQVectorScorer.create(sim, values, queryVector);
    }

    static void checkInvariants(int maxOrd, int vectorByteLength, IndexInput input) {
        if (input.length() < (long) vectorByteLength * maxOrd) {
            throw new IllegalArgumentException("input length is less than expected vector data");
        }
    }
}
