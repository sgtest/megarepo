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
package org.elasticsearch.transport.netty4;

import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.lease.Releasables;
import org.elasticsearch.common.network.NetworkService;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.indices.breaker.CircuitBreakerService;
import org.elasticsearch.indices.breaker.NoneCircuitBreakerService;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.transport.MockTransportService;
import org.elasticsearch.threadpool.TestThreadPool;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TcpTransport;
import org.elasticsearch.transport.TransportChannel;
import org.elasticsearch.transport.TransportException;
import org.elasticsearch.transport.TransportRequest;
import org.elasticsearch.transport.TransportRequestHandler;
import org.elasticsearch.transport.TransportRequestOptions;
import org.elasticsearch.transport.TransportResponse;
import org.elasticsearch.transport.TransportResponseHandler;
import org.elasticsearch.transport.TransportResponseOptions;
import org.elasticsearch.transport.TransportService;

import java.io.IOException;
import java.util.Collections;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;

public class Netty4ScheduledPingTests extends ESTestCase {
    public void testScheduledPing() throws Exception {
        ThreadPool threadPool = new TestThreadPool(getClass().getName());

        Settings settings = Settings.builder()
            .put(TcpTransport.PING_SCHEDULE.getKey(), "5ms")
            .put(TcpTransport.PORT.getKey(), 0)
            .put("cluster.name", "test")
            .build();

        CircuitBreakerService circuitBreakerService = new NoneCircuitBreakerService();

        NamedWriteableRegistry registry = new NamedWriteableRegistry(Collections.emptyList());
        final Netty4Transport nettyA = new Netty4Transport(settings, threadPool, new NetworkService(Collections.emptyList()),
            BigArrays.NON_RECYCLING_INSTANCE, registry, circuitBreakerService);
        MockTransportService serviceA = new MockTransportService(settings, nettyA, threadPool, TransportService.NOOP_TRANSPORT_INTERCEPTOR,
                null);
        serviceA.start();
        serviceA.acceptIncomingRequests();

        final Netty4Transport nettyB = new Netty4Transport(settings, threadPool, new NetworkService(Collections.emptyList()),
            BigArrays.NON_RECYCLING_INSTANCE, registry, circuitBreakerService);
        MockTransportService serviceB = new MockTransportService(settings, nettyB, threadPool, TransportService.NOOP_TRANSPORT_INTERCEPTOR,
                null);

        serviceB.start();
        serviceB.acceptIncomingRequests();

        DiscoveryNode nodeA = serviceA.getLocalDiscoNode();
        DiscoveryNode nodeB = serviceB.getLocalDiscoNode();

        serviceA.connectToNode(nodeB);
        serviceB.connectToNode(nodeA);

        assertBusy(() -> {
            assertThat(nettyA.getPing().getSuccessfulPings(), greaterThan(100L));
            assertThat(nettyB.getPing().getSuccessfulPings(), greaterThan(100L));
        });
        assertThat(nettyA.getPing().getFailedPings(), equalTo(0L));
        assertThat(nettyB.getPing().getFailedPings(), equalTo(0L));

        serviceA.registerRequestHandler("sayHello", TransportRequest.Empty::new, ThreadPool.Names.GENERIC,
            new TransportRequestHandler<TransportRequest.Empty>() {
                @Override
                public void messageReceived(TransportRequest.Empty request, TransportChannel channel) {
                    try {
                        channel.sendResponse(TransportResponse.Empty.INSTANCE, TransportResponseOptions.EMPTY);
                    } catch (IOException e) {
                        logger.error("Unexpected failure", e);
                        fail(e.getMessage());
                    }
                }
            });

        int rounds = scaledRandomIntBetween(100, 5000);
        for (int i = 0; i < rounds; i++) {
            serviceB.submitRequest(nodeA, "sayHello",
                TransportRequest.Empty.INSTANCE, TransportRequestOptions.builder().withCompress(randomBoolean()).build(),
                new TransportResponseHandler<TransportResponse.Empty>() {
                    @Override
                    public TransportResponse.Empty newInstance() {
                        return TransportResponse.Empty.INSTANCE;
                    }

                    @Override
                    public String executor() {
                        return ThreadPool.Names.GENERIC;
                    }

                    @Override
                    public void handleResponse(TransportResponse.Empty response) {
                    }

                    @Override
                    public void handleException(TransportException exp) {
                        logger.error("Unexpected failure", exp);
                        fail("got exception instead of a response: " + exp.getMessage());
                    }
                }).txGet();
        }

        assertBusy(() -> {
            assertThat(nettyA.getPing().getSuccessfulPings(), greaterThan(200L));
            assertThat(nettyB.getPing().getSuccessfulPings(), greaterThan(200L));
        });
        assertThat(nettyA.getPing().getFailedPings(), equalTo(0L));
        assertThat(nettyB.getPing().getFailedPings(), equalTo(0L));

        Releasables.close(serviceA, serviceB);
        terminate(threadPool);
    }

}
