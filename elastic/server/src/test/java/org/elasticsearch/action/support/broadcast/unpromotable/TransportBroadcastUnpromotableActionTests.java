/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.support.broadcast.unpromotable;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.ActionTestUtils;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.routing.IndexShardRoutingTable;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.cluster.routing.ShardRoutingState;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.core.IOUtils;
import org.elasticsearch.core.Tuple;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.transport.CapturingTransport;
import org.elasticsearch.threadpool.TestThreadPool;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.NodeNotConnectedException;
import org.elasticsearch.transport.TransportService;
import org.junit.After;
import org.junit.AfterClass;
import org.junit.Before;
import org.junit.BeforeClass;

import java.io.IOException;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.TimeUnit;
import java.util.stream.Collectors;
import java.util.stream.Stream;

import static org.elasticsearch.action.support.replication.ClusterStateCreationUtils.state;
import static org.elasticsearch.action.support.replication.ClusterStateCreationUtils.stateWithAssignedPrimariesAndReplicas;
import static org.elasticsearch.cluster.routing.TestShardRouting.newShardRouting;
import static org.elasticsearch.test.ClusterServiceUtils.createClusterService;
import static org.elasticsearch.test.ClusterServiceUtils.setState;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.is;

public class TransportBroadcastUnpromotableActionTests extends ESTestCase {

    private static ThreadPool THREAD_POOL;
    private ClusterService clusterService;
    private TransportService transportService;
    private CapturingTransport transport;
    private TestTransportBroadcastUnpromotableAction broadcastUnpromotableAction;

    @BeforeClass
    public static void beforeClass() {
        THREAD_POOL = new TestThreadPool(TransportBroadcastUnpromotableActionTests.class.getSimpleName());
    }

    @Override
    @Before
    public void setUp() throws Exception {
        super.setUp();
        transport = new CapturingTransport();
        clusterService = createClusterService(THREAD_POOL);
        transportService = transport.createTransportService(
            clusterService.getSettings(),
            THREAD_POOL,
            TransportService.NOOP_TRANSPORT_INTERCEPTOR,
            x -> clusterService.localNode(),
            null,
            Collections.emptySet()
        );
        transportService.start();
        transportService.acceptIncomingRequests();
        broadcastUnpromotableAction = new TestTransportBroadcastUnpromotableAction();
    }

    @Override
    @After
    public void tearDown() throws Exception {
        super.tearDown();
        IOUtils.close(clusterService, transportService);
    }

    @AfterClass
    public static void afterClass() {
        ThreadPool.terminate(THREAD_POOL, 30, TimeUnit.SECONDS);
        THREAD_POOL = null;
    }

    private class TestTransportBroadcastUnpromotableAction extends TransportBroadcastUnpromotableAction<TestBroadcastUnpromotableRequest> {

        TestTransportBroadcastUnpromotableAction() {
            super(
                "indices:admin/test",
                TransportBroadcastUnpromotableActionTests.this.clusterService,
                TransportBroadcastUnpromotableActionTests.this.transportService,
                new ActionFilters(Set.of()),
                TestBroadcastUnpromotableRequest::new,
                ThreadPool.Names.SAME
            );
        }

        @Override
        protected void unpromotableShardOperation(
            Task task,
            TestBroadcastUnpromotableRequest request,
            ActionListener<ActionResponse.Empty> listener
        ) {
            assert false : "not reachable in these tests";
        }

    }

    private static class TestBroadcastUnpromotableRequest extends BroadcastUnpromotableRequest {

        TestBroadcastUnpromotableRequest(StreamInput in) throws IOException {
            super(in);
        }

        TestBroadcastUnpromotableRequest(IndexShardRoutingTable indexShardRoutingTable) {
            super(indexShardRoutingTable);
        }

    }

    private static List<ShardRouting.Role> getReplicaRoles(int numPromotableReplicas, int numSearchReplicas) {
        List<ShardRouting.Role> replicaRoles = Stream.concat(
            Collections.nCopies(numPromotableReplicas, randomBoolean() ? ShardRouting.Role.DEFAULT : ShardRouting.Role.INDEX_ONLY).stream(),
            Collections.nCopies(numSearchReplicas, ShardRouting.Role.SEARCH_ONLY).stream()
        ).collect(Collectors.toList());
        Collections.shuffle(replicaRoles, random());
        return replicaRoles;
    }

    private static List<Tuple<ShardRoutingState, ShardRouting.Role>> getReplicaRolesWithRandomStates(
        int numPromotableReplicas,
        int numSearchReplicas,
        List<ShardRoutingState> possibleStates
    ) {
        return getReplicaRoles(numPromotableReplicas, numSearchReplicas).stream()
            .map(role -> new Tuple<>(randomFrom(possibleStates), role))
            .collect(Collectors.toList());
    }

    private static List<Tuple<ShardRoutingState, ShardRouting.Role>> getReplicaRolesWithRandomStates(
        int numPromotableReplicas,
        int numSearchReplicas
    ) {
        return getReplicaRolesWithRandomStates(
            numPromotableReplicas,
            numSearchReplicas,
            Arrays.stream(ShardRoutingState.values()).toList()
        );
    }

    private static List<Tuple<ShardRoutingState, ShardRouting.Role>> getReplicaRolesWithState(
        int numPromotableReplicas,
        int numSearchReplicas,
        ShardRoutingState state
    ) {
        return getReplicaRolesWithRandomStates(numPromotableReplicas, numSearchReplicas, List.of(state));
    }

    private int countRequestsForIndex(ClusterState state, String index) {
        PlainActionFuture<ActionResponse.Empty> response = PlainActionFuture.newFuture();
        state.routingTable().activePrimaryShardsGrouped(new String[] { index }, true).iterator().forEachRemaining(shardId -> {
            logger.debug("--> executing for primary shard id: {}", shardId.shardId());
            ActionTestUtils.execute(
                broadcastUnpromotableAction,
                null,
                new TestBroadcastUnpromotableRequest(state.routingTable().shardRoutingTable(shardId.shardId())),
                response
            );
        });

        Map<String, List<CapturingTransport.CapturedRequest>> capturedRequests = transport.getCapturedRequestsByTargetNodeAndClear();
        int totalRequests = 0;
        for (Map.Entry<String, List<CapturingTransport.CapturedRequest>> entry : capturedRequests.entrySet()) {
            logger.debug("Captured requests for node [{}] are: [{}]", entry.getKey(), entry.getValue());
            totalRequests += entry.getValue().size();
        }
        return totalRequests;
    }

    public void testNotStartedPrimary() throws Exception {
        final String index = "test";
        final int numPromotableReplicas = randomInt(2);
        final int numSearchReplicas = randomInt(2);
        final ClusterState state = state(
            index,
            randomBoolean(),
            randomBoolean() ? ShardRoutingState.INITIALIZING : ShardRoutingState.UNASSIGNED,
            getReplicaRolesWithState(numPromotableReplicas, numSearchReplicas, ShardRoutingState.UNASSIGNED)
        );
        setState(clusterService, state);
        logger.debug("--> using initial state:\n{}", clusterService.state());
        assertThat(countRequestsForIndex(state, index), is(equalTo(0)));
    }

    public void testMixOfStartedPromotableAndSearchReplicas() throws Exception {
        final String index = "test";
        final int numShards = 1 + randomInt(3);
        final int numPromotableReplicas = randomInt(2);
        final int numSearchReplicas = randomInt(2);

        ClusterState state = stateWithAssignedPrimariesAndReplicas(
            new String[] { index },
            numShards,
            getReplicaRoles(numPromotableReplicas, numSearchReplicas)
        );
        setState(clusterService, state);
        logger.debug("--> using initial state:\n{}", clusterService.state());
        assertThat(countRequestsForIndex(state, index), is(equalTo(numShards * numSearchReplicas)));
    }

    public void testSearchReplicasWithRandomStates() throws Exception {
        final String index = "test";
        final int numPromotableReplicas = randomInt(2);
        final int numSearchReplicas = randomInt(6);

        List<Tuple<ShardRoutingState, ShardRouting.Role>> replicas = getReplicaRolesWithRandomStates(
            numPromotableReplicas,
            numSearchReplicas
        );
        int numReachableUnpromotables = replicas.stream().mapToInt(t -> {
            if (t.v2() == ShardRouting.Role.SEARCH_ONLY && t.v1() != ShardRoutingState.UNASSIGNED) {
                if (t.v1() == ShardRoutingState.RELOCATING) {
                    return 2; // accounts for both the RELOCATING and the INITIALIZING copies
                }
                return 1;
            }
            return 0;
        }).sum();

        final ClusterState state = state(index, true, ShardRoutingState.STARTED, replicas);

        setState(clusterService, state);
        logger.debug("--> using initial state:\n{}", clusterService.state());
        assertThat(countRequestsForIndex(state, index), is(equalTo(numReachableUnpromotables)));
    }

    public void testInvalidNodes() throws Exception {
        final String index = "test";
        ClusterState state = stateWithAssignedPrimariesAndReplicas(
            new String[] { index },
            randomIntBetween(1, 3),
            getReplicaRoles(randomInt(2), randomIntBetween(1, 2))
        );
        setState(clusterService, state);
        logger.debug("--> using initial state:\n{}", clusterService.state());

        ShardId shardId = state.routingTable().activePrimaryShardsGrouped(new String[] { index }, true).get(0).shardId();
        IndexShardRoutingTable routingTable = state.routingTable().shardRoutingTable(shardId);
        IndexShardRoutingTable.Builder wrongRoutingTableBuilder = new IndexShardRoutingTable.Builder(shardId);
        for (int i = 0; i < routingTable.size(); i++) {
            ShardRouting shardRouting = routingTable.shard(i);
            ShardRouting wrongShardRouting = newShardRouting(
                shardId,
                shardRouting.currentNodeId() + randomIntBetween(10, 100),
                shardRouting.relocatingNodeId(),
                shardRouting.primary(),
                shardRouting.state(),
                shardRouting.unassignedInfo(),
                shardRouting.role()
            );
            wrongRoutingTableBuilder.addShard(wrongShardRouting);
        }
        IndexShardRoutingTable wrongRoutingTable = wrongRoutingTableBuilder.build();

        PlainActionFuture<ActionResponse.Empty> response = PlainActionFuture.newFuture();
        logger.debug("--> executing for wrong shard routing table: {}", wrongRoutingTable);
        assertThat(
            expectThrows(
                NodeNotConnectedException.class,
                () -> PlainActionFuture.<ActionResponse.Empty, Exception>get(
                    f -> ActionTestUtils.execute(
                        broadcastUnpromotableAction,
                        null,
                        new TestBroadcastUnpromotableRequest(wrongRoutingTable),
                        f
                    ),
                    10,
                    TimeUnit.SECONDS
                )
            ).toString(),
            containsString("discovery node must not be null")
        );
    }

    public void testNullIndexShardRoutingTable() throws Exception {
        PlainActionFuture<ActionResponse.Empty> response = PlainActionFuture.newFuture();
        IndexShardRoutingTable shardRoutingTable = null;
        assertThat(
            expectThrows(
                NullPointerException.class,
                () -> PlainActionFuture.<ActionResponse.Empty, Exception>get(
                    f -> ActionTestUtils.execute(
                        broadcastUnpromotableAction,
                        null,
                        new TestBroadcastUnpromotableRequest(shardRoutingTable),
                        f
                    ),
                    10,
                    TimeUnit.SECONDS
                )
            ).toString(),
            containsString("index shard routing table is null")
        );
    }

}
