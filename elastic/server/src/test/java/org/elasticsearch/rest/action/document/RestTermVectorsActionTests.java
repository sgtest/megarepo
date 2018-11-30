/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.rest.action.document;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.indices.breaker.NoneCircuitBreakerService;
import org.elasticsearch.rest.RestChannel;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.RestRequest.Method;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.rest.FakeRestChannel;
import org.elasticsearch.test.rest.FakeRestRequest;
import org.elasticsearch.usage.UsageService;

import java.util.Collections;

import static org.mockito.Mockito.mock;

public class RestTermVectorsActionTests extends ESTestCase {
    private RestController controller;

    public void setUp() throws Exception {
        super.setUp();
        controller = new RestController(Collections.emptySet(), null,
            mock(NodeClient.class),
            new NoneCircuitBreakerService(),
            new UsageService());
        new RestTermVectorsAction(Settings.EMPTY, controller);
    }

    public void testDeprecatedEndpoint() {
        RestRequest request = new FakeRestRequest.Builder(xContentRegistry())
            .withMethod(Method.POST)
            .withPath("/some_index/some_type/some_id/_termvector")
            .build();

        performRequest(request);
        assertWarnings("[POST /{index}/{type}/{id}/_termvector] is deprecated! Use" +
            " [POST /{index}/{type}/{id}/_termvectors] instead.");
    }

    private void performRequest(RestRequest request) {
        RestChannel channel = new FakeRestChannel(request, false, 1);
        ThreadContext threadContext = new ThreadContext(Settings.EMPTY);
        controller.dispatchRequest(request, channel, threadContext);
    }
}
