/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.health;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.ActionType;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.client.internal.node.NodeClient;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.collect.Iterators;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.ChunkedToXContent;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.tasks.CancellableTask;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xcontent.ToXContent;

import java.io.IOException;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.NoSuchElementException;
import java.util.Objects;

public class GetHealthAction extends ActionType<GetHealthAction.Response> {

    public static final GetHealthAction INSTANCE = new GetHealthAction();
    public static final String NAME = "cluster:monitor/health_api";

    private GetHealthAction() {
        super(NAME, GetHealthAction.Response::new);
    }

    public static class Response extends ActionResponse implements ChunkedToXContent {

        private final ClusterName clusterName;
        @Nullable
        private final HealthStatus status;
        private final List<HealthIndicatorResult> indicators;

        public Response(StreamInput in) {
            throw new AssertionError("GetHealthAction should not be sent over the wire.");
        }

        public Response(final ClusterName clusterName, final List<HealthIndicatorResult> indicators, boolean showTopLevelStatus) {
            this.indicators = indicators;
            this.clusterName = clusterName;
            if (showTopLevelStatus) {
                this.status = HealthStatus.merge(indicators.stream().map(HealthIndicatorResult::status));
            } else {
                this.status = null;
            }
        }

        public ClusterName getClusterName() {
            return clusterName;
        }

        public HealthStatus getStatus() {
            return status;
        }

        public HealthIndicatorResult findIndicator(String name) {
            return indicators.stream()
                .filter(c -> Objects.equals(c.name(), name))
                .findFirst()
                .orElseThrow(() -> new NoSuchElementException("Indicator [" + name + "] is not found"));
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            throw new AssertionError("GetHealthAction should not be sent over the wire.");
        }

        @Override
        @SuppressWarnings("unchecked")
        public Iterator<? extends ToXContent> toXContentChunked() {
            return Iterators.concat(Iterators.single((ToXContent) (builder, params) -> {
                builder.startObject();
                if (status != null) {
                    builder.field("status", status.xContentValue());
                }
                builder.field("cluster_name", clusterName.value());
                builder.startObject("indicators");
                return builder;
            }),
                Iterators.concat(
                    indicators.stream()
                        .map(
                            indicator -> Iterators.concat(
                                // having the indicator name printed here prevents us from flat mapping all
                                // indicators however the affected resources which are the O(indices) fields are
                                // flat mapped over all diagnoses within the indicator
                                Iterators.single((ToXContent) (builder, params) -> builder.field(indicator.name())),
                                indicator.toXContentChunked()
                            )
                        )
                        .toArray(Iterator[]::new)
                ),
                Iterators.single((b, p) -> b.endObject().endObject())
            );
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) {
                return true;
            }
            if (o == null || getClass() != o.getClass()) {
                return false;
            }
            Response response = (Response) o;
            return clusterName.equals(response.clusterName) && status == response.status && indicators.equals(response.indicators);
        }

        @Override
        public int hashCode() {
            return Objects.hash(clusterName, status, indicators);
        }

        @Override
        public String toString() {
            return "Response{clusterName=" + clusterName + ", status=" + status + ", indicatorResults=" + indicators + '}';
        }
    }

    public static class Request extends ActionRequest {
        private final String indicatorName;
        private final boolean verbose;

        public Request(boolean verbose) {
            // We never compute details if no indicator name is given because of the runtime cost:
            this.indicatorName = null;
            this.verbose = verbose;
        }

        public Request(String indicatorName, boolean verbose) {
            this.indicatorName = indicatorName;
            this.verbose = verbose;
        }

        @Override
        public ActionRequestValidationException validate() {
            return null;
        }

        @Override
        public Task createTask(long id, String type, String action, TaskId parentTaskId, Map<String, String> headers) {
            return new CancellableTask(id, type, action, "", parentTaskId, headers);
        }
    }

    public static class TransportAction extends org.elasticsearch.action.support.TransportAction<Request, Response> {

        private final ClusterService clusterService;
        private final HealthService healthService;
        private final NodeClient client;

        @Inject
        public TransportAction(
            ActionFilters actionFilters,
            TransportService transportService,
            ClusterService clusterService,
            HealthService healthService,
            NodeClient client
        ) {
            super(NAME, actionFilters, transportService.getTaskManager());
            this.clusterService = clusterService;
            this.healthService = healthService;
            this.client = client;
        }

        @Override
        protected void doExecute(Task task, Request request, ActionListener<Response> responseListener) {
            assert task instanceof CancellableTask;
            healthService.getHealth(
                client,
                request.indicatorName,
                request.verbose,
                responseListener.map(
                    healthIndicatorResults -> new Response(
                        clusterService.getClusterName(),
                        healthIndicatorResults,
                        request.indicatorName == null
                    )
                )
            );
        }
    }
}
