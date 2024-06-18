/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.rest.inference;

import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.client.internal.node.NodeClient;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.core.RestApiVersion;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.rest.Scope;
import org.elasticsearch.rest.ServerlessScope;
import org.elasticsearch.rest.action.RestToXContentListener;
import org.elasticsearch.xpack.core.ml.action.StartTrainedModelDeploymentAction;
import org.elasticsearch.xpack.core.ml.inference.assignment.AllocationStatus;
import org.elasticsearch.xpack.ml.MachineLearning;
import org.elasticsearch.xpack.ml.rest.RestCompatibilityChecker;

import java.io.IOException;
import java.util.Collections;
import java.util.List;
import java.util.Objects;

import static org.elasticsearch.rest.RestRequest.Method.POST;
import static org.elasticsearch.xpack.core.ml.action.StartTrainedModelDeploymentAction.Request.CACHE_SIZE;
import static org.elasticsearch.xpack.core.ml.action.StartTrainedModelDeploymentAction.Request.NUMBER_OF_ALLOCATIONS;
import static org.elasticsearch.xpack.core.ml.action.StartTrainedModelDeploymentAction.Request.QUEUE_CAPACITY;
import static org.elasticsearch.xpack.core.ml.action.StartTrainedModelDeploymentAction.Request.THREADS_PER_ALLOCATION;
import static org.elasticsearch.xpack.core.ml.action.StartTrainedModelDeploymentAction.Request.TIMEOUT;
import static org.elasticsearch.xpack.core.ml.action.StartTrainedModelDeploymentAction.Request.WAIT_FOR;

@ServerlessScope(Scope.PUBLIC)
public class RestStartTrainedModelDeploymentAction extends BaseRestHandler {

    public RestStartTrainedModelDeploymentAction(boolean disableInferenceProcessCache) {
        super();
        if (disableInferenceProcessCache) {
            this.defaultCacheSize = ByteSizeValue.ZERO;
        } else {
            // Don't set the default cache size yet
            defaultCacheSize = null;
        }
    }

    private final ByteSizeValue defaultCacheSize;

    @Override
    public String getName() {
        return "xpack_ml_start_trained_models_deployment_action";
    }

    @Override
    public List<Route> routes() {
        return Collections.singletonList(
            new Route(
                POST,
                MachineLearning.BASE_PATH
                    + "trained_models/{"
                    + StartTrainedModelDeploymentAction.Request.MODEL_ID.getPreferredName()
                    + "}/deployment/_start"
            )
        );
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) throws IOException {
        String modelId = restRequest.param(StartTrainedModelDeploymentAction.Request.MODEL_ID.getPreferredName());
        String deploymentId = restRequest.param(StartTrainedModelDeploymentAction.Request.DEPLOYMENT_ID.getPreferredName(), modelId);
        StartTrainedModelDeploymentAction.Request request;

        if (restRequest.hasContentOrSourceParam()) { // request has body
            request = StartTrainedModelDeploymentAction.Request.parseRequest(
                modelId,
                deploymentId,
                restRequest.contentOrSourceParamParser()
            );
        } else {
            request = new StartTrainedModelDeploymentAction.Request(modelId, deploymentId);
        }

        if (restRequest.hasParam(TIMEOUT.getPreferredName())) {
            TimeValue openTimeout = validateParameters(
                request.getTimeout(),
                restRequest.paramAsTime(TIMEOUT.getPreferredName(), StartTrainedModelDeploymentAction.DEFAULT_TIMEOUT),
                StartTrainedModelDeploymentAction.DEFAULT_TIMEOUT
            ); // hasParam, so never default
            request.setTimeout(openTimeout);
        }

        request.setWaitForState(
            validateParameters(
                request.getWaitForState(),
                AllocationStatus.State.fromString(
                    restRequest.param(WAIT_FOR.getPreferredName(), StartTrainedModelDeploymentAction.DEFAULT_WAITFOR_STATE.toString())
                ),
                StartTrainedModelDeploymentAction.DEFAULT_WAITFOR_STATE
            )
        );

        RestCompatibilityChecker.checkAndSetDeprecatedParam(
            NUMBER_OF_ALLOCATIONS.getDeprecatedNames()[0],
            NUMBER_OF_ALLOCATIONS.getPreferredName(),
            RestApiVersion.V_8,
            restRequest,
            (r, s) -> validateParameters(
                request.getNumberOfAllocations(),
                r.paramAsInt(s, StartTrainedModelDeploymentAction.DEFAULT_NUM_ALLOCATIONS),
                StartTrainedModelDeploymentAction.DEFAULT_NUM_ALLOCATIONS
            ),
            request::setNumberOfAllocations
        );
        RestCompatibilityChecker.checkAndSetDeprecatedParam(
            THREADS_PER_ALLOCATION.getDeprecatedNames()[0],
            THREADS_PER_ALLOCATION.getPreferredName(),
            RestApiVersion.V_8,
            restRequest,
            (r, s) -> validateParameters(
                request.getThreadsPerAllocation(),
                r.paramAsInt(s, StartTrainedModelDeploymentAction.DEFAULT_NUM_THREADS),
                StartTrainedModelDeploymentAction.DEFAULT_NUM_THREADS
            ),
            request::setThreadsPerAllocation
        );
        request.setQueueCapacity(
            validateParameters(
                request.getQueueCapacity(),
                restRequest.paramAsInt(QUEUE_CAPACITY.getPreferredName(), StartTrainedModelDeploymentAction.DEFAULT_QUEUE_CAPACITY),
                StartTrainedModelDeploymentAction.DEFAULT_QUEUE_CAPACITY
            )
        );

        if (restRequest.hasParam(CACHE_SIZE.getPreferredName())) {
            request.setCacheSize(
                validateParameters(
                    request.getCacheSize(),
                    ByteSizeValue.parseBytesSizeValue(restRequest.param(CACHE_SIZE.getPreferredName()), CACHE_SIZE.getPreferredName()),
                    null
                )
            );
        } else if (defaultCacheSize != null && request.getCacheSize() == null) {
            request.setCacheSize(defaultCacheSize);
        }

        request.setPriority(
            validateParameters(
                request.getPriority().toString(),
                restRequest.param(StartTrainedModelDeploymentAction.TaskParams.PRIORITY.getPreferredName()),
                StartTrainedModelDeploymentAction.DEFAULT_PRIORITY.toString()
            )
        );

        return channel -> client.execute(StartTrainedModelDeploymentAction.INSTANCE, request, new RestToXContentListener<>(channel));
    }

    /**
     * This function validates that the body and query parameters don't conflict, and returns the value that should be used.
     * When using this function, the body parameter should already have been set to the default value in
     * {@link StartTrainedModelDeploymentAction}, or, set to a different value from the rest request.
     *
     * @param paramDefault (from {@link StartTrainedModelDeploymentAction})
     * @return the parameter to use
     * @throws ElasticsearchStatusException if the parameters don't match
     */
    private static <T> T validateParameters(@Nullable T bodyParam, @Nullable T queryParam, @Nullable T paramDefault)
        throws ElasticsearchStatusException {
        if (Objects.equals(bodyParam, paramDefault) && queryParam != null) {
            // the body param is the same as the default for this value. We cannot tell if this was set intentionally, or if it was just the
            // default, thus we will assume it was the default
            return queryParam;
        }

        if (Objects.equals(bodyParam, queryParam)) {
            return bodyParam;
        } else if (bodyParam == null) {
            return queryParam;
        } else if (queryParam == null) {
            return bodyParam;
        } else {
            throw new ElasticsearchStatusException(
                "The parameter " + bodyParam + " in the body is different from the parameter " + queryParam + " in the query",
                RestStatus.BAD_REQUEST
            );
        }
    }
}
