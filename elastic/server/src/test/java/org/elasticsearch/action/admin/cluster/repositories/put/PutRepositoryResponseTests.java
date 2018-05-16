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
package org.elasticsearch.action.admin.cluster.repositories.put;

import org.elasticsearch.action.admin.cluster.repositories.put.PutRepositoryResponse;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.test.AbstractStreamableXContentTestCase;
import org.elasticsearch.test.ESTestCase;

import java.io.IOException;

import static org.elasticsearch.test.ESTestCase.randomBoolean;
import static org.hamcrest.Matchers.equalTo;

public class PutRepositoryResponseTests extends AbstractStreamableXContentTestCase<PutRepositoryResponse> {

    @Override
    protected PutRepositoryResponse doParseInstance(XContentParser parser) throws IOException {
        return PutRepositoryResponse.fromXContent(parser);
    }

    @Override
    protected PutRepositoryResponse createBlankInstance() {
        return new PutRepositoryResponse();
    }

    @Override
    protected PutRepositoryResponse createTestInstance() {
        return new PutRepositoryResponse(randomBoolean());
    }
}
