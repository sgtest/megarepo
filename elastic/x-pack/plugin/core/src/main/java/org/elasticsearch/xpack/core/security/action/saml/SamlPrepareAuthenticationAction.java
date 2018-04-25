/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.security.action.saml;

import org.elasticsearch.action.Action;
import org.elasticsearch.client.ElasticsearchClient;

/**
 * Action for initiating an authentication process using SAML assertions
 */
public final class SamlPrepareAuthenticationAction
        extends Action<SamlPrepareAuthenticationRequest, SamlPrepareAuthenticationResponse, SamlPrepareAuthenticationRequestBuilder> {

    public static final String NAME = "cluster:admin/xpack/security/saml/prepare";
    public static final SamlPrepareAuthenticationAction INSTANCE = new SamlPrepareAuthenticationAction();

    private SamlPrepareAuthenticationAction() {
        super(NAME);
    }

    @Override
    public SamlPrepareAuthenticationRequestBuilder newRequestBuilder(ElasticsearchClient client) {
        return new SamlPrepareAuthenticationRequestBuilder(client);
    }

    @Override
    public SamlPrepareAuthenticationResponse newResponse() {
        return new SamlPrepareAuthenticationResponse();
    }
}
