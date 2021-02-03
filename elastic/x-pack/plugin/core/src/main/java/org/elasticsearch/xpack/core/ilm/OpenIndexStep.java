/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.core.ilm;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.indices.open.OpenIndexRequest;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateObserver;
import org.elasticsearch.cluster.metadata.IndexMetadata;

/**
 * Invokes a open step on a single index.
 */

final class OpenIndexStep extends AsyncActionStep {

    static final String NAME = "open-index";

    OpenIndexStep(StepKey key, StepKey nextStepKey, Client client) {
        super(key, nextStepKey, client);
    }

    @Override
    public void performAction(IndexMetadata indexMetadata, ClusterState currentClusterState,
                              ClusterStateObserver observer, Listener listener) {
        if (indexMetadata.getState() == IndexMetadata.State.CLOSE) {
            OpenIndexRequest request = new OpenIndexRequest(indexMetadata.getIndex().getName());
            getClient().admin().indices()
                .open(request,
                    ActionListener.wrap(openIndexResponse -> {
                        if (openIndexResponse.isAcknowledged() == false) {
                            throw new ElasticsearchException("open index request failed to be acknowledged");
                        }
                        listener.onResponse(true);
                    }, listener::onFailure));

        } else {
            listener.onResponse(true);
        }
    }

    @Override
    public boolean isRetryable() {
        return true;
    }
}
