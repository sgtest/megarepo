/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.inference.nlp;

import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.ml.inference.nlp.tokenizers.BertTokenizer;

import java.io.IOException;
import java.util.Arrays;

public class BertRequestBuilder implements NlpTask.RequestBuilder {

    static final String REQUEST_ID = "request_id";
    static final String TOKENS = "tokens";
    static final String ARG1 = "arg_1";
    static final String ARG2 = "arg_2";
    static final String ARG3 = "arg_3";

    private final BertTokenizer tokenizer;
    private BertTokenizer.TokenizationResult tokenization;
    private final int maxSequenceLength;

    public BertRequestBuilder(BertTokenizer tokenizer, NlpTaskConfig config) {
        this.tokenizer = tokenizer;
        this.maxSequenceLength = config.getMaxSequenceLength();
    }

    public BertTokenizer.TokenizationResult getTokenization() {
        return tokenization;
    }

    @Override
    public BytesReference buildRequest(String input, String requestId) throws IOException {
        tokenization = tokenizer.tokenize(input);
        if (tokenization.getTokenIds().length > maxSequenceLength) {
            throw ExceptionsHelper.badRequestException(
                "Input too large. The tokenized input length [{}] exceeds the maximum sequence length [{}]",
                tokenization.getTokenIds().length, maxSequenceLength);
        }
        return jsonRequest(tokenization.getTokenIds(), requestId);
    }

    static BytesReference jsonRequest(int[] tokens, String requestId) throws IOException {
        XContentBuilder builder = XContentFactory.jsonBuilder();
        builder.startObject();
        builder.field(REQUEST_ID, requestId);
        builder.array(TOKENS, tokens);

        int[] inputMask = new int[tokens.length];
        Arrays.fill(inputMask, 1);
        int[] segmentMask = new int[tokens.length];
        Arrays.fill(segmentMask, 0);
        int[] positionalIds = new int[tokens.length];
        Arrays.setAll(positionalIds, i -> i);

        builder.array(ARG1, inputMask);
        builder.array(ARG2, segmentMask);
        builder.array(ARG3, positionalIds);
        builder.endObject();

        // BytesReference.bytes closes the builder
        return BytesReference.bytes(builder);
    }
}
