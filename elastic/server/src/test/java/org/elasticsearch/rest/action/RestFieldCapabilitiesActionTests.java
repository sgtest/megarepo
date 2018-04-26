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

package org.elasticsearch.rest.action;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.rest.action.RestFieldCapabilitiesAction;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.rest.FakeRestRequest;
import org.elasticsearch.usage.UsageService;
import org.junit.Before;

import java.io.IOException;
import java.util.Collections;

import static org.mockito.Mockito.mock;

public class RestFieldCapabilitiesActionTests extends ESTestCase {

    private RestFieldCapabilitiesAction action;

    @Before
    public void setUpAction() {
        action = new RestFieldCapabilitiesAction(Settings.EMPTY, mock(RestController.class));
    }

    public void testRequestBodyIsDeprecated() throws IOException {
        String content = "{ \"fields\": [\"title\"] }";
        RestRequest request = new FakeRestRequest.Builder(xContentRegistry())
            .withPath("/_field_caps")
            .withContent(new BytesArray(content), XContentType.JSON)
            .build();
        action.prepareRequest(request, mock(NodeClient.class));

        assertWarnings("Specifying a request body is deprecated -- the" +
            " [fields] request parameter should be used instead.");
    }
}
