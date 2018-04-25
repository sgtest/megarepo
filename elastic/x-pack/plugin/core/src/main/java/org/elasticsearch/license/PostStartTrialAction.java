/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.license;

import org.elasticsearch.action.Action;
import org.elasticsearch.client.ElasticsearchClient;

public class PostStartTrialAction extends Action<PostStartTrialRequest, PostStartTrialResponse, PostStartTrialRequestBuilder> {

    public static final PostStartTrialAction INSTANCE = new PostStartTrialAction();
    public static final String NAME = "cluster:admin/xpack/license/start_trial";

    private PostStartTrialAction() {
        super(NAME);
    }

    @Override
    public PostStartTrialRequestBuilder newRequestBuilder(ElasticsearchClient client) {
        return new PostStartTrialRequestBuilder(client, this);
    }

    @Override
    public PostStartTrialResponse newResponse() {
        return new PostStartTrialResponse();
    }
}
