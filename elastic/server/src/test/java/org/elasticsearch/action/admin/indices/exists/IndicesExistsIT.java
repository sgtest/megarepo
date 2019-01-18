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

package org.elasticsearch.action.admin.indices.exists;

import org.elasticsearch.cluster.coordination.ClusterBootstrapService;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.discovery.MasterNotDiscoveredException;
import org.elasticsearch.discovery.zen.ElectMasterService;
import org.elasticsearch.gateway.GatewayService;
import org.elasticsearch.test.ESIntegTestCase;
import org.elasticsearch.test.ESIntegTestCase.ClusterScope;
import org.elasticsearch.test.InternalTestCluster;

import java.io.IOException;
import java.util.List;
import java.util.stream.Collectors;

import static org.elasticsearch.node.Node.NODE_MASTER_SETTING;
import static org.elasticsearch.node.Node.NODE_NAME_SETTING;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertThrows;

@ClusterScope(scope = ESIntegTestCase.Scope.TEST, numDataNodes = 0, numClientNodes = 0, transportClientRatio = 0.0,
    autoMinMasterNodes = false)
public class IndicesExistsIT extends ESIntegTestCase {

    @Override
    protected List<Settings> addExtraClusterBootstrapSettings(List<Settings> allNodesSettings) {
        final List<String> masterNodeNames
            = allNodesSettings.stream().filter(NODE_MASTER_SETTING::get).map(NODE_NAME_SETTING::get).collect(Collectors.toList());
        return allNodesSettings.stream().map(s -> Settings.builder().put(s)
            .putList(ClusterBootstrapService.INITIAL_MASTER_NODES_SETTING.getKey(), masterNodeNames).build()).collect(Collectors.toList());
    }

    public void testIndexExistsWithBlocksInPlace() throws IOException {
        Settings settings = Settings.builder()
            .put(GatewayService.RECOVER_AFTER_NODES_SETTING.getKey(), 99)
            .put(ElectMasterService.DISCOVERY_ZEN_MINIMUM_MASTER_NODES_SETTING.getKey(), Integer.MAX_VALUE).build();
        String node = internalCluster().startNode(settings);

        assertThrows(client(node).admin().indices().prepareExists("test").setMasterNodeTimeout(TimeValue.timeValueSeconds(0)),
            MasterNotDiscoveredException.class);

        internalCluster().stopRandomNode(InternalTestCluster.nameFilter(node)); // shut down node so that test properly cleans up
    }
}
