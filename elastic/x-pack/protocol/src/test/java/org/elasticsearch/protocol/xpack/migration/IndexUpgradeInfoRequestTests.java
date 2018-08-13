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

package org.elasticsearch.protocol.xpack.migration;

import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.test.AbstractWireSerializingTestCase;

public class IndexUpgradeInfoRequestTests extends AbstractWireSerializingTestCase<IndexUpgradeInfoRequest> {
    @Override
    protected IndexUpgradeInfoRequest createTestInstance() {
        int indexCount = randomInt(4);
        String[] indices = new String[indexCount];
        for (int i = 0; i < indexCount; i++) {
            indices[i] = randomAlphaOfLength(10);
        }
        IndexUpgradeInfoRequest request = new IndexUpgradeInfoRequest(indices);
        if (randomBoolean()) {
            request.indicesOptions(IndicesOptions.fromOptions(randomBoolean(), randomBoolean(), randomBoolean(), randomBoolean()));
        }
        return request;
    }

    @Override
    protected Writeable.Reader<IndexUpgradeInfoRequest> instanceReader() {
        return IndexUpgradeInfoRequest::new;
    }

    public void testNullIndices() {
        expectThrows(NullPointerException.class, () -> new IndexUpgradeInfoRequest((String[])null));
        expectThrows(NullPointerException.class, () -> new IndexUpgradeInfoRequest().indices((String[])null));
    }
}
