/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.protocol.xpack.migration;

import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.test.AbstractStreamableXContentTestCase;

import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;

public class IndexUpgradeInfoResponseTests extends AbstractStreamableXContentTestCase<IndexUpgradeInfoResponse> {
    @Override
    protected IndexUpgradeInfoResponse doParseInstance(XContentParser parser) {
        return IndexUpgradeInfoResponse.fromXContent(parser);
    }

    @Override
    protected IndexUpgradeInfoResponse createBlankInstance() {
        return new IndexUpgradeInfoResponse();
    }

    @Override
    protected IndexUpgradeInfoResponse createTestInstance() {
        return randomIndexUpgradeInfoResponse(randomIntBetween(0, 10));
    }

    private static IndexUpgradeInfoResponse randomIndexUpgradeInfoResponse(int numIndices) {
        Map<String, UpgradeActionRequired> actions = new HashMap<>();
        for (int i = 0; i < numIndices; i++) {
            actions.put(randomAlphaOfLength(5), randomFrom(UpgradeActionRequired.values()));
        }
        return new IndexUpgradeInfoResponse(actions);
    }

    @Override
    protected IndexUpgradeInfoResponse mutateInstance(IndexUpgradeInfoResponse instance) {
        if (instance.getActions().size() == 0) {
            return randomIndexUpgradeInfoResponse(1);
        }
        Map<String, UpgradeActionRequired> actions = new HashMap<>(instance.getActions());
        if (randomBoolean()) {
            Iterator<Map.Entry<String, UpgradeActionRequired>> iterator = actions.entrySet().iterator();
            iterator.next();
            iterator.remove();
        } else {
            actions.put(randomAlphaOfLength(5), randomFrom(UpgradeActionRequired.values()));
        }
        return new IndexUpgradeInfoResponse(actions);
    }
}
