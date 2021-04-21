/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.core.slm;

import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.test.AbstractSerializingTestCase;

import java.io.IOException;
import java.util.HashMap;
import java.util.Map;

public class SnapshotLifecycleStatsTests extends AbstractSerializingTestCase<SnapshotLifecycleStats> {
    @Override
    protected SnapshotLifecycleStats doParseInstance(XContentParser parser) throws IOException {
        return SnapshotLifecycleStats.parse(parser);
    }

    public static SnapshotLifecycleStats.SnapshotPolicyStats randomPolicyStats(String policyId) {
        return new SnapshotLifecycleStats.SnapshotPolicyStats(policyId,
            randomBoolean() ? 0 : randomIntBetween(0, Integer.MAX_VALUE),
            randomBoolean() ? 0 : randomIntBetween(0, Integer.MAX_VALUE),
            randomBoolean() ? 0 : randomIntBetween(0, Integer.MAX_VALUE),
            randomBoolean() ? 0 : randomIntBetween(0, Integer.MAX_VALUE));
    }

    public static SnapshotLifecycleStats randomLifecycleStats() {
        int policies = randomIntBetween(0, 5);
        Map<String, SnapshotLifecycleStats.SnapshotPolicyStats> policyStats = new HashMap<>(policies);
        for (int i = 0; i < policies; i++) {
            String policy = "policy-" + randomAlphaOfLength(4);
            policyStats.put(policy, randomPolicyStats(policy));
        }
        return new SnapshotLifecycleStats(
            randomBoolean() ? 0 : randomIntBetween(0, Integer.MAX_VALUE),
            randomBoolean() ? 0 : randomIntBetween(0, Integer.MAX_VALUE),
            randomBoolean() ? 0 : randomIntBetween(0, Integer.MAX_VALUE),
            randomBoolean() ? 0 : randomIntBetween(0, Integer.MAX_VALUE),
            policyStats);
    }

    @Override
    protected SnapshotLifecycleStats createTestInstance() {
        return randomLifecycleStats();
    }

    @Override
    protected SnapshotLifecycleStats mutateInstance(SnapshotLifecycleStats instance) throws IOException {
        return randomValueOtherThan(instance, () -> instance.merge(createTestInstance()));
    }

    @Override
    protected Writeable.Reader<SnapshotLifecycleStats> instanceReader() {
        return SnapshotLifecycleStats::new;
    }
}
