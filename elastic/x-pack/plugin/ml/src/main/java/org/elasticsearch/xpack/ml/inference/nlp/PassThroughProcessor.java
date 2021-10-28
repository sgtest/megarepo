/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.inference.nlp;

import org.elasticsearch.xpack.core.ml.inference.results.InferenceResults;
import org.elasticsearch.xpack.core.ml.inference.results.PyTorchPassThroughResults;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.NlpConfig;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.PassThroughConfig;
import org.elasticsearch.xpack.ml.inference.deployment.PyTorchResult;
import org.elasticsearch.xpack.ml.inference.nlp.tokenizers.NlpTokenizer;
import org.elasticsearch.xpack.ml.inference.nlp.tokenizers.TokenizationResult;

import java.util.List;
import java.util.Optional;

import static org.elasticsearch.xpack.core.ml.inference.trainedmodel.InferenceConfig.DEFAULT_RESULTS_FIELD;

/**
 * A NLP processor that directly returns the PyTorch result
 * without any post-processing
 */
public class PassThroughProcessor implements NlpTask.Processor {

    private final NlpTask.RequestBuilder requestBuilder;
    private final String resultsField;

    PassThroughProcessor(NlpTokenizer tokenizer, PassThroughConfig config) {
        this.requestBuilder = tokenizer.requestBuilder();
        this.resultsField = config.getResultsField();
    }

    @Override
    public void validateInputs(List<String> inputs) {
        // nothing to validate
    }

    @Override
    public NlpTask.RequestBuilder getRequestBuilder(NlpConfig config) {
        return requestBuilder;
    }

    @Override
    public NlpTask.ResultProcessor getResultProcessor(NlpConfig config) {
        return (tokenization, pyTorchResult) -> processResult(tokenization, pyTorchResult, config.getResultsField());
    }

    private static InferenceResults processResult(TokenizationResult tokenization, PyTorchResult pyTorchResult, String resultsField) {
        // TODO - process all results in the batch
        return new PyTorchPassThroughResults(
            Optional.ofNullable(resultsField).orElse(DEFAULT_RESULTS_FIELD),
            pyTorchResult.getInferenceResult()[0],
            tokenization.anyTruncated()
        );
    }
}
