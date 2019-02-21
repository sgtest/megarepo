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
package org.elasticsearch.cluster.coordination;

import joptsimple.OptionSet;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.admin.cluster.settings.ClusterUpdateSettingsRequest;
import org.elasticsearch.cli.MockTerminal;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.Manifest;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.discovery.DiscoverySettings;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.NodeEnvironment;
import org.elasticsearch.env.NodeMetaData;
import org.elasticsearch.env.TestEnvironment;
import org.elasticsearch.node.Node;
import org.elasticsearch.test.ESIntegTestCase;
import org.elasticsearch.test.InternalTestCluster;
import org.elasticsearch.test.junit.annotations.TestLogging;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

import static org.elasticsearch.action.support.WriteRequest.RefreshPolicy.IMMEDIATE;
import static org.elasticsearch.index.query.QueryBuilders.matchAllQuery;
import static org.elasticsearch.indices.recovery.RecoverySettings.INDICES_RECOVERY_MAX_BYTES_PER_SEC_SETTING;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertHitCount;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;

@ESIntegTestCase.ClusterScope(scope = ESIntegTestCase.Scope.TEST, numDataNodes = 0, autoMinMasterNodes = false)
@TestLogging("_root:DEBUG,org.elasticsearch.cluster.service:TRACE,org.elasticsearch.discovery.zen:TRACE")
public class ElasticsearchNodeCommandIT extends ESIntegTestCase {

    private MockTerminal executeCommand(ElasticsearchNodeCommand command, Environment environment, int nodeOrdinal, boolean abort)
            throws Exception {
        final MockTerminal terminal = new MockTerminal();
        final OptionSet options = command.getParser().parse("-ordinal", Integer.toString(nodeOrdinal));
        final String input;

        if (abort) {
            input = randomValueOtherThanMany(c -> c.equalsIgnoreCase("y"), () -> randomAlphaOfLength(1));
        } else {
            input = randomBoolean() ? "y" : "Y";
        }

        terminal.addTextInput(input);

        try {
            command.execute(terminal, options, environment);
        } finally {
            assertThat(terminal.getOutput(), containsString(ElasticsearchNodeCommand.STOP_WARNING_MSG));
        }

        return terminal;
    }

    private MockTerminal unsafeBootstrap(Environment environment, int nodeOrdinal, boolean abort) throws Exception {
        final MockTerminal terminal = executeCommand(new UnsafeBootstrapMasterCommand(), environment, nodeOrdinal, abort);
        assertThat(terminal.getOutput(), containsString(UnsafeBootstrapMasterCommand.CONFIRMATION_MSG));
        assertThat(terminal.getOutput(), containsString(UnsafeBootstrapMasterCommand.MASTER_NODE_BOOTSTRAPPED_MSG));
        return terminal;
    }

    private MockTerminal detachCluster(Environment environment, int nodeOrdinal, boolean abort) throws Exception {
        final MockTerminal terminal = executeCommand(new DetachClusterCommand(), environment, nodeOrdinal, abort);
        assertThat(terminal.getOutput(), containsString(DetachClusterCommand.CONFIRMATION_MSG));
        assertThat(terminal.getOutput(), containsString(DetachClusterCommand.NODE_DETACHED_MSG));
        return terminal;
    }

    private MockTerminal unsafeBootstrap(Environment environment) throws Exception {
        return unsafeBootstrap(environment, 0, false);
    }

    private MockTerminal detachCluster(Environment environment) throws Exception {
        return detachCluster(environment, 0, false);
    }

    private void expectThrows(ThrowingRunnable runnable, String message) {
        ElasticsearchException ex = expectThrows(ElasticsearchException.class, runnable);
        assertThat(ex.getMessage(), containsString(message));
    }

    public void testBootstrapNotMasterEligible() {
        final Environment environment = TestEnvironment.newEnvironment(Settings.builder()
                .put(internalCluster().getDefaultSettings())
                .put(Node.NODE_MASTER_SETTING.getKey(), false)
                .build());
        expectThrows(() -> unsafeBootstrap(environment), UnsafeBootstrapMasterCommand.NOT_MASTER_NODE_MSG);
    }

    public void testBootstrapNoDataFolder() {
        final Environment environment = TestEnvironment.newEnvironment(internalCluster().getDefaultSettings());
        expectThrows(() -> unsafeBootstrap(environment), ElasticsearchNodeCommand.NO_NODE_FOLDER_FOUND_MSG);
    }

    public void testDetachNoDataFolder() {
        final Environment environment = TestEnvironment.newEnvironment(internalCluster().getDefaultSettings());
        expectThrows(() -> detachCluster(environment), ElasticsearchNodeCommand.NO_NODE_FOLDER_FOUND_MSG);
    }

    public void testBootstrapNodeLocked() throws IOException {
        Settings envSettings = buildEnvSettings(Settings.EMPTY);
        Environment environment = TestEnvironment.newEnvironment(envSettings);
        try (NodeEnvironment ignored = new NodeEnvironment(envSettings, environment)) {
            expectThrows(() -> unsafeBootstrap(environment), ElasticsearchNodeCommand.FAILED_TO_OBTAIN_NODE_LOCK_MSG);
        }
    }

    public void testDetachNodeLocked() throws IOException {
        Settings envSettings = buildEnvSettings(Settings.EMPTY);
        Environment environment = TestEnvironment.newEnvironment(envSettings);
        try (NodeEnvironment ignored = new NodeEnvironment(envSettings, environment)) {
            expectThrows(() -> detachCluster(environment), ElasticsearchNodeCommand.FAILED_TO_OBTAIN_NODE_LOCK_MSG);
        }
    }

    public void testBootstrapNoNodeMetaData() throws IOException {
        Settings envSettings = buildEnvSettings(Settings.EMPTY);
        Environment environment = TestEnvironment.newEnvironment(envSettings);
        try (NodeEnvironment nodeEnvironment = new NodeEnvironment(envSettings, environment)) {
            NodeMetaData.FORMAT.cleanupOldFiles(-1, nodeEnvironment.nodeDataPaths());
        }

        expectThrows(() -> unsafeBootstrap(environment), UnsafeBootstrapMasterCommand.NO_NODE_METADATA_FOUND_MSG);
    }

    public void testBootstrapNotBootstrappedCluster() throws Exception {
        internalCluster().startNode(
                Settings.builder()
                        .put(DiscoverySettings.INITIAL_STATE_TIMEOUT_SETTING.getKey(), "0s") // to ensure quick node startup
                        .build());
        assertBusy(() -> {
            ClusterState state = client().admin().cluster().prepareState().setLocal(true)
                    .execute().actionGet().getState();
            assertTrue(state.blocks().hasGlobalBlockWithId(NoMasterBlockService.NO_MASTER_BLOCK_ID));
        });

        internalCluster().stopRandomDataNode();

        Environment environment = TestEnvironment.newEnvironment(internalCluster().getDefaultSettings());
        expectThrows(() -> unsafeBootstrap(environment), ElasticsearchNodeCommand.GLOBAL_GENERATION_MISSING_MSG);
    }

    public void testDetachNotBootstrappedCluster() throws Exception {
        internalCluster().startNode(
                Settings.builder()
                        .put(DiscoverySettings.INITIAL_STATE_TIMEOUT_SETTING.getKey(), "0s") // to ensure quick node startup
                        .build());
        assertBusy(() -> {
            ClusterState state = client().admin().cluster().prepareState().setLocal(true)
                    .execute().actionGet().getState();
            assertTrue(state.blocks().hasGlobalBlockWithId(NoMasterBlockService.NO_MASTER_BLOCK_ID));
        });

        internalCluster().stopRandomDataNode();

        Environment environment = TestEnvironment.newEnvironment(internalCluster().getDefaultSettings());
        expectThrows(() -> detachCluster(environment), ElasticsearchNodeCommand.GLOBAL_GENERATION_MISSING_MSG);
    }

    public void testBootstrapNoManifestFile() throws IOException {
        internalCluster().setBootstrapMasterNodeIndex(0);
        internalCluster().startNode();
        ensureStableCluster(1);
        NodeEnvironment nodeEnvironment = internalCluster().getMasterNodeInstance(NodeEnvironment.class);
        internalCluster().stopRandomDataNode();
        Environment environment = TestEnvironment.newEnvironment(internalCluster().getDefaultSettings());
        Manifest.FORMAT.cleanupOldFiles(-1, nodeEnvironment.nodeDataPaths());

        expectThrows(() -> unsafeBootstrap(environment), ElasticsearchNodeCommand.NO_MANIFEST_FILE_FOUND_MSG);
    }

    public void testDetachNoManifestFile() throws IOException {
        internalCluster().setBootstrapMasterNodeIndex(0);
        internalCluster().startNode();
        ensureStableCluster(1);
        NodeEnvironment nodeEnvironment = internalCluster().getMasterNodeInstance(NodeEnvironment.class);
        internalCluster().stopRandomDataNode();
        Environment environment = TestEnvironment.newEnvironment(internalCluster().getDefaultSettings());
        Manifest.FORMAT.cleanupOldFiles(-1, nodeEnvironment.nodeDataPaths());

        expectThrows(() -> detachCluster(environment), ElasticsearchNodeCommand.NO_MANIFEST_FILE_FOUND_MSG);
    }

    public void testBootstrapNoMetaData() throws IOException {
        internalCluster().setBootstrapMasterNodeIndex(0);
        internalCluster().startNode();
        ensureStableCluster(1);
        NodeEnvironment nodeEnvironment = internalCluster().getMasterNodeInstance(NodeEnvironment.class);
        internalCluster().stopRandomDataNode();

        Environment environment = TestEnvironment.newEnvironment(internalCluster().getDefaultSettings());
        MetaData.FORMAT.cleanupOldFiles(-1, nodeEnvironment.nodeDataPaths());

        expectThrows(() -> unsafeBootstrap(environment), ElasticsearchNodeCommand.NO_GLOBAL_METADATA_MSG);
    }

    public void testDetachNoMetaData() throws IOException {
        internalCluster().setBootstrapMasterNodeIndex(0);
        internalCluster().startNode();
        ensureStableCluster(1);
        NodeEnvironment nodeEnvironment = internalCluster().getMasterNodeInstance(NodeEnvironment.class);
        internalCluster().stopRandomDataNode();

        Environment environment = TestEnvironment.newEnvironment(internalCluster().getDefaultSettings());
        MetaData.FORMAT.cleanupOldFiles(-1, nodeEnvironment.nodeDataPaths());

        expectThrows(() -> detachCluster(environment), ElasticsearchNodeCommand.NO_GLOBAL_METADATA_MSG);
    }

    public void testBootstrapAbortedByUser() throws IOException {
        internalCluster().setBootstrapMasterNodeIndex(0);
        internalCluster().startNode();
        ensureStableCluster(1);
        internalCluster().stopRandomDataNode();

        Environment environment = TestEnvironment.newEnvironment(internalCluster().getDefaultSettings());
        expectThrows(() -> unsafeBootstrap(environment, 0, true), ElasticsearchNodeCommand.ABORTED_BY_USER_MSG);
    }

    public void testDetachAbortedByUser() throws IOException {
        internalCluster().setBootstrapMasterNodeIndex(0);
        internalCluster().startNode();
        ensureStableCluster(1);
        internalCluster().stopRandomDataNode();

        Environment environment = TestEnvironment.newEnvironment(internalCluster().getDefaultSettings());
        expectThrows(() -> detachCluster(environment, 0, true), ElasticsearchNodeCommand.ABORTED_BY_USER_MSG);
    }

    @AwaitsFix(bugUrl = "https://github.com/elastic/elasticsearch/issues/39220")
    public void test3MasterNodes2Failed() throws Exception {
        internalCluster().setBootstrapMasterNodeIndex(2);
        List<String> masterNodes = new ArrayList<>();

        logger.info("--> start 1st master-eligible node");
        masterNodes.add(internalCluster().startMasterOnlyNode(Settings.builder()
                .put(DiscoverySettings.INITIAL_STATE_TIMEOUT_SETTING.getKey(), "0s")
                .build())); // node ordinal 0

        logger.info("--> start one data-only node");
        String dataNode = internalCluster().startDataOnlyNode(Settings.builder()
                .put(DiscoverySettings.INITIAL_STATE_TIMEOUT_SETTING.getKey(), "0s")
                .build()); // node ordinal 1

        logger.info("--> start 2nd and 3rd master-eligible nodes and bootstrap");
        masterNodes.addAll(internalCluster().startMasterOnlyNodes(2)); // node ordinals 2 and 3

        logger.info("--> create index test");
        createIndex("test");

        logger.info("--> stop 2nd and 3d master eligible node");
        internalCluster().stopRandomNode(InternalTestCluster.nameFilter(masterNodes.get(1)));
        internalCluster().stopRandomNode(InternalTestCluster.nameFilter(masterNodes.get(2)));

        logger.info("--> ensure NO_MASTER_BLOCK on data-only node");
        assertBusy(() -> {
            ClusterState state = internalCluster().client(dataNode).admin().cluster().prepareState().setLocal(true)
                    .execute().actionGet().getState();
            assertTrue(state.blocks().hasGlobalBlockWithId(NoMasterBlockService.NO_MASTER_BLOCK_ID));
        });

        logger.info("--> try to unsafely bootstrap 1st master-eligible node, while node lock is held");
        final Environment environment = TestEnvironment.newEnvironment(internalCluster().getDefaultSettings());
        expectThrows(() -> unsafeBootstrap(environment), UnsafeBootstrapMasterCommand.FAILED_TO_OBTAIN_NODE_LOCK_MSG);

        logger.info("--> stop 1st master-eligible node and data-only node");
        NodeEnvironment nodeEnvironment = internalCluster().getMasterNodeInstance(NodeEnvironment.class);
        internalCluster().stopRandomNode(InternalTestCluster.nameFilter(masterNodes.get(0)));
        internalCluster().stopRandomDataNode();

        logger.info("--> unsafely-bootstrap 1st master-eligible node");
        MockTerminal terminal = unsafeBootstrap(environment);
        MetaData metaData = MetaData.FORMAT.loadLatestState(logger, xContentRegistry(), nodeEnvironment.nodeDataPaths());
        assertThat(terminal.getOutput(), containsString(
                String.format(Locale.ROOT, UnsafeBootstrapMasterCommand.CLUSTER_STATE_TERM_VERSION_MSG_FORMAT,
                        metaData.coordinationMetaData().term(), metaData.version())));

        logger.info("--> start 1st master-eligible node");
        internalCluster().startMasterOnlyNode();

        logger.info("--> detach-cluster on data-only node");
        detachCluster(environment, 1, false);

        logger.info("--> start data-only node");
        String dataNode2 = internalCluster().startDataOnlyNode();

        logger.info("--> ensure there is no NO_MASTER_BLOCK and unsafe-bootstrap is reflected in cluster state");
        assertBusy(() -> {
            ClusterState state = internalCluster().client(dataNode2).admin().cluster().prepareState().setLocal(true)
                    .execute().actionGet().getState();
            assertFalse(state.blocks().hasGlobalBlockWithId(NoMasterBlockService.NO_MASTER_BLOCK_ID));
            assertTrue(state.metaData().persistentSettings().getAsBoolean(UnsafeBootstrapMasterCommand.UNSAFE_BOOTSTRAP.getKey(), false));
        });

        logger.info("--> ensure index test is green");
        ensureGreen("test");

        logger.info("--> detach-cluster on 2nd and 3rd master-eligible nodes");
        detachCluster(environment, 2, false);
        detachCluster(environment, 3, false);

        logger.info("--> start 2nd and 3rd master-eligible nodes and ensure 4 nodes stable cluster");
        internalCluster().startMasterOnlyNodes(2);
        ensureStableCluster(4);
    }

    public void testAllMasterEligibleNodesFailedDanglingIndexImport() throws Exception {
        internalCluster().setBootstrapMasterNodeIndex(0);

        logger.info("--> start mixed data and master-eligible node and bootstrap cluster");
        String masterNode = internalCluster().startNode(); // node ordinal 0

        logger.info("--> start data-only node and ensure 2 nodes stable cluster");
        String dataNode = internalCluster().startDataOnlyNode(); // node ordinal 1
        ensureStableCluster(2);

        logger.info("--> index 1 doc and ensure index is green");
        client().prepareIndex("test", "type1", "1").setSource("field1", "value1").setRefreshPolicy(IMMEDIATE).get();
        ensureGreen("test");

        logger.info("--> verify 1 doc in the index");
        assertHitCount(client().prepareSearch().setQuery(matchAllQuery()).get(), 1L);
        assertThat(client().prepareGet("test", "type1", "1").execute().actionGet().isExists(), equalTo(true));

        logger.info("--> stop data-only node and detach it from the old cluster");
        internalCluster().stopRandomNode(InternalTestCluster.nameFilter(dataNode));
        final Environment environment = TestEnvironment.newEnvironment(internalCluster().getDefaultSettings());
        detachCluster(environment, 1, false);

        logger.info("--> stop master-eligible node, clear its data and start it again - new cluster should form");
        internalCluster().restartNode(masterNode, new InternalTestCluster.RestartCallback(){
            @Override
            public boolean clearData(String nodeName) {
                return true;
            }
        });

        logger.info("--> start data-only only node and ensure 2 nodes stable cluster");
        internalCluster().startDataOnlyNode();
        ensureStableCluster(2);

        logger.info("--> verify that the dangling index exists and has green status");
        assertBusy(() -> {
            assertThat(client().admin().indices().prepareExists("test").execute().actionGet().isExists(), equalTo(true));
        });
        ensureGreen("test");

        logger.info("--> verify the doc is there");
        assertThat(client().prepareGet("test", "type1", "1").execute().actionGet().isExists(), equalTo(true));
    }

    public void testNoInitialBootstrapAfterDetach() throws Exception {
        internalCluster().setBootstrapMasterNodeIndex(0);
        internalCluster().startMasterOnlyNode();
        internalCluster().stopCurrentMasterNode();

        final Environment environment = TestEnvironment.newEnvironment(internalCluster().getDefaultSettings());
        detachCluster(environment);

        String node = internalCluster().startMasterOnlyNode(Settings.builder()
                // give the cluster 2 seconds to elect the master (it should not)
                .put(DiscoverySettings.INITIAL_STATE_TIMEOUT_SETTING.getKey(), "2s")
                .build());

        ClusterState state = internalCluster().client().admin().cluster().prepareState().setLocal(true)
                .execute().actionGet().getState();
        assertTrue(state.blocks().hasGlobalBlockWithId(NoMasterBlockService.NO_MASTER_BLOCK_ID));

        internalCluster().stopRandomNode(InternalTestCluster.nameFilter(node));
    }

    public void testCanRunUnsafeBootstrapAfterErroneousDetachWithoutLoosingMetaData() throws Exception {
        internalCluster().setBootstrapMasterNodeIndex(0);
        internalCluster().startMasterOnlyNode();
        ClusterUpdateSettingsRequest req = new ClusterUpdateSettingsRequest().persistentSettings(
                Settings.builder().put(INDICES_RECOVERY_MAX_BYTES_PER_SEC_SETTING.getKey(), "1234kb"));
        internalCluster().client().admin().cluster().updateSettings(req).get();

        ClusterState state = internalCluster().client().admin().cluster().prepareState().execute().actionGet().getState();
        assertThat(state.metaData().persistentSettings().get(INDICES_RECOVERY_MAX_BYTES_PER_SEC_SETTING.getKey()),
                equalTo("1234kb"));

        internalCluster().stopCurrentMasterNode();

        final Environment environment = TestEnvironment.newEnvironment(internalCluster().getDefaultSettings());
        detachCluster(environment);
        unsafeBootstrap(environment);

        internalCluster().startMasterOnlyNode();
        ensureGreen();

        state = internalCluster().client().admin().cluster().prepareState().execute().actionGet().getState();
        assertThat(state.metaData().settings().get(INDICES_RECOVERY_MAX_BYTES_PER_SEC_SETTING.getKey()),
                equalTo("1234kb"));
    }
}
