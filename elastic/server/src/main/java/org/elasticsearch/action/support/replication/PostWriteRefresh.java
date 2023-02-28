/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.support.replication;

import org.apache.lucene.store.AlreadyClosedException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionListenerResponseHandler;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.admin.indices.refresh.TransportUnpromotableShardRefreshAction;
import org.elasticsearch.action.admin.indices.refresh.UnpromotableShardRefreshRequest;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.index.engine.Engine;
import org.elasticsearch.index.shard.IndexShard;
import org.elasticsearch.index.translog.Translog;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;

public class PostWriteRefresh {

    public static final String FORCED_REFRESH_AFTER_INDEX = "refresh_flag_index";
    private final TransportService transportService;

    public PostWriteRefresh(final TransportService transportService) {
        this.transportService = transportService;
    }

    public void refreshShard(
        WriteRequest.RefreshPolicy policy,
        IndexShard indexShard,
        @Nullable Translog.Location location,
        ActionListener<Boolean> listener
    ) {
        switch (policy) {
            case NONE -> listener.onResponse(false);
            case WAIT_UNTIL -> waitUntil(indexShard, location, new ActionListener<>() {
                @Override
                public void onResponse(Boolean forced) {
                    if (indexShard.getReplicationGroup().getRoutingTable().unpromotableShards().size() > 0) {
                        refreshUnpromotables(indexShard, location, listener, forced);
                    } else {
                        listener.onResponse(forced);
                    }
                }

                @Override
                public void onFailure(Exception e) {
                    listener.onFailure(e);
                }
            });
            case IMMEDIATE -> immediate(indexShard, new ActionListener<>() {
                @Override
                public void onResponse(Engine.RefreshResult refreshResult) {
                    if (indexShard.getReplicationGroup().getRoutingTable().unpromotableShards().size() > 0) {
                        sendUnpromotableRequests(indexShard, refreshResult.generation(), true, listener);
                    } else {
                        listener.onResponse(true);
                    }
                }

                @Override
                public void onFailure(Exception e) {
                    listener.onFailure(e);
                }
            });
            default -> throw new IllegalArgumentException("unknown refresh policy: " + policy);
        }
    }

    public static void refreshReplicaShard(
        WriteRequest.RefreshPolicy policy,
        IndexShard indexShard,
        @Nullable Translog.Location location,
        ActionListener<Boolean> listener
    ) {
        switch (policy) {
            case NONE -> listener.onResponse(false);
            case WAIT_UNTIL -> waitUntil(indexShard, location, listener);
            case IMMEDIATE -> immediate(indexShard, listener.map(r -> true));
            default -> throw new IllegalArgumentException("unknown refresh policy: " + policy);
        }
    }

    private static void immediate(IndexShard indexShard, ActionListener<Engine.RefreshResult> listener) {
        Engine.RefreshResult refreshResult = indexShard.refresh(FORCED_REFRESH_AFTER_INDEX);
        listener.onResponse(refreshResult);
    }

    private static void waitUntil(IndexShard indexShard, Translog.Location location, ActionListener<Boolean> listener) {
        if (location != null) {
            indexShard.addRefreshListener(location, listener::onResponse);
        } else {
            listener.onResponse(false);
        }
    }

    private void refreshUnpromotables(IndexShard indexShard, Translog.Location location, ActionListener<Boolean> listener, boolean forced) {
        Engine engineOrNull = indexShard.getEngineOrNull();
        if (engineOrNull == null) {
            listener.onFailure(new AlreadyClosedException("Engine closed during refresh."));
            return;
        }

        engineOrNull.addFlushListener(location, new ActionListener<>() {
            @Override
            public void onResponse(Long generation) {
                sendUnpromotableRequests(indexShard, generation, forced, listener);
            }

            @Override
            public void onFailure(Exception e) {
                listener.onFailure(e);
            }
        });
    }

    private void sendUnpromotableRequests(IndexShard indexShard, long generation, boolean wasForced, ActionListener<Boolean> listener) {
        UnpromotableShardRefreshRequest unpromotableReplicaRequest = new UnpromotableShardRefreshRequest(
            indexShard.getReplicationGroup().getRoutingTable(),
            generation
        );
        transportService.sendRequest(
            transportService.getLocalNode(),
            TransportUnpromotableShardRefreshAction.NAME,
            unpromotableReplicaRequest,
            new ActionListenerResponseHandler<>(
                listener.delegateFailure((l, r) -> l.onResponse(wasForced)),
                (in) -> ActionResponse.Empty.INSTANCE,
                ThreadPool.Names.REFRESH
            )
        );
    }

}
