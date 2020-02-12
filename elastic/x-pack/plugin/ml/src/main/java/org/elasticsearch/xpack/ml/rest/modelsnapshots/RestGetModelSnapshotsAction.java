/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.rest.modelsnapshots;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.rest.BaseRestHandler;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestToXContentListener;
import org.elasticsearch.xpack.core.action.util.PageParams;
import org.elasticsearch.xpack.core.ml.action.GetModelSnapshotsAction;
import org.elasticsearch.xpack.core.ml.action.GetModelSnapshotsAction.Request;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.ml.MachineLearning;

import java.io.IOException;
import java.util.Collections;
import java.util.List;

import static org.elasticsearch.rest.RestRequest.Method.GET;
import static org.elasticsearch.rest.RestRequest.Method.POST;

public class RestGetModelSnapshotsAction extends BaseRestHandler {

    private static final String ALL = "_all";
    private static final String ALL_SNAPSHOT_IDS = null;

    // Even though these are null, setting up the defaults in case
    // we want to change them later
    private static final String DEFAULT_SORT = null;
    private static final String DEFAULT_START = null;
    private static final String DEFAULT_END = null;
    private static final boolean DEFAULT_DESC_ORDER = true;

    @Override
    public List<Route> routes() {
        return Collections.emptyList();
    }

    @Override
    public List<ReplacedRoute> replacedRoutes() {
        // TODO: remove deprecated endpoint in 8.0.0
        return List.of(
            new ReplacedRoute(GET, MachineLearning.BASE_PATH + "anomaly_detectors/{"
                + Job.ID.getPreferredName() + "}/model_snapshots/{" + Request.SNAPSHOT_ID.getPreferredName() + "}",
                GET, MachineLearning.PRE_V7_BASE_PATH + "anomaly_detectors/{"
                + Job.ID.getPreferredName() + "}/model_snapshots/{" + Request.SNAPSHOT_ID.getPreferredName() + "}"),
            new ReplacedRoute(POST, MachineLearning.BASE_PATH + "anomaly_detectors/{"
                + Job.ID.getPreferredName() + "}/model_snapshots/{" + Request.SNAPSHOT_ID.getPreferredName() + "}",
                POST, MachineLearning.PRE_V7_BASE_PATH + "anomaly_detectors/{"
                + Job.ID.getPreferredName() + "}/model_snapshots/{" + Request.SNAPSHOT_ID.getPreferredName() + "}"),
            new ReplacedRoute(GET, MachineLearning.BASE_PATH + "anomaly_detectors/{"
                + Job.ID.getPreferredName() + "}/model_snapshots",
                GET, MachineLearning.PRE_V7_BASE_PATH + "anomaly_detectors/{"
                + Job.ID.getPreferredName() + "}/model_snapshots"),
            new ReplacedRoute(POST, MachineLearning.BASE_PATH + "anomaly_detectors/{"
                + Job.ID.getPreferredName() + "}/model_snapshots",
                POST, MachineLearning.PRE_V7_BASE_PATH + "anomaly_detectors/{"
                + Job.ID.getPreferredName() + "}/model_snapshots")
        );
    }

    @Override
    public String getName() {
        return "ml_get_model_snapshot_action";
    }

    @Override
    protected RestChannelConsumer prepareRequest(RestRequest restRequest, NodeClient client) throws IOException {
        String jobId = restRequest.param(Job.ID.getPreferredName());
        String snapshotId = restRequest.param(Request.SNAPSHOT_ID.getPreferredName());
        if (ALL.equals(snapshotId)) {
            snapshotId = ALL_SNAPSHOT_IDS;
        }
        Request getModelSnapshots;
        if (restRequest.hasContentOrSourceParam()) {
            XContentParser parser = restRequest.contentOrSourceParamParser();
            getModelSnapshots = Request.parseRequest(jobId, snapshotId, parser);
        } else {
            getModelSnapshots = new Request(jobId, snapshotId);
            getModelSnapshots.setSort(restRequest.param(Request.SORT.getPreferredName(), DEFAULT_SORT));
            if (restRequest.hasParam(Request.START.getPreferredName())) {
                getModelSnapshots.setStart(restRequest.param(Request.START.getPreferredName(), DEFAULT_START));
            }
            if (restRequest.hasParam(Request.END.getPreferredName())) {
                getModelSnapshots.setEnd(restRequest.param(Request.END.getPreferredName(), DEFAULT_END));
            }
            getModelSnapshots.setDescOrder(restRequest.paramAsBoolean(Request.DESC.getPreferredName(), DEFAULT_DESC_ORDER));
            getModelSnapshots.setPageParams(new PageParams(
                    restRequest.paramAsInt(PageParams.FROM.getPreferredName(), PageParams.DEFAULT_FROM),
                    restRequest.paramAsInt(PageParams.SIZE.getPreferredName(), PageParams.DEFAULT_SIZE)));
        }

        return channel -> client.execute(GetModelSnapshotsAction.INSTANCE, getModelSnapshots, new RestToXContentListener<>(channel));
    }
}
