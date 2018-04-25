/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.security.action.user;

import org.elasticsearch.action.Action;
import org.elasticsearch.client.ElasticsearchClient;

/**
 * Action for deleting a native user.
 */
public class DeleteUserAction extends Action<DeleteUserRequest, DeleteUserResponse, DeleteUserRequestBuilder> {

    public static final DeleteUserAction INSTANCE = new DeleteUserAction();
    public static final String NAME = "cluster:admin/xpack/security/user/delete";

    protected DeleteUserAction() {
        super(NAME);
    }

    @Override
    public DeleteUserRequestBuilder newRequestBuilder(ElasticsearchClient client) {
        return new DeleteUserRequestBuilder(client, this);
    }

    @Override
    public DeleteUserResponse newResponse() {
        return new DeleteUserResponse();
    }
}
