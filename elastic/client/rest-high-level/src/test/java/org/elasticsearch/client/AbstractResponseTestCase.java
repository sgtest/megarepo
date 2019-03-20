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
package org.elasticsearch.client;

import org.elasticsearch.cluster.ClusterModule;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.xcontent.LoggingDeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContent;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.test.ESTestCase;

import java.io.IOException;

/**
 * Base class for HLRC response parsing tests.
 *
 * This case class facilitates generating server side reponse test instances and
 * verifies that they are correctly parsed into HLRC response instances.
 *
 * @param <S> The class representing the response on the server side.
 * @param <C> The class representing the response on the client side.
 */
public abstract class AbstractResponseTestCase<S extends ToXContent, C> extends ESTestCase {

    private static final int NUMBER_OF_TEST_RUNS = 20;

    public final void testFromXContent() throws IOException {
        for (int i = 0; i < NUMBER_OF_TEST_RUNS; i++) {
            final S serverTestInstance = createServerTestInstance();

            final XContentType xContentType = randomFrom(XContentType.values());
            final BytesReference bytes = toShuffledXContent(serverTestInstance, xContentType, ToXContent.EMPTY_PARAMS, randomBoolean());

            final XContent xContent = XContentFactory.xContent(xContentType);
            final XContentParser parser = xContent.createParser(
                new NamedXContentRegistry(ClusterModule.getNamedXWriteables()),
                LoggingDeprecationHandler.INSTANCE,
                bytes.streamInput());
            final C clientInstance = doParseToClientInstance(parser);
            assertInstances(serverTestInstance, clientInstance);
        }
    }

    protected abstract S createServerTestInstance();

    protected abstract C doParseToClientInstance(XContentParser parser) throws IOException;

    protected abstract void assertInstances(S serverTestInstance, C clientInstance);

}
