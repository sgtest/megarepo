/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.action;

import org.elasticsearch.test.AbstractStreamableTestCase;
import org.elasticsearch.xpack.core.ml.job.config.AnalysisLimits;
import org.elasticsearch.xpack.core.ml.job.config.JobUpdate;

public class UpdateJobActionRequestTests
        extends AbstractStreamableTestCase<UpdateJobAction.Request> {

    @Override
    protected UpdateJobAction.Request createTestInstance() {
        String jobId = randomAlphaOfLength(10);
        // no need to randomize JobUpdate this is already tested in: JobUpdateTests
        JobUpdate.Builder jobUpdate = new JobUpdate.Builder(jobId);
        jobUpdate.setAnalysisLimits(new AnalysisLimits(100L, 100L));
        UpdateJobAction.Request request = new UpdateJobAction.Request(jobId, jobUpdate.build());
        request.setWaitForAck(randomBoolean());
        return request;
    }

    @Override
    protected UpdateJobAction.Request createBlankInstance() {
        return new UpdateJobAction.Request();
    }

}
