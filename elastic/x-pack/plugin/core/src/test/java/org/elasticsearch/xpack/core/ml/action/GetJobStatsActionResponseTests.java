/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.action;

import org.elasticsearch.Version;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.transport.TransportAddress;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.test.AbstractStreamableTestCase;
import org.elasticsearch.xpack.core.ml.action.GetJobsStatsAction.Response;
import org.elasticsearch.xpack.core.ml.action.util.QueryPage;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.config.JobState;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.DataCounts;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.DataCountsTests;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSizeStats;

import java.net.InetAddress;
import java.util.ArrayList;
import java.util.EnumSet;
import java.util.List;

import static org.elasticsearch.common.unit.TimeValue.parseTimeValue;

public class GetJobStatsActionResponseTests extends AbstractStreamableTestCase<Response> {

    @Override
    protected Response createTestInstance() {
        final Response result;

        int listSize = randomInt(10);
        List<Response.JobStats> jobStatsList = new ArrayList<>(listSize);
        for (int j = 0; j < listSize; j++) {
            String jobId = randomAlphaOfLength(10);

            DataCounts dataCounts = new DataCountsTests().createTestInstance();

            ModelSizeStats sizeStats = null;
            if (randomBoolean()) {
                sizeStats = new ModelSizeStats.Builder("foo").build();
            }
            JobState jobState = randomFrom(EnumSet.allOf(JobState.class));

            DiscoveryNode node = null;
            if (randomBoolean()) {
                node = new DiscoveryNode("_id", new TransportAddress(InetAddress.getLoopbackAddress(), 9300), Version.CURRENT);
            }
            String explanation = null;
            if (randomBoolean()) {
                explanation = randomAlphaOfLength(3);
            }
            TimeValue openTime = null;
            if (randomBoolean()) {
                openTime = parseTimeValue(randomPositiveTimeValue(), "open_time-Test");
            }
            Response.JobStats jobStats = new Response.JobStats(jobId, dataCounts, sizeStats, jobState, node, explanation, openTime);
            jobStatsList.add(jobStats);
        }

        result = new Response(new QueryPage<>(jobStatsList, jobStatsList.size(), Job.RESULTS_FIELD));

        return result;
    }

    @Override
    protected Response createBlankInstance() {
        return new Response();
    }

}
