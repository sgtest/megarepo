/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.inference.nlp;

import org.elasticsearch.common.ValidationException;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.NlpConfig;
import org.elasticsearch.xpack.ml.inference.deployment.PyTorchResult;
import org.elasticsearch.xpack.core.ml.inference.results.InferenceResults;
import org.elasticsearch.xpack.ml.inference.nlp.tokenizers.BertTokenizer;

import java.io.IOException;

public class NlpTask {

    private final NlpConfig config;
    private final BertTokenizer tokenizer;

    public NlpTask(NlpConfig config, Vocabulary vocabulary) {
        this.config = config;
        this.tokenizer = BertTokenizer.builder(vocabulary.get())
            .setWithSpecialTokens(config.getTokenizationParams().withSpecialTokens())
            .setDoLowerCase(config.getTokenizationParams().doLowerCase())
            .build();
    }

    /**
     * Create and validate the NLP Processor
     * @return
     * @throws ValidationException if the validation fails
     */
    public Processor createProcessor() throws ValidationException {
        return TaskType.fromString(config.getName()).createProcessor(tokenizer, config);
    }

    public interface RequestBuilder {
        BytesReference buildRequest(String inputs, String requestId) throws IOException;
    }

    public interface ResultProcessor {
        InferenceResults processResult(PyTorchResult pyTorchResult);
    }

    public interface Processor {
        /**
         * Validate the task input string.
         * Throws an exception if the inputs fail validation
         *
         * @param inputs Text to validate
         */
        void validateInputs(String inputs);

        RequestBuilder getRequestBuilder();
        ResultProcessor getResultProcessor();
    }
}
