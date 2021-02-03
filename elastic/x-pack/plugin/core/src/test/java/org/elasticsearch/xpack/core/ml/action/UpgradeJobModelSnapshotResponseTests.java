/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.core.ml.action;

import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.test.AbstractSerializingTestCase;
import org.elasticsearch.xpack.core.ml.action.UpgradeJobModelSnapshotAction.Response;

public class UpgradeJobModelSnapshotResponseTests extends AbstractSerializingTestCase<Response> {

    @Override
    protected Response createTestInstance() {
        return new Response(randomBoolean(), randomBoolean() ? null : randomAlphaOfLength(10));
    }

    @Override
    protected Writeable.Reader<Response> instanceReader() {
        return Response::new;
    }

    @Override
    protected boolean supportsUnknownFields() {
        return false;
    }

    @Override
    protected Response doParseInstance(XContentParser parser) {
        return Response.parseRequest(parser);
    }
}
