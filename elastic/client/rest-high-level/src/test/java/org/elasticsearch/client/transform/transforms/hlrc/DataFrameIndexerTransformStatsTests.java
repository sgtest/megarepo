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

package org.elasticsearch.client.transform.transforms.hlrc;

import org.elasticsearch.client.AbstractHlrcXContentTestCase;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.xpack.core.transform.transforms.TransformIndexerStats;

import java.io.IOException;

public class DataFrameIndexerTransformStatsTests extends AbstractHlrcXContentTestCase<
        TransformIndexerStats,
        org.elasticsearch.client.transform.transforms.DataFrameIndexerTransformStats> {

    public static TransformIndexerStats fromHlrc(
            org.elasticsearch.client.transform.transforms.DataFrameIndexerTransformStats instance) {
        return new TransformIndexerStats(
            instance.getNumPages(),
            instance.getNumDocuments(),
            instance.getOutputDocuments(),
            instance.getNumInvocations(),
            instance.getIndexTime(),
            instance.getSearchTime(),
            instance.getIndexTotal(),
            instance.getSearchTotal(),
            instance.getIndexFailures(),
            instance.getSearchFailures(),
            instance.getExpAvgCheckpointDurationMs(),
            instance.getExpAvgDocumentsIndexed(),
            instance.getExpAvgDocumentsProcessed());
    }

    @Override
    public org.elasticsearch.client.transform.transforms.DataFrameIndexerTransformStats doHlrcParseInstance(XContentParser parser)
            throws IOException {
        return org.elasticsearch.client.transform.transforms.DataFrameIndexerTransformStats.fromXContent(parser);
    }

    @Override
    public TransformIndexerStats convertHlrcToInternal(
            org.elasticsearch.client.transform.transforms.DataFrameIndexerTransformStats instance) {
        return fromHlrc(instance);
    }

    public static TransformIndexerStats randomStats() {
        return new TransformIndexerStats(randomLongBetween(10L, 10000L),
            randomLongBetween(0L, 10000L), randomLongBetween(0L, 10000L), randomLongBetween(0L, 10000L), randomLongBetween(0L, 10000L),
            randomLongBetween(0L, 10000L), randomLongBetween(0L, 10000L), randomLongBetween(0L, 10000L), randomLongBetween(0L, 10000L),
            randomLongBetween(0L, 10000L),
            randomBoolean() ? null : randomDouble(),
            randomBoolean() ? null : randomDouble(),
            randomBoolean() ? null : randomDouble());
    }

    @Override
    protected TransformIndexerStats createTestInstance() {
        return randomStats();
    }

    @Override
    protected TransformIndexerStats doParseInstance(XContentParser parser) throws IOException {
        return TransformIndexerStats.fromXContent(parser);
    }

    @Override
    protected boolean supportsUnknownFields() {
        return true;
    }

}
