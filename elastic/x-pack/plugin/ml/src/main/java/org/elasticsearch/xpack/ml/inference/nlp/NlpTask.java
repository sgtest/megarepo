/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.inference.nlp;

import org.elasticsearch.common.ValidationException;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.xcontent.support.XContentMapValues;
import org.elasticsearch.xpack.core.ml.inference.TrainedModelInput;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.NlpConfig;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.ml.inference.deployment.PyTorchResult;
import org.elasticsearch.xpack.core.ml.inference.results.InferenceResults;
import org.elasticsearch.xpack.ml.inference.nlp.tokenizers.NlpTokenizer;
import org.elasticsearch.xpack.ml.inference.nlp.tokenizers.TokenizationResult;

import java.io.IOException;
import java.util.Map;
import java.util.Objects;

public class NlpTask {

    private final NlpConfig config;
    private final NlpTokenizer tokenizer;

    public NlpTask(NlpConfig config, Vocabulary vocabulary) {
        this.config = config;
        this.tokenizer = NlpTokenizer.build(vocabulary, config.getTokenization());
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
        Request buildRequest(String inputs, String requestId) throws IOException;
    }

    public interface ResultProcessor {
        InferenceResults processResult(TokenizationResult tokenization, PyTorchResult pyTorchResult);
    }

    public interface ResultProcessorFactory {
        ResultProcessor build(TokenizationResult tokenizationResult);
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

    public static String extractInput(TrainedModelInput input, Map<String, Object> doc) {
        assert input.getFieldNames().size() == 1;
        String inputField = input.getFieldNames().get(0);
        Object inputValue = XContentMapValues.extractValue(inputField, doc);
        if (inputValue == null) {
            throw ExceptionsHelper.badRequestException("no value could be found for input field [{}]", inputField);
        }
        if (inputValue instanceof String) {
            return (String) inputValue;
        }
        throw ExceptionsHelper.badRequestException("input value [{}] for field [{}] is not a string", inputValue, inputField);
    }

    public static class Request {
        public final TokenizationResult tokenization;
        public final BytesReference processInput;

        public Request(TokenizationResult tokenization, BytesReference processInput) {
            this.tokenization = Objects.requireNonNull(tokenization);
            this.processInput = Objects.requireNonNull(processInput);
        }
    }
}
