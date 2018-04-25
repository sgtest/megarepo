/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.security.action.saml;

import org.elasticsearch.action.Action;
import org.elasticsearch.client.ElasticsearchClient;

/**
 * Action to perform IdP-initiated logout for a SAML-SSO user
 */
public final class SamlInvalidateSessionAction
        extends Action<SamlInvalidateSessionRequest, SamlInvalidateSessionResponse, SamlInvalidateSessionRequestBuilder> {

    public static final String NAME = "cluster:admin/xpack/security/saml/invalidate";
    public static final SamlInvalidateSessionAction INSTANCE = new SamlInvalidateSessionAction();

    private SamlInvalidateSessionAction() {
        super(NAME);
    }

    @Override
    public SamlInvalidateSessionRequestBuilder newRequestBuilder(ElasticsearchClient client) {
        return new SamlInvalidateSessionRequestBuilder(client);
    }

    @Override
    public SamlInvalidateSessionResponse newResponse() {
        return new SamlInvalidateSessionResponse();
    }
}
