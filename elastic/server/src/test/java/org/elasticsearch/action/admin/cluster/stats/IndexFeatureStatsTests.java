/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.admin.cluster.stats;

import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.test.AbstractWireSerializingTestCase;

public class IndexFeatureStatsTests extends AbstractWireSerializingTestCase<IndexFeatureStats> {
    @Override
    protected Writeable.Reader<IndexFeatureStats> instanceReader() {
        return IndexFeatureStats::new;
    }

    @Override
    protected IndexFeatureStats createTestInstance() {
        IndexFeatureStats indexFeatureStats = new IndexFeatureStats(randomAlphaOfLengthBetween(3, 10));
        indexFeatureStats.indexCount = randomIntBetween(0, Integer.MAX_VALUE);
        indexFeatureStats.count = randomIntBetween(0, Integer.MAX_VALUE);
        return indexFeatureStats;
    }

    public void testToXContent() {
        IndexFeatureStats testInstance = createTestInstance();
        assertEquals("{\"name\":\"" + testInstance.name +
            "\",\"count\":" + testInstance.count +
            ",\"index_count\":" + testInstance.indexCount + "}", Strings.toString(testInstance));
    }
}
