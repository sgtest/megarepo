/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.security.action.oidc;

import org.elasticsearch.action.ActionType;
import org.elasticsearch.common.io.stream.Writeable;

public class OpenIdConnectPrepareAuthenticationAction extends ActionType<OpenIdConnectPrepareAuthenticationResponse> {

    public static final OpenIdConnectPrepareAuthenticationAction INSTANCE = new OpenIdConnectPrepareAuthenticationAction();
    public static final String NAME = "cluster:admin/xpack/security/oidc/prepare";

    private OpenIdConnectPrepareAuthenticationAction() {
        super(NAME);
    }

    @Override
    public Writeable.Reader<OpenIdConnectPrepareAuthenticationResponse> getResponseReader() {
        return OpenIdConnectPrepareAuthenticationResponse::new;
    }
}
