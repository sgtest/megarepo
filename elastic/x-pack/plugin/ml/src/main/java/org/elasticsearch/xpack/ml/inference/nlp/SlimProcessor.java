/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.inference.nlp;

import org.elasticsearch.xpack.core.ml.inference.results.InferenceResults;
import org.elasticsearch.xpack.core.ml.inference.results.SlimResults;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.NlpConfig;
import org.elasticsearch.xpack.ml.inference.nlp.tokenizers.NlpTokenizer;
import org.elasticsearch.xpack.ml.inference.nlp.tokenizers.TokenizationResult;
import org.elasticsearch.xpack.ml.inference.pytorch.results.PyTorchInferenceResult;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import static org.elasticsearch.xpack.core.ml.inference.trainedmodel.InferenceConfig.DEFAULT_RESULTS_FIELD;

public class SlimProcessor extends NlpTask.Processor {

    private final NlpTask.RequestBuilder requestBuilder;

    public SlimProcessor(NlpTokenizer tokenizer) {
        super(tokenizer);
        this.requestBuilder = tokenizer.requestBuilder();
    }

    @Override
    public void validateInputs(List<String> inputs) {}

    @Override
    public NlpTask.RequestBuilder getRequestBuilder(NlpConfig config) {
        return requestBuilder;
    }

    @Override
    public NlpTask.ResultProcessor getResultProcessor(NlpConfig config) {
        return (tokenization, pyTorchResult) -> processResult(tokenization, pyTorchResult, config.getResultsField());
    }

    static InferenceResults processResult(TokenizationResult tokenization, PyTorchInferenceResult pyTorchResult, String resultsField) {
        List<SlimResults.WeightedToken> weightedTokens;
        if (pyTorchResult.getInferenceResult()[0].length == 1) {
            weightedTokens = sparseVectorToTokenWeights(pyTorchResult.getInferenceResult()[0][0]);
        } else {
            weightedTokens = multipleSparseVectorsToTokenWeights(pyTorchResult.getInferenceResult()[0]);
        }

        return new SlimResults(
            Optional.ofNullable(resultsField).orElse(DEFAULT_RESULTS_FIELD),
            weightedTokens,
            tokenization.anyTruncated()
        );
    }

    static List<SlimResults.WeightedToken> multipleSparseVectorsToTokenWeights(double[][] vector) {
        // reduce to a single 1d array choosing the max value
        // in each column and placing that in the first row
        for (int i = 1; i < vector.length; i++) {
            for (int tokenId = 0; tokenId < vector[i].length; tokenId++) {
                if (vector[i][tokenId] > vector[0][tokenId]) {
                    vector[0][tokenId] = vector[i][tokenId];
                }
            }
        }
        return sparseVectorToTokenWeights(vector[0]);
    }

    static List<SlimResults.WeightedToken> sparseVectorToTokenWeights(double[] vector) {
        // Anything with a score > 0.0 is retained.
        List<SlimResults.WeightedToken> weightedTokens = new ArrayList<>();
        for (int i = 0; i < vector.length; i++) {
            if (vector[i] > 0.0) {
                weightedTokens.add(new SlimResults.WeightedToken(i, (float) vector[i]));
            }
        }
        return weightedTokens;
    }
}
