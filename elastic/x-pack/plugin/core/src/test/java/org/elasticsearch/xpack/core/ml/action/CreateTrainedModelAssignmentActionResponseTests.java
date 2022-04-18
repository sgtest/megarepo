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
import org.elasticsearch.xpack.core.ml.action.CreateTrainedModelAssignmentAction.Response;
import org.elasticsearch.xpack.core.ml.inference.assignment.TrainedModelAssignmentTests;

import java.io.IOException;

public class CreateTrainedModelAssignmentActionResponseTests extends AbstractSerializingTestCase<Response> {

    @Override
    protected Response createTestInstance() {
        return new Response(TrainedModelAssignmentTests.randomInstance());
    }

    @Override
    protected Writeable.Reader<Response> instanceReader() {
        return Response::new;
    }

    @Override
    protected Response doParseInstance(XContentParser parser) throws IOException {
        return Response.fromXContent(parser);
    }
}
