/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.action.admin.cluster.stats;

import org.elasticsearch.Version;
import org.elasticsearch.action.admin.indices.stats.CommonStats;
import org.elasticsearch.action.admin.indices.stats.CommonStatsFlags;
import org.elasticsearch.action.admin.indices.stats.ShardStats;
import org.elasticsearch.cluster.health.ClusterHealthStatus;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.routing.RecoverySource;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.cluster.routing.UnassignedInfo;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.shard.IndexShard;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.index.shard.ShardPath;
import org.elasticsearch.index.store.StoreStats;
import org.elasticsearch.test.AbstractWireSerializingTestCase;

import java.io.IOException;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.stream.Collectors;

import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.equalTo;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class VersionStatsTests extends AbstractWireSerializingTestCase<VersionStats> {

    @Override
    protected Writeable.Reader<VersionStats> instanceReader() {
        return VersionStats::new;
    }

    @Override
    protected VersionStats createTestInstance() {
        return randomInstance();
    }

    @Override
    protected VersionStats mutateInstance(VersionStats instance) throws IOException {
        return new VersionStats(instance.versionStats().stream()
            .map(svs -> {
                switch (randomIntBetween(1, 4)) {
                    case 1:
                        return new VersionStats.SingleVersionStats(Version.V_7_3_0,
                            svs.indexCount, svs.primaryShardCount, svs.totalPrimaryByteCount);
                    case 2:
                        return new VersionStats.SingleVersionStats(svs.version,
                            svs.indexCount + 1, svs.primaryShardCount, svs.totalPrimaryByteCount);
                    case 3:
                        return new VersionStats.SingleVersionStats(svs.version,
                            svs.indexCount, svs.primaryShardCount + 1, svs.totalPrimaryByteCount);
                    case 4:
                        return new VersionStats.SingleVersionStats(svs.version,
                            svs.indexCount, svs.primaryShardCount, svs.totalPrimaryByteCount + 1);
                    default:
                        throw new IllegalArgumentException("unexpected branch");
                }
            })
            .collect(Collectors.toList()));
    }

    public void testCreation() {
        Metadata metadata = Metadata.builder().build();
        VersionStats stats = VersionStats.of(metadata, Collections.emptyList());
        assertThat(stats.versionStats(), equalTo(Collections.emptySet()));


        metadata = new Metadata.Builder()
            .put(indexMeta("foo", Version.CURRENT, 4), true)
            .put(indexMeta("bar", Version.CURRENT, 3), true)
            .put(indexMeta("baz", Version.V_7_0_0, 2), true)
            .build();
        stats = VersionStats.of(metadata, Collections.emptyList());
        assertThat(stats.versionStats().size(), equalTo(2));
        VersionStats.SingleVersionStats s1 = new VersionStats.SingleVersionStats(Version.CURRENT, 2, 7, 0);
        VersionStats.SingleVersionStats s2 = new VersionStats.SingleVersionStats(Version.V_7_0_0, 1, 2, 0);
        assertThat(stats.versionStats(), containsInAnyOrder(s1, s2));

        ShardId shardId = new ShardId("bar", "uuid", 0);
        ShardRouting shardRouting = ShardRouting.newUnassigned(shardId, true,
            RecoverySource.PeerRecoverySource.INSTANCE, new UnassignedInfo(UnassignedInfo.Reason.INDEX_CREATED, "message"));
        Path path = createTempDir().resolve("indices").resolve(shardRouting.shardId().getIndex().getUUID())
            .resolve(String.valueOf(shardRouting.shardId().id()));
        IndexShard indexShard = mock(IndexShard.class);
        StoreStats storeStats = new StoreStats(100, 200);
        when(indexShard.storeStats()).thenReturn(storeStats);
        ShardStats shardStats = new ShardStats(shardRouting, new ShardPath(false, path, path, shardRouting.shardId()),
            new CommonStats(null, indexShard, new CommonStatsFlags(CommonStatsFlags.Flag.Store)),
            null, null, null);
        ClusterStatsNodeResponse nodeResponse =
            new ClusterStatsNodeResponse(new DiscoveryNode("id", buildNewFakeTransportAddress(), Version.CURRENT),
                ClusterHealthStatus.GREEN, null, null, new ShardStats[]{shardStats});

        stats = VersionStats.of(metadata, Collections.singletonList(nodeResponse));
        assertThat(stats.versionStats().size(), equalTo(2));
        s1 = new VersionStats.SingleVersionStats(Version.CURRENT, 2, 7, 100);
        s2 = new VersionStats.SingleVersionStats(Version.V_7_0_0, 1, 2, 0);
        assertThat(stats.versionStats(), containsInAnyOrder(s1, s2));
    }

    private static IndexMetadata indexMeta(String name, Version version, int primaryShards) {
        Settings settings = Settings.builder()
            .put(IndexMetadata.SETTING_VERSION_CREATED, version)
            .put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, primaryShards)
            .put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, randomIntBetween(0, 3))
            .build();
        IndexMetadata.Builder indexMetadata = new IndexMetadata.Builder(name).settings(settings);
        return indexMetadata.build();
    }

    public static VersionStats randomInstance() {
        List<Version> versions = Arrays.asList(Version.CURRENT, Version.V_7_0_0, Version.V_7_1_0, Version.V_7_2_0);
        List<VersionStats.SingleVersionStats> stats = new ArrayList<>();
        for (Version v : versions) {
            VersionStats.SingleVersionStats s =
                new VersionStats.SingleVersionStats(v, randomIntBetween(10, 20), randomIntBetween(20, 30), randomNonNegativeLong());
            stats.add(s);
        }
        return new VersionStats(stats);
    }
}
