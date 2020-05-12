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

package org.elasticsearch.cluster.metadata;

import org.elasticsearch.ExceptionsHelper;
import org.elasticsearch.ResourceAlreadyExistsException;
import org.elasticsearch.Version;
import org.elasticsearch.action.admin.indices.alias.Alias;
import org.elasticsearch.action.admin.indices.create.CreateIndexClusterStateUpdateRequest;
import org.elasticsearch.action.admin.indices.shrink.ResizeType;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ESAllocationTestCase;
import org.elasticsearch.cluster.EmptyClusterInfoService;
import org.elasticsearch.cluster.block.ClusterBlocks;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodeRole;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.routing.RoutingTable;
import org.elasticsearch.cluster.routing.allocation.AllocationService;
import org.elasticsearch.cluster.routing.allocation.allocator.BalancedShardsAllocator;
import org.elasticsearch.cluster.routing.allocation.decider.AllocationDeciders;
import org.elasticsearch.cluster.routing.allocation.decider.MaxRetryAllocationDecider;
import org.elasticsearch.cluster.shards.ShardCounts;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.ValidationException;
import org.elasticsearch.common.collect.ImmutableOpenMap;
import org.elasticsearch.common.compress.CompressedXContent;
import org.elasticsearch.common.settings.IndexScopedSettings;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.index.Index;
import org.elasticsearch.index.IndexNotFoundException;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.index.query.QueryShardContext;
import org.elasticsearch.indices.InvalidAliasNameException;
import org.elasticsearch.indices.InvalidIndexNameException;
import org.elasticsearch.indices.SystemIndexDescriptor;
import org.elasticsearch.test.ClusterServiceUtils;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.VersionUtils;
import org.elasticsearch.test.gateway.TestGatewayAllocator;
import org.elasticsearch.threadpool.TestThreadPool;
import org.elasticsearch.threadpool.ThreadPool;
import org.hamcrest.Matchers;
import org.junit.Before;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.BiConsumer;
import java.util.function.BiFunction;
import java.util.function.Consumer;
import java.util.stream.Collectors;
import java.util.stream.Stream;

import static java.util.Collections.emptyMap;
import static org.elasticsearch.cluster.metadata.IndexMetadata.INDEX_NUMBER_OF_ROUTING_SHARDS_SETTING;
import static org.elasticsearch.cluster.metadata.IndexMetadata.INDEX_NUMBER_OF_SHARDS_SETTING;
import static org.elasticsearch.cluster.metadata.IndexMetadata.INDEX_READ_ONLY_BLOCK;
import static org.elasticsearch.cluster.metadata.IndexMetadata.SETTING_INDEX_VERSION_CREATED;
import static org.elasticsearch.cluster.metadata.IndexMetadata.SETTING_NUMBER_OF_REPLICAS;
import static org.elasticsearch.cluster.metadata.IndexMetadata.SETTING_NUMBER_OF_SHARDS;
import static org.elasticsearch.cluster.metadata.IndexMetadata.SETTING_READ_ONLY;
import static org.elasticsearch.cluster.metadata.IndexMetadata.SETTING_VERSION_CREATED;
import static org.elasticsearch.cluster.metadata.MetadataCreateIndexService.aggregateIndexSettings;
import static org.elasticsearch.cluster.metadata.MetadataCreateIndexService.buildIndexMetadata;
import static org.elasticsearch.cluster.metadata.MetadataCreateIndexService.clusterStateCreateIndex;
import static org.elasticsearch.cluster.metadata.MetadataCreateIndexService.getIndexNumberOfRoutingShards;
import static org.elasticsearch.cluster.metadata.MetadataCreateIndexService.parseV1Mappings;
import static org.elasticsearch.cluster.metadata.MetadataCreateIndexService.resolveAndValidateAliases;
import static org.elasticsearch.cluster.shards.ShardCounts.forDataNodeCount;
import static org.elasticsearch.index.IndexSettings.INDEX_SOFT_DELETES_SETTING;
import static org.elasticsearch.indices.IndicesServiceTests.createClusterForShardLimitTest;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.endsWith;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasKey;
import static org.hamcrest.Matchers.hasToString;
import static org.hamcrest.Matchers.hasValue;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;
import static org.hamcrest.Matchers.startsWith;

public class MetadataCreateIndexServiceTests extends ESTestCase {

    private AliasValidator aliasValidator;
    private CreateIndexClusterStateUpdateRequest request;
    private QueryShardContext queryShardContext;

    @Before
    public void setupCreateIndexRequestAndAliasValidator() {
        aliasValidator = new AliasValidator();
        request = new CreateIndexClusterStateUpdateRequest("create index", "test", "test");
        Settings indexSettings = Settings.builder().put(IndexMetadata.SETTING_VERSION_CREATED, Version.CURRENT)
            .put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, 1).put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, 1).build();
        queryShardContext = new QueryShardContext(0,
            new IndexSettings(IndexMetadata.builder("test").settings(indexSettings).build(), indexSettings),
            BigArrays.NON_RECYCLING_INSTANCE, null, null, null, null, null, xContentRegistry(), writableRegistry(),
            null, null, () -> randomNonNegativeLong(), null, null, () -> true, null);
    }

    private ClusterState createClusterState(String name, int numShards, int numReplicas, Settings settings) {
        int numRoutingShards = settings.getAsInt(IndexMetadata.INDEX_NUMBER_OF_ROUTING_SHARDS_SETTING.getKey(), numShards);
        Metadata.Builder metaBuilder = Metadata.builder();
        IndexMetadata indexMetadata = IndexMetadata.builder(name).settings(settings(Version.CURRENT)
            .put(settings))
            .numberOfShards(numShards).numberOfReplicas(numReplicas)
            .setRoutingNumShards(numRoutingShards).build();
        metaBuilder.put(indexMetadata, false);
        Metadata metadata = metaBuilder.build();
        RoutingTable.Builder routingTableBuilder = RoutingTable.builder();
        routingTableBuilder.addAsNew(metadata.index(name));

        RoutingTable routingTable = routingTableBuilder.build();
        ClusterState clusterState = ClusterState.builder(org.elasticsearch.cluster.ClusterName.CLUSTER_NAME_SETTING
            .getDefault(Settings.EMPTY))
            .metadata(metadata).routingTable(routingTable).blocks(ClusterBlocks.builder().addBlocks(indexMetadata)).build();
        return clusterState;
    }

    public static boolean isShrinkable(int source, int target) {
        int x = source / target;
        assert source > target : source  + " <= " + target;
        return target * x == source;
    }

    public static boolean isSplitable(int source, int target) {
        int x = target / source;
        assert source < target : source  + " >= " + target;
        return source * x == target;
    }

    public void testValidateShrinkIndex() {
        int numShards = randomIntBetween(2, 42);
        ClusterState state = createClusterState("source", numShards, randomIntBetween(0, 10),
            Settings.builder().put("index.blocks.write", true).build());

        assertEquals("index [source] already exists",
            expectThrows(ResourceAlreadyExistsException.class, () ->
                MetadataCreateIndexService.validateShrinkIndex(state, "target", Collections.emptySet(), "source", Settings.EMPTY)
            ).getMessage());

        assertEquals("no such index [no_such_index]",
            expectThrows(IndexNotFoundException.class, () ->
                MetadataCreateIndexService.validateShrinkIndex(state, "no_such_index", Collections.emptySet(), "target", Settings.EMPTY)
            ).getMessage());

        Settings targetSettings = Settings.builder().put("index.number_of_shards", 1).build();
        assertEquals("can't shrink an index with only one shard",
            expectThrows(IllegalArgumentException.class, () -> MetadataCreateIndexService.validateShrinkIndex(createClusterState("source",
                1, 0, Settings.builder().put("index.blocks.write", true).build()), "source",
                Collections.emptySet(), "target", targetSettings)).getMessage());

        assertEquals("the number of target shards [10] must be less that the number of source shards [5]",
            expectThrows(IllegalArgumentException.class, () -> MetadataCreateIndexService.validateShrinkIndex(createClusterState("source",
                5, 0, Settings.builder().put("index.blocks.write", true).build()), "source",
                Collections.emptySet(), "target", Settings.builder().put("index.number_of_shards", 10).build())).getMessage());


        assertEquals("index source must be read-only to resize index. use \"index.blocks.write=true\"",
            expectThrows(IllegalStateException.class, () ->
                    MetadataCreateIndexService.validateShrinkIndex(
                        createClusterState("source", randomIntBetween(2, 100), randomIntBetween(0, 10), Settings.EMPTY)
                        , "source", Collections.emptySet(), "target", targetSettings)
            ).getMessage());

        assertEquals("index source must have all shards allocated on the same node to shrink index",
            expectThrows(IllegalStateException.class, () ->
                MetadataCreateIndexService.validateShrinkIndex(state, "source", Collections.emptySet(), "target", targetSettings)

            ).getMessage());
        assertEquals("the number of source shards [8] must be a multiple of [3]",
            expectThrows(IllegalArgumentException.class, () ->
                    MetadataCreateIndexService.validateShrinkIndex(createClusterState("source", 8, randomIntBetween(0, 10),
                        Settings.builder().put("index.blocks.write", true).build()), "source", Collections.emptySet(), "target",
                        Settings.builder().put("index.number_of_shards", 3).build())
            ).getMessage());

        assertEquals("mappings are not allowed when resizing indices, all mappings are copied from the source index",
            expectThrows(IllegalArgumentException.class, () -> {
                MetadataCreateIndexService.validateShrinkIndex(state, "source", Collections.singleton("foo"),
                    "target", targetSettings);
                }
            ).getMessage());

        // create one that won't fail
        ClusterState clusterState = ClusterState.builder(createClusterState("source", numShards, 0,
            Settings.builder().put("index.blocks.write", true).build())).nodes(DiscoveryNodes.builder().add(newNode("node1")))
            .build();
        AllocationService service = new AllocationService(new AllocationDeciders(
            Collections.singleton(new MaxRetryAllocationDecider())),
            new TestGatewayAllocator(), new BalancedShardsAllocator(Settings.EMPTY), EmptyClusterInfoService.INSTANCE);

        RoutingTable routingTable = service.reroute(clusterState, "reroute").routingTable();
        clusterState = ClusterState.builder(clusterState).routingTable(routingTable).build();
        // now we start the shard
        routingTable = ESAllocationTestCase.startInitializingShardsAndReroute(service, clusterState, "source").routingTable();
        clusterState = ClusterState.builder(clusterState).routingTable(routingTable).build();
        int targetShards;
        do {
            targetShards = randomIntBetween(1, numShards/2);
        } while (isShrinkable(numShards, targetShards) == false);
        MetadataCreateIndexService.validateShrinkIndex(clusterState, "source", Collections.emptySet(), "target",
            Settings.builder().put("index.number_of_shards", targetShards).build());
    }

    public void testValidateSplitIndex() {
        int numShards = randomIntBetween(1, 42);
        Settings targetSettings = Settings.builder().put("index.number_of_shards", numShards * 2).build();
        ClusterState state = createClusterState("source", numShards, randomIntBetween(0, 10),
            Settings.builder().put("index.blocks.write", true).build());

        assertEquals("index [source] already exists",
            expectThrows(ResourceAlreadyExistsException.class, () ->
                MetadataCreateIndexService.validateSplitIndex(state, "target", Collections.emptySet(), "source", targetSettings)
            ).getMessage());

        assertEquals("no such index [no_such_index]",
            expectThrows(IndexNotFoundException.class, () ->
                MetadataCreateIndexService.validateSplitIndex(state, "no_such_index", Collections.emptySet(), "target", targetSettings)
            ).getMessage());

        assertEquals("the number of source shards [10] must be less that the number of target shards [5]",
            expectThrows(IllegalArgumentException.class, () -> MetadataCreateIndexService.validateSplitIndex(createClusterState("source",
                10, 0, Settings.builder().put("index.blocks.write", true).build()), "source", Collections.emptySet(),
                "target", Settings.builder().put("index.number_of_shards", 5).build())
            ).getMessage());


        assertEquals("index source must be read-only to resize index. use \"index.blocks.write=true\"",
            expectThrows(IllegalStateException.class, () ->
                MetadataCreateIndexService.validateSplitIndex(
                    createClusterState("source", randomIntBetween(2, 100), randomIntBetween(0, 10), Settings.EMPTY)
                    , "source", Collections.emptySet(), "target", targetSettings)
            ).getMessage());


        assertEquals("the number of source shards [3] must be a factor of [4]",
            expectThrows(IllegalArgumentException.class, () ->
                MetadataCreateIndexService.validateSplitIndex(createClusterState("source", 3, randomIntBetween(0, 10),
                    Settings.builder().put("index.blocks.write", true).build()), "source", Collections.emptySet(), "target",
                    Settings.builder().put("index.number_of_shards", 4).build())
            ).getMessage());

        assertEquals("mappings are not allowed when resizing indices, all mappings are copied from the source index",
            expectThrows(IllegalArgumentException.class, () -> {
                    MetadataCreateIndexService.validateSplitIndex(state, "source", Collections.singleton("foo"),
                        "target", targetSettings);
                }
            ).getMessage());

        int targetShards;
        do {
            targetShards = randomIntBetween(numShards+1, 100);
        } while (isSplitable(numShards, targetShards) == false);
        ClusterState clusterState = ClusterState.builder(createClusterState("source", numShards, 0,
            Settings.builder().put("index.blocks.write", true).put("index.number_of_routing_shards", targetShards).build()))
            .nodes(DiscoveryNodes.builder().add(newNode("node1"))).build();
        AllocationService service = new AllocationService(new AllocationDeciders(
            Collections.singleton(new MaxRetryAllocationDecider())),
            new TestGatewayAllocator(), new BalancedShardsAllocator(Settings.EMPTY), EmptyClusterInfoService.INSTANCE);

        RoutingTable routingTable = service.reroute(clusterState, "reroute").routingTable();
        clusterState = ClusterState.builder(clusterState).routingTable(routingTable).build();
        // now we start the shard
        routingTable = ESAllocationTestCase.startInitializingShardsAndReroute(service, clusterState, "source").routingTable();
        clusterState = ClusterState.builder(clusterState).routingTable(routingTable).build();

        MetadataCreateIndexService.validateSplitIndex(clusterState, "source", Collections.emptySet(), "target",
            Settings.builder().put("index.number_of_shards", targetShards).build());
    }

    public void testPrepareResizeIndexSettings() {
        final List<Version> versions = Arrays.asList(VersionUtils.randomVersion(random()), VersionUtils.randomVersion(random()));
        versions.sort(Comparator.comparingLong(l -> l.id));
        final Version version = versions.get(0);
        final Version upgraded = versions.get(1);
        final Settings.Builder indexSettingsBuilder =
                Settings.builder()
                        .put("index.version.created", version)
                        .put("index.version.upgraded", upgraded)
                        .put("index.similarity.default.type", "BM25")
                        .put("index.analysis.analyzer.default.tokenizer", "keyword")
                        .put("index.soft_deletes.enabled", "true");
        if (randomBoolean()) {
            indexSettingsBuilder.put("index.allocation.max_retries", randomIntBetween(1, 1000));
        }
        runPrepareResizeIndexSettingsTest(
                indexSettingsBuilder.build(),
                Settings.EMPTY,
                Collections.emptyList(),
                randomBoolean(),
                settings -> {
                    assertThat("similarity settings must be copied", settings.get("index.similarity.default.type"), equalTo("BM25"));
                    assertThat(
                            "analysis settings must be copied",
                            settings.get("index.analysis.analyzer.default.tokenizer"),
                            equalTo("keyword"));
                    assertThat(settings.get("index.routing.allocation.initial_recovery._id"), equalTo("node1"));
                    assertThat(settings.get("index.allocation.max_retries"), nullValue());
                    assertThat(settings.getAsVersion("index.version.created", null), equalTo(version));
                    assertThat(settings.getAsVersion("index.version.upgraded", null), equalTo(upgraded));
                    assertThat(settings.get("index.soft_deletes.enabled"), equalTo("true"));
                });
    }

    public void testPrepareResizeIndexSettingsCopySettings() {
        final int maxMergeCount = randomIntBetween(1, 16);
        final int maxThreadCount = randomIntBetween(1, 16);
        final Setting<String> nonCopyableExistingIndexSetting =
                Setting.simpleString("index.non_copyable.existing", Setting.Property.IndexScope, Setting.Property.NotCopyableOnResize);
        final Setting<String> nonCopyableRequestIndexSetting =
                Setting.simpleString("index.non_copyable.request", Setting.Property.IndexScope, Setting.Property.NotCopyableOnResize);
        runPrepareResizeIndexSettingsTest(
                Settings.builder()
                        .put("index.merge.scheduler.max_merge_count", maxMergeCount)
                        .put("index.non_copyable.existing", "existing")
                        .build(),
                Settings.builder()
                        .put("index.blocks.write", (String) null)
                        .put("index.merge.scheduler.max_thread_count", maxThreadCount)
                        .put("index.non_copyable.request", "request")
                        .build(),
                Arrays.asList(nonCopyableExistingIndexSetting, nonCopyableRequestIndexSetting),
                true,
                settings -> {
                    assertNull(settings.getAsBoolean("index.blocks.write", null));
                    assertThat(settings.get("index.routing.allocation.require._name"), equalTo("node1"));
                    assertThat(settings.getAsInt("index.merge.scheduler.max_merge_count", null), equalTo(maxMergeCount));
                    assertThat(settings.getAsInt("index.merge.scheduler.max_thread_count", null), equalTo(maxThreadCount));
                    assertNull(settings.get("index.non_copyable.existing"));
                    assertThat(settings.get("index.non_copyable.request"), equalTo("request"));
                });
    }

    public void testPrepareResizeIndexSettingsAnalysisSettings() {
        // analysis settings from the request are not overwritten
        runPrepareResizeIndexSettingsTest(
                Settings.EMPTY,
                Settings.builder().put("index.analysis.analyzer.default.tokenizer", "whitespace").build(),
                Collections.emptyList(),
                randomBoolean(),
                settings ->
                    assertThat(
                            "analysis settings are not overwritten",
                            settings.get("index.analysis.analyzer.default.tokenizer"),
                            equalTo("whitespace"))
                );

    }

    public void testPrepareResizeIndexSettingsSimilaritySettings() {
        // similarity settings from the request are not overwritten
        runPrepareResizeIndexSettingsTest(
                Settings.EMPTY,
                Settings.builder().put("index.similarity.sim.type", "DFR").build(),
                Collections.emptyList(),
                randomBoolean(),
                settings ->
                        assertThat("similarity settings are not overwritten", settings.get("index.similarity.sim.type"), equalTo("DFR")));

    }

    public void testDoNotOverrideSoftDeletesSettingOnResize() {
        runPrepareResizeIndexSettingsTest(
            Settings.builder().put("index.soft_deletes.enabled", "false").build(),
            Settings.builder().put("index.soft_deletes.enabled", "true").build(),
            Collections.emptyList(),
            randomBoolean(),
            settings -> assertThat(settings.get("index.soft_deletes.enabled"), equalTo("true")));
    }

    private void runPrepareResizeIndexSettingsTest(
            final Settings sourceSettings,
            final Settings requestSettings,
            final Collection<Setting<?>> additionalIndexScopedSettings,
            final boolean copySettings,
            final Consumer<Settings> consumer) {
        final String indexName = randomAlphaOfLength(10);

        final Settings indexSettings = Settings.builder()
                .put("index.blocks.write", true)
                .put("index.routing.allocation.require._name", "node1")
                .put(sourceSettings)
                .build();

        final ClusterState initialClusterState =
                ClusterState
                        .builder(createClusterState(indexName, randomIntBetween(2, 10), 0, indexSettings))
                        .nodes(DiscoveryNodes.builder().add(newNode("node1")))
                        .build();

        final AllocationService service = new AllocationService(
                new AllocationDeciders(Collections.singleton(new MaxRetryAllocationDecider())),
                new TestGatewayAllocator(),
                new BalancedShardsAllocator(Settings.EMPTY),
                EmptyClusterInfoService.INSTANCE);

        final RoutingTable initialRoutingTable = service.reroute(initialClusterState, "reroute").routingTable();
        final ClusterState routingTableClusterState = ClusterState.builder(initialClusterState).routingTable(initialRoutingTable).build();

        // now we start the shard
        final RoutingTable routingTable
            = ESAllocationTestCase.startInitializingShardsAndReroute(service, routingTableClusterState, indexName).routingTable();
        final ClusterState clusterState = ClusterState.builder(routingTableClusterState).routingTable(routingTable).build();

        final Settings.Builder indexSettingsBuilder = Settings.builder().put("index.number_of_shards", 1).put(requestSettings);
        final Set<Setting<?>> settingsSet =
                Stream.concat(
                        IndexScopedSettings.BUILT_IN_INDEX_SETTINGS.stream(),
                        additionalIndexScopedSettings.stream())
                        .collect(Collectors.toSet());
        MetadataCreateIndexService.prepareResizeIndexSettings(
                clusterState,
                Collections.emptySet(),
                indexSettingsBuilder,
                clusterState.metadata().index(indexName).getIndex(),
                "target",
                ResizeType.SHRINK,
                copySettings,
                new IndexScopedSettings(Settings.EMPTY, settingsSet));
        consumer.accept(indexSettingsBuilder.build());
    }

    private DiscoveryNode newNode(String nodeId) {
        return new DiscoveryNode(
                nodeId,
                buildNewFakeTransportAddress(),
                emptyMap(),
                Set.of(DiscoveryNodeRole.MASTER_ROLE, DiscoveryNodeRole.DATA_ROLE), Version.CURRENT);
    }

    public void testValidateIndexName() throws Exception {
        ThreadPool testThreadPool = new TestThreadPool(getTestName());
        try {
            MetadataCreateIndexService checkerService = new MetadataCreateIndexService(
                Settings.EMPTY,
                ClusterServiceUtils.createClusterService(testThreadPool),
                null,
                null,
                null,
                null,
                null,
                testThreadPool,
                null,
                Collections.emptyList(),
                false
            );
            validateIndexName(checkerService, "index?name", "must not contain the following characters " + Strings.INVALID_FILENAME_CHARS);

            validateIndexName(checkerService, "index#name", "must not contain '#'");

            validateIndexName(checkerService, "_indexname", "must not start with '_', '-', or '+'");
            validateIndexName(checkerService, "-indexname", "must not start with '_', '-', or '+'");
            validateIndexName(checkerService, "+indexname", "must not start with '_', '-', or '+'");

            validateIndexName(checkerService, "INDEXNAME", "must be lowercase");

            validateIndexName(checkerService, "..", "must not be '.' or '..'");

            validateIndexName(checkerService, "foo:bar", "must not contain ':'");
        } finally {
            testThreadPool.shutdown();
        }
    }

    private void validateIndexName(MetadataCreateIndexService metadataCreateIndexService, String indexName, String errorMessage) {
        InvalidIndexNameException e = expectThrows(InvalidIndexNameException.class,
            () -> metadataCreateIndexService.validateIndexName(indexName, ClusterState.builder(ClusterName.CLUSTER_NAME_SETTING
                .getDefault(Settings.EMPTY)).build()));
        assertThat(e.getMessage(), endsWith(errorMessage));
    }

    public void testCalculateNumRoutingShards() {
        assertEquals(1024, MetadataCreateIndexService.calculateNumRoutingShards(1, Version.CURRENT));
        assertEquals(1024, MetadataCreateIndexService.calculateNumRoutingShards(2, Version.CURRENT));
        assertEquals(768, MetadataCreateIndexService.calculateNumRoutingShards(3, Version.CURRENT));
        assertEquals(576, MetadataCreateIndexService.calculateNumRoutingShards(9, Version.CURRENT));
        assertEquals(1024, MetadataCreateIndexService.calculateNumRoutingShards(512, Version.CURRENT));
        assertEquals(2048, MetadataCreateIndexService.calculateNumRoutingShards(1024, Version.CURRENT));
        assertEquals(4096, MetadataCreateIndexService.calculateNumRoutingShards(2048, Version.CURRENT));

        for (int i = 0; i < 1000; i++) {
            int randomNumShards = randomIntBetween(1, 10000);
            int numRoutingShards = MetadataCreateIndexService.calculateNumRoutingShards(randomNumShards, Version.CURRENT);
            if (numRoutingShards <= 1024) {
                assertTrue("numShards: " + randomNumShards, randomNumShards < 513);
                assertTrue("numRoutingShards: " + numRoutingShards, numRoutingShards > 512);
            } else {
                assertEquals("numShards: " + randomNumShards, numRoutingShards / 2, randomNumShards);
            }

            double ratio = numRoutingShards / randomNumShards;
            int intRatio = (int) ratio;
            assertEquals(ratio, intRatio, 0.0d);
            assertTrue(1 < ratio);
            assertTrue(ratio <= 1024);
            assertEquals(0, intRatio % 2);
            assertEquals("ratio is not a power of two", intRatio, Integer.highestOneBit(intRatio));
        }
    }

    public void testShardLimit() {
        int nodesInCluster = randomIntBetween(2,90);
        ShardCounts counts = forDataNodeCount(nodesInCluster);
        Settings clusterSettings = Settings.builder()
            .put(Metadata.SETTING_CLUSTER_MAX_SHARDS_PER_NODE.getKey(), counts.getShardsPerNode())
            .build();
        ClusterState state = createClusterForShardLimitTest(nodesInCluster, counts.getFirstIndexShards(), counts.getFirstIndexReplicas(),
            clusterSettings);

        Settings indexSettings = Settings.builder()
            .put(SETTING_VERSION_CREATED, Version.CURRENT)
            .put(SETTING_NUMBER_OF_SHARDS, counts.getFailingIndexShards())
            .put(SETTING_NUMBER_OF_REPLICAS, counts.getFailingIndexReplicas())
            .build();

        final ValidationException e = expectThrows(
            ValidationException.class,
            () -> MetadataCreateIndexService.checkShardLimit(indexSettings, state));
        int totalShards = counts.getFailingIndexShards() * (1 + counts.getFailingIndexReplicas());
        int currentShards = counts.getFirstIndexShards() * (1 + counts.getFirstIndexReplicas());
        int maxShards = counts.getShardsPerNode() * nodesInCluster;
        final String expectedMessage = String.format(
            Locale.ROOT,
            "this action would add [%d] total shards, but this cluster currently has [%d]/[%d] maximum shards open",
            totalShards,
            currentShards,
            maxShards);
        assertThat(e, hasToString(containsString(expectedMessage)));
    }

    public void testValidateDotIndex() {
        List<SystemIndexDescriptor> systemIndexDescriptors = new ArrayList<>();
        systemIndexDescriptors.add(new SystemIndexDescriptor(".test", "test"));
        systemIndexDescriptors.add(new SystemIndexDescriptor(".test3", "test"));
        systemIndexDescriptors.add(new SystemIndexDescriptor(".pattern-test*", "test-1"));
        systemIndexDescriptors.add(new SystemIndexDescriptor(".pattern-test-overlapping", "test-2"));

        ThreadPool testThreadPool = new TestThreadPool(getTestName());
        try {
            MetadataCreateIndexService checkerService = new MetadataCreateIndexService(
                Settings.EMPTY,
                ClusterServiceUtils.createClusterService(testThreadPool),
                null,
                null,
                null,
                null,
                null,
                testThreadPool,
                null,
                systemIndexDescriptors,
                false
            );
            // Check deprecations
            checkerService.validateDotIndex(".test2", ClusterState.EMPTY_STATE, false);
            assertWarnings("index name [.test2] starts with a dot '.', in the next major version, index " +
                "names starting with a dot are reserved for hidden indices and system indices");

            // Check non-system hidden indices don't trigger a warning
            checkerService.validateDotIndex(".test2", ClusterState.EMPTY_STATE, true);

            // Check NO deprecation warnings if we give the index name
            checkerService.validateDotIndex(".test", ClusterState.EMPTY_STATE, false);
            checkerService.validateDotIndex(".test3", ClusterState.EMPTY_STATE, false);

            // Check that patterns with wildcards work
            checkerService.validateDotIndex(".pattern-test", ClusterState.EMPTY_STATE, false);
            checkerService.validateDotIndex(".pattern-test-with-suffix", ClusterState.EMPTY_STATE, false);
            checkerService.validateDotIndex(".pattern-test-other-suffix", ClusterState.EMPTY_STATE, false);

            // Check that an exception is thrown if more than one descriptor matches the index name
            AssertionError exception = expectThrows(AssertionError.class,
                () -> checkerService.validateDotIndex(".pattern-test-overlapping", ClusterState.EMPTY_STATE, false));
            assertThat(exception.getMessage(),
                containsString("index name [.pattern-test-overlapping] is claimed as a system index by multiple system index patterns:"));
            assertThat(exception.getMessage(), containsString("pattern: [.pattern-test*], description: [test-1]"));
            assertThat(exception.getMessage(), containsString("pattern: [.pattern-test-overlapping], description: [test-2]"));

        } finally {
            testThreadPool.shutdown();
        }
    }

    public void testParseMappingsAppliesDataFromTemplateAndRequest() throws Exception {
        IndexTemplateMetadata templateMetadata = addMatchingTemplate(templateBuilder -> {
            templateBuilder.putAlias(AliasMetadata.builder("alias1"));
            templateBuilder.putMapping("_doc", createMapping("mapping_from_template", "text"));
        });
        request.mappings(createMapping("mapping_from_request", "text").string());

        Map<String, Object> parsedMappings = MetadataCreateIndexService.parseV1Mappings(request.mappings(),
            List.of(templateMetadata.getMappings()), NamedXContentRegistry.EMPTY);

        assertThat(parsedMappings, hasKey("_doc"));
        Map<String, Object> doc = (Map<String, Object>) parsedMappings.get("_doc");
        assertThat(doc, hasKey("properties"));
        Map<String, Object> mappingsProperties = (Map<String, Object>) doc.get("properties");
        assertThat(mappingsProperties, hasKey("mapping_from_request"));
        assertThat(mappingsProperties, hasKey("mapping_from_template"));
    }

    public void testAggregateSettingsAppliesSettingsFromTemplatesAndRequest() {
        IndexTemplateMetadata templateMetadata = addMatchingTemplate(builder -> {
            builder.settings(Settings.builder().put("template_setting", "value1"));
        });
        ImmutableOpenMap.Builder<String, IndexTemplateMetadata> templatesBuilder = ImmutableOpenMap.builder();
        templatesBuilder.put("template_1", templateMetadata);
        Metadata metadata = new Metadata.Builder().templates(templatesBuilder.build()).build();
        ClusterState clusterState = ClusterState.builder(org.elasticsearch.cluster.ClusterName.CLUSTER_NAME_SETTING
            .getDefault(Settings.EMPTY))
            .metadata(metadata)
            .build();
        request.settings(Settings.builder().put("request_setting", "value2").build());

        Settings aggregatedIndexSettings = aggregateIndexSettings(clusterState, request, templateMetadata.settings(), Map.of(),
            null, Settings.EMPTY, IndexScopedSettings.DEFAULT_SCOPED_SETTINGS);

        assertThat(aggregatedIndexSettings.get("template_setting"), equalTo("value1"));
        assertThat(aggregatedIndexSettings.get("request_setting"), equalTo("value2"));
    }

    public void testInvalidAliasName() {
        final String[] invalidAliasNames = new String[] { "-alias1", "+alias2", "_alias3", "a#lias", "al:ias", ".", ".." };
        String aliasName = randomFrom(invalidAliasNames);
        request.aliases(Set.of(new Alias(aliasName)));

        expectThrows(InvalidAliasNameException.class, () ->
            resolveAndValidateAliases(request.index(), request.aliases(), List.of(), Metadata.builder().build(),
                aliasValidator, xContentRegistry(), queryShardContext)
        );
    }

    public void testRequestDataHavePriorityOverTemplateData() throws Exception {
        CompressedXContent templateMapping = createMapping("test", "text");
        CompressedXContent reqMapping = createMapping("test", "keyword");

        IndexTemplateMetadata templateMetadata = addMatchingTemplate(builder -> builder
            .putAlias(AliasMetadata.builder("alias").searchRouting("fromTemplate").build())
            .putMapping("_doc", templateMapping)
            .settings(Settings.builder().put("key1", "templateValue"))
        );

        request.mappings(reqMapping.string());
        request.aliases(Set.of(new Alias("alias").searchRouting("fromRequest")));
        request.settings(Settings.builder().put("key1", "requestValue").build());

        Map<String, Object> parsedMappings = MetadataCreateIndexService.parseV1Mappings(request.mappings(),
            List.of(templateMetadata.mappings()), xContentRegistry());
        List<AliasMetadata> resolvedAliases = resolveAndValidateAliases(request.index(), request.aliases(),
            MetadataIndexTemplateService.resolveAliases(List.of(templateMetadata)),
            Metadata.builder().build(), aliasValidator, xContentRegistry(), queryShardContext);
        Settings aggregatedIndexSettings = aggregateIndexSettings(ClusterState.EMPTY_STATE, request, templateMetadata.settings(), Map.of(),
            null, Settings.EMPTY, IndexScopedSettings.DEFAULT_SCOPED_SETTINGS);

        assertThat(resolvedAliases.get(0).getSearchRouting(), equalTo("fromRequest"));
        assertThat(aggregatedIndexSettings.get("key1"), equalTo("requestValue"));
        assertThat(parsedMappings, hasKey("_doc"));
        Map<String, Object> doc = (Map<String, Object>) parsedMappings.get("_doc");
        assertThat(doc, hasKey("properties"));
        Map<String, Object> mappingsProperties = (Map<String, Object>) doc.get("properties");
        assertThat(mappingsProperties, hasKey("test"));
        assertThat((Map<String, Object>) mappingsProperties.get("test"), hasValue("keyword"));
    }

    public void testDefaultSettings() {
        Settings aggregatedIndexSettings = aggregateIndexSettings(ClusterState.EMPTY_STATE, request, Settings.EMPTY, Map.of(),
            null, Settings.EMPTY, IndexScopedSettings.DEFAULT_SCOPED_SETTINGS);

        assertThat(aggregatedIndexSettings.get(SETTING_NUMBER_OF_SHARDS), equalTo("1"));
    }

    public void testSettingsFromClusterState() {
        Settings aggregatedIndexSettings = aggregateIndexSettings(ClusterState.EMPTY_STATE, request, Settings.EMPTY, Map.of(),
            null, Settings.builder().put(SETTING_NUMBER_OF_SHARDS, 15).build(), IndexScopedSettings.DEFAULT_SCOPED_SETTINGS);

        assertThat(aggregatedIndexSettings.get(SETTING_NUMBER_OF_SHARDS), equalTo("15"));
    }

    public void testTemplateOrder() throws Exception {
        List<IndexTemplateMetadata> templates = new ArrayList<>(3);
        templates.add(addMatchingTemplate(builder -> builder
            .order(3)
            .settings(Settings.builder().put(SETTING_NUMBER_OF_SHARDS, 12))
            .putAlias(AliasMetadata.builder("alias1").writeIndex(true).searchRouting("3").build())
        ));
        templates.add(addMatchingTemplate(builder -> builder
            .order(2)
            .settings(Settings.builder().put(SETTING_NUMBER_OF_SHARDS, 11))
            .putAlias(AliasMetadata.builder("alias1").searchRouting("2").build())
        ));
        templates.add(addMatchingTemplate(builder -> builder
            .order(1)
            .settings(Settings.builder().put(SETTING_NUMBER_OF_SHARDS, 10))
            .putAlias(AliasMetadata.builder("alias1").searchRouting("1").build())
        ));
        Settings aggregatedIndexSettings = aggregateIndexSettings(ClusterState.EMPTY_STATE, request,
            MetadataIndexTemplateService.resolveSettings(templates), Map.of(),
            null, Settings.EMPTY, IndexScopedSettings.DEFAULT_SCOPED_SETTINGS);
        List<AliasMetadata> resolvedAliases = resolveAndValidateAliases(request.index(), request.aliases(),
            MetadataIndexTemplateService.resolveAliases(templates),
            Metadata.builder().build(), aliasValidator, xContentRegistry(), queryShardContext);
        assertThat(aggregatedIndexSettings.get(SETTING_NUMBER_OF_SHARDS), equalTo("12"));
        AliasMetadata alias = resolvedAliases.get(0);
        assertThat(alias.getSearchRouting(), equalTo("3"));
        assertThat(alias.writeIndex(), is(true));
    }

    public void testAggregateIndexSettingsIgnoresTemplatesOnCreateFromSourceIndex() throws Exception {
        CompressedXContent templateMapping = createMapping("test", "text");

        IndexTemplateMetadata templateMetadata = addMatchingTemplate(builder -> builder
            .putAlias(AliasMetadata.builder("alias").searchRouting("fromTemplate").build())
            .putMapping("_doc", templateMapping)
            .settings(Settings.builder().put("templateSetting", "templateValue"))
        );

        request.settings(Settings.builder().put("requestSetting", "requestValue").build());
        request.resizeType(ResizeType.SPLIT);
        request.recoverFrom(new Index("sourceIndex", UUID.randomUUID().toString()));
        ClusterState clusterState =
            createClusterState("sourceIndex", 1, 0,
                Settings.builder().put("index.blocks.write", true).build());

        Settings aggregatedIndexSettings = aggregateIndexSettings(clusterState, request, templateMetadata.settings(), Map.of(),
            clusterState.metadata().index("sourceIndex"), Settings.EMPTY, IndexScopedSettings.DEFAULT_SCOPED_SETTINGS);

        assertThat(aggregatedIndexSettings.get("templateSetting"), is(nullValue()));
        assertThat(aggregatedIndexSettings.get("requestSetting"), is("requestValue"));
    }

    public void testClusterStateCreateIndexThrowsWriteIndexValidationException() throws Exception {
        IndexMetadata existingWriteIndex = IndexMetadata.builder("test2")
            .settings(settings(Version.CURRENT)).putAlias(AliasMetadata.builder("alias1").writeIndex(true).build())
            .numberOfShards(1).numberOfReplicas(0).build();
        ClusterState currentClusterState =
            ClusterState.builder(ClusterState.EMPTY_STATE).metadata(Metadata.builder().put(existingWriteIndex, false).build()).build();

        IndexMetadata newIndex = IndexMetadata.builder("test")
            .settings(settings(Version.CURRENT))
            .numberOfShards(1)
            .numberOfReplicas(0)
            .putAlias(AliasMetadata.builder("alias1").writeIndex(true).build())
            .build();

        assertThat(
            expectThrows(IllegalStateException.class,
                () -> clusterStateCreateIndex(currentClusterState, Set.of(), newIndex, (state, reason) -> state, null)).getMessage(),
            startsWith("alias [alias1] has more than one write index [")
        );
    }

    public void testClusterStateCreateIndex() {
        ClusterState currentClusterState =
            ClusterState.builder(ClusterState.EMPTY_STATE).build();

        IndexMetadata newIndexMetadata = IndexMetadata.builder("test")
            .settings(settings(Version.CURRENT).put(SETTING_READ_ONLY, true))
            .numberOfShards(1)
            .numberOfReplicas(0)
            .putAlias(AliasMetadata.builder("alias1").writeIndex(true).build())
            .build();

        // used as a value container, not for the concurrency and visibility guarantees
        AtomicBoolean allocationRerouted = new AtomicBoolean(false);
        BiFunction<ClusterState, String, ClusterState> rerouteRoutingTable = (clusterState, reason) -> {
            allocationRerouted.compareAndSet(false, true);
            return clusterState;
        };

        ClusterState updatedClusterState = clusterStateCreateIndex(currentClusterState, Set.of(INDEX_READ_ONLY_BLOCK), newIndexMetadata,
            rerouteRoutingTable, null);
        assertThat(updatedClusterState.blocks().getIndexBlockWithId("test", INDEX_READ_ONLY_BLOCK.id()), is(INDEX_READ_ONLY_BLOCK));
        assertThat(updatedClusterState.routingTable().index("test"), is(notNullValue()));
        assertThat(allocationRerouted.get(), is(true));
    }

    public void testClusterStateCreateIndexWithMetadataTransaction() {
        ClusterState currentClusterState = ClusterState.builder(ClusterState.EMPTY_STATE)
            .metadata(Metadata.builder()
                .put(IndexMetadata.builder("my-index")
                    .settings(settings(Version.CURRENT).put(SETTING_READ_ONLY, true))
                    .numberOfShards(1)
                    .numberOfReplicas(0)))
            .build();

        IndexMetadata newIndexMetadata = IndexMetadata.builder("test")
            .settings(settings(Version.CURRENT).put(SETTING_READ_ONLY, true))
            .numberOfShards(1)
            .numberOfReplicas(0)
            .putAlias(AliasMetadata.builder("alias1").writeIndex(true).build())
            .build();

        // adds alias from new index to existing index
        BiConsumer<Metadata.Builder, IndexMetadata> metadataTransformer = (builder, indexMetadata) -> {
            AliasMetadata newAlias = indexMetadata.getAliases().iterator().next().value;
            IndexMetadata myIndex = builder.get("my-index");
            builder.put(IndexMetadata.builder(myIndex).putAlias(AliasMetadata.builder(newAlias.getAlias()).build()));
        };

        ClusterState updatedClusterState = clusterStateCreateIndex(currentClusterState, Set.of(INDEX_READ_ONLY_BLOCK), newIndexMetadata,
            (clusterState, y) -> clusterState, metadataTransformer);
        assertTrue(updatedClusterState.metadata().findAllAliases(new String[]{"my-index"}).containsKey("my-index"));
        assertNotNull(updatedClusterState.metadata().findAllAliases(new String[]{"my-index"}).get("my-index"));
        assertNotNull(updatedClusterState.metadata().findAllAliases(new String[]{"my-index"}).get("my-index").get(0).alias(),
            equalTo("alias1"));
    }

    public void testParseMappingsWithTypedTemplateAndTypelessIndexMapping() throws Exception {
        IndexTemplateMetadata templateMetadata = addMatchingTemplate(builder -> {
            try {
                builder.putMapping("type", "{\"type\": {}}");
            } catch (IOException e) {
                ExceptionsHelper.reThrowIfNotNull(e);
            }
        });

        Map<String, Object> mappings = parseV1Mappings("{\"_doc\":{}}", List.of(templateMetadata.mappings()), xContentRegistry());
        assertThat(mappings, Matchers.hasKey(MapperService.SINGLE_MAPPING_NAME));
    }

    public void testParseMappingsWithTypedTemplate() throws Exception {
        IndexTemplateMetadata templateMetadata = addMatchingTemplate(builder -> {
            try {
                builder.putMapping("type",
                    "{\"type\":{\"properties\":{\"field\":{\"type\":\"keyword\"}}}}");
            } catch (IOException e) {
                ExceptionsHelper.reThrowIfNotNull(e);
            }
        });
        Map<String, Object> mappings = parseV1Mappings("", List.of(templateMetadata.mappings()), xContentRegistry());
        assertThat(mappings, Matchers.hasKey(MapperService.SINGLE_MAPPING_NAME));
    }

    public void testParseMappingsWithTypelessTemplate() throws Exception {
        IndexTemplateMetadata templateMetadata = addMatchingTemplate(builder -> {
            try {
                builder.putMapping(MapperService.SINGLE_MAPPING_NAME, "{\"_doc\": {}}");
            } catch (IOException e) {
                ExceptionsHelper.reThrowIfNotNull(e);
            }
        });
        Map<String, Object> mappings = parseV1Mappings("", List.of(templateMetadata.mappings()), xContentRegistry());
        assertThat(mappings, Matchers.hasKey(MapperService.SINGLE_MAPPING_NAME));
    }

    public void testBuildIndexMetadata() {
        IndexMetadata sourceIndexMetadata = IndexMetadata.builder("parent")
            .settings(Settings.builder()
                .put("index.version.created", Version.CURRENT)
                .build())
            .numberOfShards(1)
            .numberOfReplicas(0)
            .primaryTerm(0, 3L)
            .build();

        Settings indexSettings = Settings.builder()
            .put("index.version.created", Version.CURRENT)
            .put(SETTING_NUMBER_OF_REPLICAS, 0)
            .put(SETTING_NUMBER_OF_SHARDS, 1)
            .build();
        List<AliasMetadata> aliases = List.of(AliasMetadata.builder("alias1").build());
        IndexMetadata indexMetadata = buildIndexMetadata("test", aliases, () -> null, indexSettings, 4, sourceIndexMetadata);

        assertThat(indexMetadata.getAliases().size(), is(1));
        assertThat(indexMetadata.getAliases().keys().iterator().next().value, is("alias1"));
        assertThat("The source index primary term must be used", indexMetadata.primaryTerm(0), is(3L));
    }

    public void testGetIndexNumberOfRoutingShardsWithNullSourceIndex() {
        Settings indexSettings = Settings.builder()
            .put("index.version.created", Version.CURRENT)
            .put(INDEX_NUMBER_OF_SHARDS_SETTING.getKey(), 3)
            .build();
        int targetRoutingNumberOfShards = getIndexNumberOfRoutingShards(indexSettings, null);
        assertThat("When the target routing number of shards is not specified the expected value is the configured number of shards " +
            "multiplied by 2 at most ten times (ie. 3 * 2^8)", targetRoutingNumberOfShards, is(768));
    }

    public void testGetIndexNumberOfRoutingShardsWhenExplicitlyConfigured() {
        Settings indexSettings = Settings.builder()
            .put(INDEX_NUMBER_OF_ROUTING_SHARDS_SETTING.getKey(), 9)
            .put(INDEX_NUMBER_OF_SHARDS_SETTING.getKey(), 3)
            .build();
        int targetRoutingNumberOfShards = getIndexNumberOfRoutingShards(indexSettings, null);
        assertThat(targetRoutingNumberOfShards, is(9));
    }

    public void testGetIndexNumberOfRoutingShardsYieldsSourceNumberOfShards() {
        Settings indexSettings = Settings.builder()
            .put(INDEX_NUMBER_OF_SHARDS_SETTING.getKey(), 3)
            .build();

        IndexMetadata sourceIndexMetadata = IndexMetadata.builder("parent")
            .settings(Settings.builder()
                .put("index.version.created", Version.CURRENT)
                .build())
            .numberOfShards(6)
            .numberOfReplicas(0)
            .build();

        int targetRoutingNumberOfShards = getIndexNumberOfRoutingShards(indexSettings, sourceIndexMetadata);
        assertThat(targetRoutingNumberOfShards, is(6));
    }

    public void testRejectWithSoftDeletesDisabled() {
        final IllegalArgumentException error = expectThrows(IllegalArgumentException.class, () -> {
            request = new CreateIndexClusterStateUpdateRequest("create index", "test", "test");
            request.settings(Settings.builder().put(INDEX_SOFT_DELETES_SETTING.getKey(), false).build());
            aggregateIndexSettings(ClusterState.EMPTY_STATE, request, Settings.EMPTY, Map.of(),
                null, Settings.EMPTY, IndexScopedSettings.DEFAULT_SCOPED_SETTINGS);
        });
        assertThat(error.getMessage(), equalTo("Creating indices with soft-deletes disabled is no longer supported. "
            + "Please do not specify a value for setting [index.soft_deletes.enabled]."));
    }

    public void testRejectTranslogRetentionSettings() {
        request = new CreateIndexClusterStateUpdateRequest("create index", "test", "test");
        final Settings.Builder settings = Settings.builder();
        if (randomBoolean()) {
            settings.put(IndexSettings.INDEX_TRANSLOG_RETENTION_AGE_SETTING.getKey(), TimeValue.timeValueMillis(between(1, 120)));
        } else {
            settings.put(IndexSettings.INDEX_TRANSLOG_RETENTION_SIZE_SETTING.getKey(), between(1, 128) + "mb");
        }
        if (randomBoolean()) {
            settings.put(SETTING_INDEX_VERSION_CREATED.getKey(),
                VersionUtils.randomVersionBetween(random(), Version.V_8_0_0, Version.CURRENT));
        }
        request.settings(settings.build());
        IllegalArgumentException error = expectThrows(IllegalArgumentException.class,
            () -> aggregateIndexSettings(ClusterState.EMPTY_STATE, request, Settings.EMPTY, Map.of(),
                null, Settings.EMPTY, IndexScopedSettings.DEFAULT_SCOPED_SETTINGS));
        assertThat(error.getMessage(), equalTo("Translog retention settings [index.translog.retention.age] " +
            "and [index.translog.retention.size] are no longer supported. Please do not specify values for these settings"));
    }

    public void testDeprecateTranslogRetentionSettings() {
        request = new CreateIndexClusterStateUpdateRequest("create index", "test", "test");
        final Settings.Builder settings = Settings.builder();
        if (randomBoolean()) {
            settings.put(IndexSettings.INDEX_TRANSLOG_RETENTION_AGE_SETTING.getKey(), TimeValue.timeValueMillis(between(1, 120)));
        } else {
            settings.put(IndexSettings.INDEX_TRANSLOG_RETENTION_SIZE_SETTING.getKey(), between(1, 128) + "mb");
        }
        settings.put(SETTING_INDEX_VERSION_CREATED.getKey(), VersionUtils.randomPreviousCompatibleVersion(random(), Version.V_8_0_0));
        request.settings(settings.build());
        aggregateIndexSettings(ClusterState.EMPTY_STATE, request, Settings.EMPTY, Map.of(),
            null, Settings.EMPTY, IndexScopedSettings.DEFAULT_SCOPED_SETTINGS);
        assertWarnings("Translog retention settings [index.translog.retention.age] "
            + "and [index.translog.retention.size] are deprecated and effectively ignored. They will be removed in a future version.");
    }

    @SuppressWarnings("unchecked")
    public void testMappingsMergingIsSmart() throws Exception {
        Template ctt1 = new Template(null,
            new CompressedXContent("{\"_doc\":{\"_source\":{\"enabled\": false},\"_meta\":{\"ct1\":{\"ver\": \"text\"}}," +
                "\"properties\":{\"foo\":{\"type\":\"text\",\"ignore_above\":7,\"analyzer\":\"english\"}}}}"), null);
        Template ctt2 = new Template(null,
            new CompressedXContent("{\"_doc\":{\"_meta\":{\"ct1\":{\"ver\": \"keyword\"},\"ct2\":\"potato\"}," +
                "\"properties\":{\"foo\":{\"type\":\"keyword\",\"ignore_above\":13}}}}"), null);

        ComponentTemplate ct1 = new ComponentTemplate(ctt1, null, null);
        ComponentTemplate ct2 = new ComponentTemplate(ctt2, null, null);

        boolean shouldBeText = randomBoolean();
        List<String> composedOf = shouldBeText ? Arrays.asList("ct2", "ct1") : Arrays.asList("ct1", "ct2");
        logger.info("--> the {} analyzer should win ({})", shouldBeText ? "text" : "keyword", composedOf);
        IndexTemplateV2 template = new IndexTemplateV2(Collections.singletonList("index"), null, composedOf, null, null, null, null);

        ClusterState state = ClusterState.builder(ClusterState.EMPTY_STATE)
            .metadata(Metadata.builder(Metadata.EMPTY_METADATA)
                .put("ct1", ct1)
                .put("ct2", ct2)
                .put("index-template", template)
                .build())
            .build();

        Map<String, Object> resolved =
            MetadataCreateIndexService.resolveV2Mappings("{\"_doc\":{\"_meta\":{\"ct2\":\"eggplant\"}," +
                    "\"properties\":{\"bar\":{\"type\":\"text\"}}}}", state,
                "index-template", new NamedXContentRegistry(Collections.emptyList()));

        assertThat("expected exactly one type but was: " + resolved, resolved.size(), equalTo(1));
        Map<String, Object> innerResolved = (Map<String, Object>) resolved.get(MapperService.SINGLE_MAPPING_NAME);
        assertThat("was: " + innerResolved, innerResolved.size(), equalTo(3));

        Map<String, Object> nonProperties = new HashMap<>(innerResolved);
        nonProperties.remove("properties");
        Map<String, Object> expectedNonProperties = new HashMap<>();
        expectedNonProperties.put("_source", Collections.singletonMap("enabled", false));
        Map<String, Object> meta = new HashMap<>();
        meta.put("ct2", "eggplant");
        if (shouldBeText) {
            meta.put("ct1", Collections.singletonMap("ver", "text"));
        } else {
            meta.put("ct1", Collections.singletonMap("ver", "keyword"));
        }
        expectedNonProperties.put("_meta", meta);
        assertThat(nonProperties, equalTo(expectedNonProperties));

        Map<String, Object> innerInnerResolved = (Map<String, Object>) innerResolved.get("properties");
        assertThat(innerInnerResolved.size(), equalTo(2));
        assertThat(innerInnerResolved.get("bar"), equalTo(Collections.singletonMap("type", "text")));
        Map<String, Object> fooMappings = new HashMap<>();
        if (shouldBeText) {
            fooMappings.put("type", "text");
            fooMappings.put("ignore_above", 7);
            fooMappings.put("analyzer", "english");
        } else {
            fooMappings.put("type", "keyword");
            fooMappings.put("ignore_above", 13);
        }
        assertThat(innerInnerResolved.get("foo"), equalTo(fooMappings));
    }

    @SuppressWarnings("unchecked")
    public void testMappingsMergingHandlesDots() throws Exception {
        Template ctt1 = new Template(null,
            new CompressedXContent("{\"_doc\":{\"properties\":{\"foo\":{\"properties\":{\"bar\":{\"type\": \"long\"}}}}}}"), null);
        Template ctt2 = new Template(null,
            new CompressedXContent("{\"_doc\":{\"properties\":{\"foo.bar\":{\"type\": \"text\",\"analyzer\":\"english\"}}}}"), null);

        ComponentTemplate ct1 = new ComponentTemplate(ctt1, null, null);
        ComponentTemplate ct2 = new ComponentTemplate(ctt2, null, null);

        IndexTemplateV2 template = new IndexTemplateV2(Collections.singletonList("index"), null, Arrays.asList("ct2", "ct1"),
            null, null, null, null);

        ClusterState state = ClusterState.builder(ClusterState.EMPTY_STATE)
            .metadata(Metadata.builder(Metadata.EMPTY_METADATA)
                .put("ct1", ct1)
                .put("ct2", ct2)
                .put("index-template", template)
                .build())
            .build();

        Map<String, Object> resolved =
            MetadataCreateIndexService.resolveV2Mappings("{}", state,
                "index-template", new NamedXContentRegistry(Collections.emptyList()));

        assertThat("expected exactly one type but was: " + resolved, resolved.size(), equalTo(1));
        Map<String, Object> innerResolved = (Map<String, Object>) resolved.get(MapperService.SINGLE_MAPPING_NAME);
        assertThat("was: " + innerResolved, innerResolved.size(), equalTo(1));

        Map<String, Object> innerInnerResolved = (Map<String, Object>) innerResolved.get("properties");
        assertThat(innerInnerResolved.size(), equalTo(1));
        assertThat(innerInnerResolved.get("foo"),
            equalTo(Collections.singletonMap("properties", Collections.singletonMap("bar", Collections.singletonMap("type", "long")))));
    }

    public void testMergeIgnoringDots() throws Exception {
        Map<String, Object> first = new HashMap<>();
        first.put("foo", Collections.singletonMap("type", "long"));
        Map<String, Object> second = new HashMap<>();
        second.put("foo.bar", Collections.singletonMap("type", "long"));
        Map<String, Object> results = MetadataCreateIndexService.mergeIgnoringDots(first, second);
        assertThat(results, equalTo(second));

        results = MetadataCreateIndexService.mergeIgnoringDots(second, first);
        assertThat(results, equalTo(first));

        second.clear();
        Map<String, Object> inner = new HashMap<>();
        inner.put("type", "text");
        inner.put("analyzer", "english");
        second.put("foo", inner);

        results = MetadataCreateIndexService.mergeIgnoringDots(first, second);
        assertThat(results, equalTo(second));

        first.put("baz", 3);
        second.put("egg", 7);

        results = MetadataCreateIndexService.mergeIgnoringDots(first, second);
        Map<String, Object> expected = new HashMap<>(second);
        expected.put("baz", 3);
        assertThat(results, equalTo(expected));
    }

    @SuppressWarnings("unchecked")
    public void testDedupTemplateDynamicTemplates() throws Exception {
        Template template = new Template(null,
            new CompressedXContent("{\"_doc\":{\"_source\":{\"enabled\": false}, \"dynamic_templates\": [" +
                "{\n" +
                "   \"docker.container.labels\": {\n" +
                "     \"mapping\": {\n" +
                "         \"type\": \"keyword\"\n" +
                "       },\n" +
                "       \"match_mapping_type\": \"string\",\n" +
                "       \"path_match\": \"labels.*\"\n" +
                "     }\n" +
                "   },\n" +
                "   {\n" +
                "     \"docker.container.labels\": {\n" +
                "       \"mapping\": {\n" +
                "         \"type\": \"keyword\"\n" +
                "       },\n" +
                "       \"match_mapping_type\": \"string\",\n" +
                "       \"path_match\": \"docker.container.labels.*\"\n" +
                "     }\n" +
                "}]}}"), null);

        IndexTemplateV2 indexTemplate = new IndexTemplateV2(Collections.singletonList("index"), template, null, null, null, null);

        ClusterState state = ClusterState.builder(ClusterState.EMPTY_STATE)
            .metadata(Metadata.builder(Metadata.EMPTY_METADATA)
                .put("index-template", indexTemplate)
                .build())
            .build();

        Map<String, Object> resolved =
            MetadataCreateIndexService.resolveV2Mappings("{}", state,
                "index-template", new NamedXContentRegistry(Collections.emptyList()));

        Map<String, Object> doc = (Map<String, Object>) resolved.get(MapperService.SINGLE_MAPPING_NAME);
        List<Map<String, Object>> dynamicTemplates = (List<Map<String, Object>>) doc.get("dynamic_templates");
        assertThat(dynamicTemplates.size(), is(1));
        Map<String, Object> dynamicMapping = (Map<String, Object>) dynamicTemplates.get(0).get("docker.container.labels");
        assertThat(dynamicMapping, is(notNullValue()));
        assertThat("last mapping with the same name must override previously defined mappings with the same name",
            dynamicMapping.get("path_match"), is("docker.container.labels.*"));
    }

    public void testDedupRequestDynamicTemplates() throws Exception {
        String requestMappingJson = "{\"_doc\":{\"_source\":{\"enabled\": false}, \"dynamic_templates\": [" +
            "{\n" +
            "   \"docker.container.labels\": {\n" +
            "     \"mapping\": {\n" +
            "         \"type\": \"keyword\"\n" +
            "       },\n" +
            "       \"match_mapping_type\": \"string\",\n" +
            "       \"path_match\": \"labels.*\"\n" +
            "     }\n" +
            "   },\n" +
            "   {\n" +
            "     \"docker.container.labels\": {\n" +
            "       \"mapping\": {\n" +
            "         \"type\": \"keyword\"\n" +
            "       },\n" +
            "       \"match_mapping_type\": \"string\",\n" +
            "       \"path_match\": \"source.request.*\"\n" +
            "     }\n" +
            "}]}}";

        String templateMappingJson = "{\"_doc\":{\"_source\":{\"enabled\": false}, \"dynamic_templates\": [" +
            "{\n" +
            "   \"docker.container.labels\": {\n" +
            "     \"mapping\": {\n" +
            "         \"type\": \"text\",\n" +
            "         \"copy_to\": \"text_labels\"\n" +
            "       },\n" +
            "       \"match_mapping_type\": \"string\",\n" +
            "       \"path_match\": \"source.template.*\"\n" +
            "     }\n" +
            "   }\n" +
            "]}}";
        Template template = new Template(null, new CompressedXContent(templateMappingJson), null);

        IndexTemplateV2 indexTemplate = new IndexTemplateV2(Collections.singletonList("index"), template, null, null, null, null);

        ClusterState state = ClusterState.builder(ClusterState.EMPTY_STATE)
            .metadata(Metadata.builder(Metadata.EMPTY_METADATA)
                .put("index-template", indexTemplate)
                .build())
            .build();

        Map<String, Object> resolved =
            MetadataCreateIndexService.resolveV2Mappings(requestMappingJson, state,
                "index-template", new NamedXContentRegistry(Collections.emptyList()));

        Map<String, Object> doc = (Map<String, Object>) resolved.get(MapperService.SINGLE_MAPPING_NAME);
        List<Map<String, Object>> dynamicTemplates = (List<Map<String, Object>>) doc.get("dynamic_templates");
        assertThat(dynamicTemplates.size(), is(1));
        Map<String, Object> dynamicMapping = (Map<String, Object>) dynamicTemplates.get(0).get("docker.container.labels");
        assertThat(dynamicMapping, is(notNullValue()));
        assertThat("last mapping with the same name must override previously defined mappings with the same name",
            dynamicMapping.get("path_match"), is("source.request.*"));
        Map<String, Object> mapping = (Map<String, Object>) dynamicMapping.get("mapping");
        assertThat("the dynamic template defined in the request must not be merged with the dynamic template with the " +
            "same name defined in the index template", mapping.size(), is(1));
        assertThat(mapping.get("type"), is("keyword"));
    }

    public void testMultipleComponentTemplatesDefineSameDynamicTemplate() throws Exception {
        String ct1Mapping = "{\"_doc\":{\"_source\":{\"enabled\": false}, \"dynamic_templates\": [" +
            "{\n" +
            "   \"docker.container.labels\": {\n" +
            "     \"mapping\": {\n" +
            "         \"type\": \"text\",\n" +
            "         \"copy_to\": \"text_labels\"\n" +
            "       },\n" +
            "       \"match_mapping_type\": \"string\",\n" +
            "       \"path_match\": \"source.first.ct.*\"\n" +
            "     }\n" +
            "   },\n" +
            "{\n" +
            "   \"other.labels\": {\n" +
            "     \"mapping\": {\n" +
            "         \"type\": \"keyword\"\n" +
            "       },\n" +
            "       \"match_mapping_type\": \"string\",\n" +
            "       \"path_match\": \"source.first.ct.other.labels*\"\n" +
            "     }\n" +
            "   }\n" +
            "]}}";
        String ct2Mapping = "{\"_doc\":{\"_source\":{\"enabled\": false}, \"dynamic_templates\": [" +
            "{\n" +
            "   \"docker.container.labels\": {\n" +
            "     \"mapping\": {\n" +
            "         \"type\": \"keyword\"\n" +
            "       },\n" +
            "       \"match_mapping_type\": \"string\",\n" +
            "       \"path_match\": \"source.second.ct.*\"\n" +
            "     }\n" +
            "   }\n" +
            "]}}";

        Template ctt1 = new Template(null, new CompressedXContent(ct1Mapping), null);
        Template ctt2 = new Template(null, new CompressedXContent(ct2Mapping), null);
        ComponentTemplate ct1 = new ComponentTemplate(ctt1, null, null);
        ComponentTemplate ct2 = new ComponentTemplate(ctt2, null, null);

        IndexTemplateV2 template = new IndexTemplateV2(Collections.singletonList("index"), null, Arrays.asList("ct1", "ct2"),
            null, null, null);

        ClusterState state = ClusterState.builder(ClusterState.EMPTY_STATE)
            .metadata(Metadata.builder(Metadata.EMPTY_METADATA)
                .put("ct1", ct1)
                .put("ct2", ct2)
                .put("index-template", template)
                .build())
            .build();

        Map<String, Object> resolved =
            MetadataCreateIndexService.resolveV2Mappings("{}", state,
                "index-template", new NamedXContentRegistry(Collections.emptyList()));

        Map<String, Object> doc = (Map<String, Object>) resolved.get(MapperService.SINGLE_MAPPING_NAME);
        List<Map<String, Object>> dynamicTemplates = (List<Map<String, Object>>) doc.get("dynamic_templates");
        assertThat(dynamicTemplates.size(), is(2));
        Map<String, Object> dockerLabelsDynamicTemplate = dynamicTemplates.get(0).get("docker.container.labels") != null ?
            dynamicTemplates.get(0) : dynamicTemplates.get(1);
        Map<String, Object> dynamicMapping = (Map<String, Object>) dockerLabelsDynamicTemplate.get("docker.container.labels");
        assertThat(dynamicMapping, is(notNullValue()));
        assertThat("dynamic template defined in the last defined component template must override the previously defined dynamic templates",
            dynamicMapping.get("path_match"), is("source.second.ct.*"));
        Map<String, Object> mapping = (Map<String, Object>) dynamicMapping.get("mapping");
        assertThat("the dynamic template defined in the second component template must not be merged with the dynamic template with the " +
            "same name defined in the first component template", mapping.size(), is(1));
        assertThat(mapping.get("type"), is("keyword"));
    }

    private IndexTemplateMetadata addMatchingTemplate(Consumer<IndexTemplateMetadata.Builder> configurator) {
        IndexTemplateMetadata.Builder builder = templateMetadataBuilder("template1", "te*");
        configurator.accept(builder);
        return builder.build();
    }

    private IndexTemplateMetadata.Builder templateMetadataBuilder(String name, String pattern) {
        return IndexTemplateMetadata
            .builder(name)
            .patterns(Collections.singletonList(pattern));
    }

    private CompressedXContent createMapping(String fieldName, String fieldType) {
        try {
            final String mapping = Strings.toString(XContentFactory.jsonBuilder()
                .startObject()
                .startObject("_doc")
                .startObject("properties")
                .startObject(fieldName)
                .field("type", fieldType)
                .endObject()
                .endObject()
                .endObject()
                .endObject());

            return new CompressedXContent(mapping);
        } catch (IOException e) {
            throw ExceptionsHelper.convertToRuntime(e);
        }
    }

}
