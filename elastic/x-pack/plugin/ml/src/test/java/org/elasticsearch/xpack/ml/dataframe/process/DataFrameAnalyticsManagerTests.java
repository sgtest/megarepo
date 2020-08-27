/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.dataframe.process;

import org.elasticsearch.client.node.NodeClient;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.ml.dataframe.DataFrameAnalyticsManager;
import org.elasticsearch.xpack.ml.dataframe.persistence.DataFrameAnalyticsConfigProvider;
import org.elasticsearch.xpack.ml.notifications.DataFrameAnalyticsAuditor;

import static org.hamcrest.Matchers.is;
import static org.mockito.Mockito.mock;

public class DataFrameAnalyticsManagerTests extends ESTestCase {

    public void testNodeShuttingDown() {
        DataFrameAnalyticsManager manager =
            new DataFrameAnalyticsManager(
                mock(NodeClient.class),
                mock(DataFrameAnalyticsConfigProvider.class),
                mock(AnalyticsProcessManager.class),
                mock(DataFrameAnalyticsAuditor.class),
                mock(IndexNameExpressionResolver.class));
        assertThat(manager.isNodeShuttingDown(), is(false));

        manager.markNodeAsShuttingDown();
        assertThat(manager.isNodeShuttingDown(), is(true));
    }
}
