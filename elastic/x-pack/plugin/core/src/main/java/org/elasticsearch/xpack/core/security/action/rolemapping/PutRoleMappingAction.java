/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.security.action.rolemapping;

import org.elasticsearch.action.Action;

/**
 * Action for adding a role to the security index
 */
public class PutRoleMappingAction extends Action<PutRoleMappingResponse> {

    public static final PutRoleMappingAction INSTANCE = new PutRoleMappingAction();
    public static final String NAME = "cluster:admin/xpack/security/role_mapping/put";

    private PutRoleMappingAction() {
        super(NAME);
    }

    @Override
    public PutRoleMappingResponse newResponse() {
        return new PutRoleMappingResponse();
    }
}
