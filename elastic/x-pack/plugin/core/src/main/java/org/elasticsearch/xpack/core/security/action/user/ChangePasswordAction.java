/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.security.action.user;

import org.elasticsearch.action.Action;
import org.elasticsearch.client.ElasticsearchClient;

public class ChangePasswordAction extends Action<ChangePasswordRequest, ChangePasswordResponse, ChangePasswordRequestBuilder> {

    public static final ChangePasswordAction INSTANCE = new ChangePasswordAction();
    public static final String NAME = "cluster:admin/xpack/security/user/change_password";

    protected ChangePasswordAction() {
        super(NAME);
    }

    @Override
    public ChangePasswordRequestBuilder newRequestBuilder(ElasticsearchClient client) {
        return new ChangePasswordRequestBuilder(client);
    }

    @Override
    public ChangePasswordResponse newResponse() {
        return new ChangePasswordResponse();
    }
}
