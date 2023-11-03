/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.inference.external.huggingface;

import org.apache.http.client.methods.HttpRequestBase;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.inference.InferenceResults;
import org.elasticsearch.xpack.inference.external.http.HttpResult;
import org.elasticsearch.xpack.inference.external.http.sender.Sender;
import org.elasticsearch.xpack.inference.external.request.huggingface.HuggingFaceElserRequest;
import org.elasticsearch.xpack.inference.external.response.huggingface.HuggingFaceElserResponseEntity;
import org.elasticsearch.xpack.inference.logging.ThrottlerManager;

import java.io.IOException;

import static org.elasticsearch.core.Strings.format;

public class HuggingFaceClient {
    private static final Logger logger = LogManager.getLogger(HuggingFaceClient.class);

    private final ThrottlerManager throttlerManager;

    private final Sender sender;

    public HuggingFaceClient(Sender sender, ThrottlerManager throttlerManager) {
        this.sender = sender;
        this.throttlerManager = throttlerManager;
    }

    public void send(HuggingFaceElserRequest request, ActionListener<InferenceResults> listener) throws IOException {
        HttpRequestBase httpRequest = request.createRequest();
        ActionListener<HttpResult> responseListener = ActionListener.wrap(response -> {
            try {
                listener.onResponse(HuggingFaceElserResponseEntity.fromResponse(response));
            } catch (Exception e) {
                String msg = format("Failed to parse the Hugging Face ELSER response for request [%s]", httpRequest.getRequestLine());
                throttlerManager.getThrottler().warn(logger, msg, e);
                listener.onFailure(new ElasticsearchException(msg, e));
            }
        }, listener::onFailure);

        sender.send(httpRequest, responseListener);
    }
}
