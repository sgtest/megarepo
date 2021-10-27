/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.core.ml.action;

import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.test.AbstractSerializingTestCase;
import org.elasticsearch.xcontent.XContentParser;
import org.elasticsearch.xpack.core.ml.action.StartTrainedModelDeploymentAction.TaskParams;

import java.io.IOException;

public class StartTrainedModelDeploymentTaskParamsTests extends AbstractSerializingTestCase<TaskParams> {

    @Override
    protected TaskParams doParseInstance(XContentParser parser) throws IOException {
        return TaskParams.fromXContent(parser);
    }

    @Override
    protected Writeable.Reader<TaskParams> instanceReader() {
        return TaskParams::new;
    }

    @Override
    protected TaskParams createTestInstance() {
        return createRandom();
    }

    public static StartTrainedModelDeploymentAction.TaskParams createRandom() {
        return new TaskParams(
            randomAlphaOfLength(10),
            randomNonNegativeLong(),
            randomIntBetween(1, 8),
            randomIntBetween(1, 8),
            randomIntBetween(1, 10000)
        );
    }
}
