/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.security.action.token;

import org.elasticsearch.action.Action;
import org.elasticsearch.client.ElasticsearchClient;

/**
 * Action for invalidating a given token
 */
public final class InvalidateTokenAction extends Action<InvalidateTokenRequest, InvalidateTokenResponse, InvalidateTokenRequestBuilder> {

    public static final String NAME = "cluster:admin/xpack/security/token/invalidate";
    public static final InvalidateTokenAction INSTANCE = new InvalidateTokenAction();

    private InvalidateTokenAction() {
        super(NAME);
    }

    @Override
    public InvalidateTokenRequestBuilder newRequestBuilder(ElasticsearchClient client) {
        return new InvalidateTokenRequestBuilder(client);
    }

    @Override
    public InvalidateTokenResponse newResponse() {
        return new InvalidateTokenResponse();
    }
}
