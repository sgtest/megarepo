/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.datastreams.action;

import org.elasticsearch.action.datastreams.GetDataStreamAction.Response;
import org.elasticsearch.cluster.health.ClusterHealthStatus;
import org.elasticsearch.cluster.metadata.DataStreamTestHelper;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.core.Tuple;
import org.elasticsearch.test.AbstractWireSerializingTestCase;

import java.time.Instant;
import java.util.ArrayList;
import java.util.List;

public class GetDataStreamsResponseTests extends AbstractWireSerializingTestCase<Response> {

    @Override
    protected Writeable.Reader<Response> instanceReader() {
        return Response::new;
    }

    @Override
    protected Response createTestInstance() {
        int numDataStreams = randomIntBetween(0, 8);
        List<Response.DataStreamInfo> dataStreams = new ArrayList<>();
        for (int i = 0; i < numDataStreams; i++) {
            List<Tuple<Instant, Instant>> timeSeries = null;
            if (randomBoolean()) {
                timeSeries = new ArrayList<>();
                int numTimeSeries = randomIntBetween(0, 3);
                for (int j = 0; j < numTimeSeries; j++) {
                    timeSeries.add(new Tuple<>(Instant.now(), Instant.now()));
                }
            }

            dataStreams.add(
                new Response.DataStreamInfo(
                    DataStreamTestHelper.randomInstance(),
                    ClusterHealthStatus.GREEN,
                    randomAlphaOfLengthBetween(2, 10),
                    randomAlphaOfLengthBetween(2, 10),
                    timeSeries != null ? new Response.TimeSeries(timeSeries) : null
                )
            );
        }
        return new Response(dataStreams);
    }

    @Override
    protected Response mutateInstance(Response instance) {
        return null;// TODO implement https://github.com/elastic/elasticsearch/issues/25929
    }
}
