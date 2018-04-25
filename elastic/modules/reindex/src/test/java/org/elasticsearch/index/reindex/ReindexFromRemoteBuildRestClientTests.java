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

package org.elasticsearch.index.reindex;

import org.elasticsearch.client.RestClient;
import org.elasticsearch.client.RestClientBuilderTestCase;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.test.ESTestCase;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static java.util.Collections.emptyMap;
import static java.util.Collections.synchronizedList;
import static org.hamcrest.Matchers.hasSize;

public class ReindexFromRemoteBuildRestClientTests extends RestClientBuilderTestCase {
    public void testBuildRestClient() throws Exception {
        RemoteInfo remoteInfo = new RemoteInfo("https", "localhost", 9200, new BytesArray("ignored"), null, null, emptyMap(),
                RemoteInfo.DEFAULT_SOCKET_TIMEOUT, RemoteInfo.DEFAULT_CONNECT_TIMEOUT);
        long taskId = randomLong();
        List<Thread> threads = synchronizedList(new ArrayList<>());
        RestClient client = TransportReindexAction.buildRestClient(remoteInfo, taskId, threads);
        try {
            assertBusy(() -> assertThat(threads, hasSize(2)));
            int i = 0;
            for (Thread thread : threads) {
                assertEquals("es-client-" + taskId + "-" + i, thread.getName());
                i++;
            }
        } finally {
            client.close();
        }
    }

    public void testHeaders() throws Exception {
        Map<String, String> headers = new HashMap<>();
        int numHeaders = randomIntBetween(1, 5);
        for (int i = 0; i < numHeaders; ++i) {
            headers.put("header" + i, Integer.toString(i));
        }
        RemoteInfo remoteInfo = new RemoteInfo("https", "localhost", 9200, new BytesArray("ignored"), null, null,
            headers, RemoteInfo.DEFAULT_SOCKET_TIMEOUT, RemoteInfo.DEFAULT_CONNECT_TIMEOUT);
        long taskId = randomLong();
        List<Thread> threads = synchronizedList(new ArrayList<>());
        RestClient client = TransportReindexAction.buildRestClient(remoteInfo, taskId, threads);
        try {
            assertHeaders(client, headers);
        } finally {
            client.close();
        }
    }
}
