/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.client.node;

import org.elasticsearch.client.Client;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.test.ESIntegTestCase;
import org.elasticsearch.test.ESIntegTestCase.ClusterScope;
import org.elasticsearch.test.ESIntegTestCase.Scope;

import static org.hamcrest.Matchers.is;

@ClusterScope(scope = Scope.SUITE)
public class NodeClientIT extends ESIntegTestCase {
    @Override
    protected Settings nodeSettings(int nodeOrdinal) {
        return Settings.builder().put(super.nodeSettings(nodeOrdinal)).put(Client.CLIENT_TYPE_SETTING_S.getKey(), "anything").build();
    }

    public void testThatClientTypeSettingCannotBeChanged() {
        for (Settings settings : internalCluster().getInstances(Settings.class)) {
            assertThat(Client.CLIENT_TYPE_SETTING_S.get(settings), is("node"));
        }
    }
}
