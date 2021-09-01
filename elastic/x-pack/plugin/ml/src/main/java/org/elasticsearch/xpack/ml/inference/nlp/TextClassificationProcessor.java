/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.inference.nlp;

import org.elasticsearch.common.Strings;
import org.elasticsearch.common.ValidationException;
import org.elasticsearch.xpack.core.ml.inference.results.InferenceResults;
import org.elasticsearch.xpack.core.ml.inference.results.TextClassificationResults;
import org.elasticsearch.xpack.core.ml.inference.results.TopClassEntry;
import org.elasticsearch.xpack.core.ml.inference.results.WarningInferenceResults;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.TextClassificationConfig;
import org.elasticsearch.xpack.ml.inference.deployment.PyTorchResult;
import org.elasticsearch.xpack.ml.inference.nlp.tokenizers.NlpTokenizer;
import org.elasticsearch.xpack.ml.inference.nlp.tokenizers.TokenizationResult;

import java.util.Comparator;
import java.util.List;
import java.util.Locale;
import java.util.stream.Collectors;
import java.util.stream.IntStream;

public class TextClassificationProcessor implements NlpTask.Processor {

    private final NlpTask.RequestBuilder requestBuilder;
    private final String[] classLabels;
    private final int numTopClasses;

    TextClassificationProcessor(NlpTokenizer tokenizer, TextClassificationConfig config) {
        this.requestBuilder = tokenizer.requestBuilder();
        List<String> classLabels = config.getClassificationLabels();
        if (classLabels == null || classLabels.isEmpty()) {
            this.classLabels = new String[] {"negative", "positive"};
        } else {
            this.classLabels = classLabels.toArray(String[]::new);
        }
        // negative values are a special case of asking for ALL classes. Since we require the output size to equal the classLabel size
        // This is a nice way of setting the value
        this.numTopClasses = config.getNumTopClasses() < 0 ? this.classLabels.length : config.getNumTopClasses();
        validate();
    }

    private void validate() {
        if (classLabels.length < 2) {
            throw new ValidationException().addValidationError(
                String.format(
                    Locale.ROOT,
                    "Text classification requires at least 2 [%s]. Invalid labels [%s]",
                    TextClassificationConfig.CLASSIFICATION_LABELS,
                    Strings.arrayToCommaDelimitedString(classLabels)
                )
            );
        }
        if (numTopClasses == 0) {
            throw new ValidationException().addValidationError(
                String.format(
                    Locale.ROOT,
                    "Text classification requires at least 1 [%s]; provided [%d]",
                    TextClassificationConfig.NUM_TOP_CLASSES,
                    numTopClasses
                )
            );
        }
    }

    @Override
    public void validateInputs(List<String> inputs) {
        // nothing to validate
    }

    @Override
    public NlpTask.RequestBuilder getRequestBuilder() {
        return requestBuilder;
    }

    @Override
    public NlpTask.ResultProcessor getResultProcessor() {
        return this::processResult;
    }

    InferenceResults processResult(TokenizationResult tokenization, PyTorchResult pyTorchResult) {
        if (pyTorchResult.getInferenceResult().length < 1) {
            return new WarningInferenceResults("Text classification result has no data");
        }

        if (pyTorchResult.getInferenceResult()[0].length != classLabels.length) {
            return new WarningInferenceResults(
                "Expected exactly [{}] values in text classification result; got [{}]",
                classLabels.length,
                pyTorchResult.getInferenceResult()[0].length
            );
        }

        double[] normalizedScores = NlpHelpers.convertToProbabilitiesBySoftMax(pyTorchResult.getInferenceResult()[0][0]);
        return new TextClassificationResults(
            IntStream.range(0, normalizedScores.length)
                .mapToObj(i -> new TopClassEntry(classLabels[i], normalizedScores[i]))
                // Put the highest scoring class first
                .sorted(Comparator.comparing(TopClassEntry::getProbability).reversed())
                .limit(numTopClasses)
                .collect(Collectors.toList())
        );
    }
}
