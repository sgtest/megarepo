/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ccr.action;

import org.elasticsearch.action.Action;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.admin.cluster.state.ClusterStateRequest;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.persistent.PersistentTasksCustomMetaData;
import org.elasticsearch.persistent.PersistentTasksService;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.transport.TransportService;

import java.io.IOException;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReferenceArray;

public class UnfollowIndexAction extends Action<AcknowledgedResponse> {

    public static final UnfollowIndexAction INSTANCE = new UnfollowIndexAction();
    public static final String NAME = "cluster:admin/xpack/ccr/unfollow_index";

    private UnfollowIndexAction() {
        super(NAME);
    }

    @Override
    public AcknowledgedResponse newResponse() {
        return new AcknowledgedResponse();
    }

    public static class Request extends ActionRequest {

        private String followIndex;

        public String getFollowIndex() {
            return followIndex;
        }

        public void setFollowIndex(String followIndex) {
            this.followIndex = followIndex;
        }

        @Override
        public ActionRequestValidationException validate() {
            return null;
        }

        @Override
        public void readFrom(StreamInput in) throws IOException {
            super.readFrom(in);
            followIndex = in.readString();
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeString(followIndex);
        }
    }

    public static class TransportAction extends HandledTransportAction<Request, AcknowledgedResponse> {

        private final Client client;
        private final PersistentTasksService persistentTasksService;

        @Inject
        public TransportAction(Settings settings,
                               TransportService transportService,
                               ActionFilters actionFilters,
                               Client client,
                               PersistentTasksService persistentTasksService) {
            super(settings, NAME, transportService, actionFilters, Request::new);
            this.client = client;
            this.persistentTasksService = persistentTasksService;
        }

        @Override
        protected void doExecute(Task task,
                                 Request request,
                                 ActionListener<AcknowledgedResponse> listener) {

            client.admin().cluster().state(new ClusterStateRequest(), ActionListener.wrap(r -> {
                IndexMetaData followIndexMetadata = r.getState().getMetaData().index(request.followIndex);
                if (followIndexMetadata == null) {
                    listener.onFailure(new IllegalArgumentException("follow index [" + request.followIndex + "] does not exist"));
                    return;
                }

                final int numShards = followIndexMetadata.getNumberOfShards();
                final AtomicInteger counter = new AtomicInteger(numShards);
                final AtomicReferenceArray<Object> responses = new AtomicReferenceArray<>(followIndexMetadata.getNumberOfShards());
                for (int i = 0; i < numShards; i++) {
                    final int shardId = i;
                    String taskId = followIndexMetadata.getIndexUUID() + "-" + shardId;
                    persistentTasksService.sendRemoveRequest(taskId,
                            new ActionListener<PersistentTasksCustomMetaData.PersistentTask<?>>() {
                        @Override
                        public void onResponse(PersistentTasksCustomMetaData.PersistentTask<?> task) {
                            responses.set(shardId, task);
                            finalizeResponse();
                        }

                        @Override
                        public void onFailure(Exception e) {
                            responses.set(shardId, e);
                            finalizeResponse();
                        }

                        void finalizeResponse() {
                            Exception error = null;
                            if (counter.decrementAndGet() == 0) {
                                for (int j = 0; j < responses.length(); j++) {
                                    Object response = responses.get(j);
                                    if (response instanceof Exception) {
                                        if (error == null) {
                                            error = (Exception) response;
                                        } else {
                                            error.addSuppressed((Throwable) response);
                                        }
                                    }
                                }

                                if (error == null) {
                                    // include task ids?
                                    listener.onResponse(new AcknowledgedResponse(true));
                                } else {
                                    // TODO: cancel all started tasks
                                    listener.onFailure(error);
                                }
                            }
                        }
                    });
                }
            }, listener::onFailure));
        }
    }

}
