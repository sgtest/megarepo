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

import com.carrotsearch.randomizedtesting.RandomizedContext;

import org.apache.logging.log4j.CloseableThreadContext;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.cluster.ClusterModule;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateUpdateTask;
import org.elasticsearch.cluster.ESAllocationTestCase;
import org.elasticsearch.cluster.block.ClusterBlock;
import org.elasticsearch.cluster.coordination.ClusterStatePublisher.AckListener;
import org.elasticsearch.cluster.coordination.CoordinationMetaData.VotingConfiguration;
import org.elasticsearch.cluster.coordination.CoordinationState.PersistedState;
import org.elasticsearch.cluster.coordination.Coordinator.Mode;
import org.elasticsearch.cluster.coordination.CoordinatorTests.Cluster.ClusterNode;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNode.Role;
import org.elasticsearch.cluster.service.ClusterApplier;
import org.elasticsearch.common.Randomness;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.io.stream.NamedWriteableAwareStreamInput;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.lease.Releasable;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.settings.Settings.Builder;
import org.elasticsearch.common.transport.TransportAddress;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.discovery.zen.PublishClusterStateStats;
import org.elasticsearch.discovery.zen.UnicastHostsProvider.HostsResolver;
import org.elasticsearch.indices.cluster.FakeThreadPoolMasterService;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.disruption.DisruptableMockTransport;
import org.elasticsearch.test.disruption.DisruptableMockTransport.ConnectionStatus;
import org.elasticsearch.transport.TransportService;
import org.hamcrest.Matcher;
import org.junit.Before;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.EnumSet;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.BiConsumer;
import java.util.function.Consumer;
import java.util.function.Function;
import java.util.function.Supplier;
import java.util.function.UnaryOperator;
import java.util.stream.Collectors;

import static java.util.Collections.emptySet;
import static org.elasticsearch.cluster.coordination.CoordinationStateTests.clusterState;
import static org.elasticsearch.cluster.coordination.CoordinationStateTests.setValue;
import static org.elasticsearch.cluster.coordination.CoordinationStateTests.value;
import static org.elasticsearch.cluster.coordination.Coordinator.PUBLISH_TIMEOUT_SETTING;
import static org.elasticsearch.cluster.coordination.Coordinator.Mode.CANDIDATE;
import static org.elasticsearch.cluster.coordination.Coordinator.Mode.FOLLOWER;
import static org.elasticsearch.cluster.coordination.Coordinator.Mode.LEADER;
import static org.elasticsearch.cluster.coordination.CoordinatorTests.Cluster.DEFAULT_DELAY_VARIABILITY;
import static org.elasticsearch.cluster.coordination.ElectionSchedulerFactory.ELECTION_BACK_OFF_TIME_SETTING;
import static org.elasticsearch.cluster.coordination.ElectionSchedulerFactory.ELECTION_DURATION_SETTING;
import static org.elasticsearch.cluster.coordination.ElectionSchedulerFactory.ELECTION_INITIAL_TIMEOUT_SETTING;
import static org.elasticsearch.cluster.coordination.FollowersChecker.FOLLOWER_CHECK_INTERVAL_SETTING;
import static org.elasticsearch.cluster.coordination.FollowersChecker.FOLLOWER_CHECK_RETRY_COUNT_SETTING;
import static org.elasticsearch.cluster.coordination.FollowersChecker.FOLLOWER_CHECK_TIMEOUT_SETTING;
import static org.elasticsearch.cluster.coordination.LeaderChecker.LEADER_CHECK_INTERVAL_SETTING;
import static org.elasticsearch.cluster.coordination.LeaderChecker.LEADER_CHECK_RETRY_COUNT_SETTING;
import static org.elasticsearch.cluster.coordination.LeaderChecker.LEADER_CHECK_TIMEOUT_SETTING;
import static org.elasticsearch.cluster.coordination.Reconfigurator.CLUSTER_AUTO_SHRINK_VOTING_CONFIGURATION;
import static org.elasticsearch.discovery.DiscoverySettings.NO_MASTER_BLOCK_ALL;
import static org.elasticsearch.discovery.DiscoverySettings.NO_MASTER_BLOCK_ID;
import static org.elasticsearch.discovery.DiscoverySettings.NO_MASTER_BLOCK_SETTING;
import static org.elasticsearch.discovery.DiscoverySettings.NO_MASTER_BLOCK_WRITES;
import static org.elasticsearch.discovery.PeerFinder.DISCOVERY_FIND_PEERS_INTERVAL_SETTING;
import static org.elasticsearch.node.Node.NODE_NAME_SETTING;
import static org.elasticsearch.transport.TransportService.NOOP_TRANSPORT_INTERCEPTOR;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.endsWith;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;
import static org.hamcrest.Matchers.greaterThanOrEqualTo;
import static org.hamcrest.Matchers.hasItem;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.lessThan;
import static org.hamcrest.Matchers.lessThanOrEqualTo;
import static org.hamcrest.Matchers.not;
import static org.hamcrest.Matchers.nullValue;
import static org.hamcrest.Matchers.sameInstance;
import static org.hamcrest.Matchers.startsWith;

public class CoordinatorTests extends ESTestCase {

    @Before
    public void resetPortCounterBeforeEachTest() {
        resetPortCounter();
    }

    // check that runRandomly leads to reproducible results
    public void testRepeatableTests() throws Exception {
        final Callable<Long> test = () -> {
            final Cluster cluster = new Cluster(randomIntBetween(1, 5));
            cluster.runRandomly();
            final long afterRunRandomly = value(cluster.getAnyNode().getLastAppliedClusterState());
            cluster.stabilise();
            final long afterStabilisation = value(cluster.getAnyNode().getLastAppliedClusterState());
            return afterRunRandomly ^ afterStabilisation;
        };
        final long seed = randomLong();
        logger.info("First run with seed [{}]", seed);
        final long result1 = RandomizedContext.current().runWithPrivateRandomness(seed, test);
        logger.info("Second run with seed [{}]", seed);
        final long result2 = RandomizedContext.current().runWithPrivateRandomness(seed, test);
        assertEquals(result1, result2);
    }

    public void testCanUpdateClusterStateAfterStabilisation() {
        final Cluster cluster = new Cluster(randomIntBetween(1, 5));
        cluster.runRandomly();
        cluster.stabilise();

        final ClusterNode leader = cluster.getAnyLeader();
        long finalValue = randomLong();

        logger.info("--> submitting value [{}] to [{}]", finalValue, leader);
        leader.submitValue(finalValue);
        cluster.stabilise(DEFAULT_CLUSTER_STATE_UPDATE_DELAY);

        for (final ClusterNode clusterNode : cluster.clusterNodes) {
            final String nodeId = clusterNode.getId();
            final ClusterState appliedState = clusterNode.getLastAppliedClusterState();
            assertThat(nodeId + " has the applied value", value(appliedState), is(finalValue));
        }
    }

    public void testDoesNotElectNonMasterNode() {
        final Cluster cluster = new Cluster(randomIntBetween(1, 5), false);
        cluster.runRandomly();
        cluster.stabilise();

        final ClusterNode leader = cluster.getAnyLeader();
        assertTrue(leader.localNode.isMasterNode());
    }

    public void testNodesJoinAfterStableCluster() {
        final Cluster cluster = new Cluster(randomIntBetween(1, 5));
        cluster.runRandomly();
        cluster.stabilise();

        final long currentTerm = cluster.getAnyLeader().coordinator.getCurrentTerm();
        cluster.addNodesAndStabilise(randomIntBetween(1, 2));

        final long newTerm = cluster.getAnyLeader().coordinator.getCurrentTerm();
        assertEquals(currentTerm, newTerm);
    }

    public void testExpandsConfigurationWhenGrowingFromOneNodeToThreeButDoesNotShrink() {
        final Cluster cluster = new Cluster(1);
        cluster.runRandomly();
        cluster.stabilise();

        final ClusterNode leader = cluster.getAnyLeader();

        cluster.addNodesAndStabilise(2);

        {
            assertThat(leader.coordinator.getMode(), is(Mode.LEADER));
            final VotingConfiguration lastCommittedConfiguration = leader.getLastAppliedClusterState().getLastCommittedConfiguration();
            assertThat(lastCommittedConfiguration + " should be all nodes", lastCommittedConfiguration.getNodeIds(),
                equalTo(cluster.clusterNodes.stream().map(ClusterNode::getId).collect(Collectors.toSet())));
        }

        final ClusterNode disconnect1 = cluster.getAnyNode();
        logger.info("--> disconnecting {}", disconnect1);
        disconnect1.disconnect();
        cluster.stabilise();

        {
            final ClusterNode newLeader = cluster.getAnyLeader();
            final VotingConfiguration lastCommittedConfiguration = newLeader.getLastAppliedClusterState().getLastCommittedConfiguration();
            assertThat(lastCommittedConfiguration + " should be all nodes", lastCommittedConfiguration.getNodeIds(),
                equalTo(cluster.clusterNodes.stream().map(ClusterNode::getId).collect(Collectors.toSet())));
        }
    }

    public void testExpandsConfigurationWhenGrowingFromThreeToFiveNodesAndShrinksBackToThreeOnFailure() {
        final Cluster cluster = new Cluster(3);
        cluster.runRandomly();
        cluster.stabilise();

        final ClusterNode leader = cluster.getAnyLeader();

        logger.info("setting auto-shrink reconfiguration to true");
        leader.submitSetAutoShrinkVotingConfiguration(true);
        cluster.stabilise(DEFAULT_CLUSTER_STATE_UPDATE_DELAY);
        assertTrue(CLUSTER_AUTO_SHRINK_VOTING_CONFIGURATION.get(leader.getLastAppliedClusterState().metaData().settings()));

        cluster.addNodesAndStabilise(2);

        {
            assertThat(leader.coordinator.getMode(), is(Mode.LEADER));
            final VotingConfiguration lastCommittedConfiguration = leader.getLastAppliedClusterState().getLastCommittedConfiguration();
            assertThat(lastCommittedConfiguration + " should be all nodes", lastCommittedConfiguration.getNodeIds(),
                equalTo(cluster.clusterNodes.stream().map(ClusterNode::getId).collect(Collectors.toSet())));
        }

        final ClusterNode disconnect1 = cluster.getAnyNode();
        final ClusterNode disconnect2 = cluster.getAnyNodeExcept(disconnect1);
        logger.info("--> disconnecting {} and {}", disconnect1, disconnect2);
        disconnect1.disconnect();
        disconnect2.disconnect();
        cluster.stabilise();

        {
            final ClusterNode newLeader = cluster.getAnyLeader();
            final VotingConfiguration lastCommittedConfiguration = newLeader.getLastAppliedClusterState().getLastCommittedConfiguration();
            assertThat(lastCommittedConfiguration + " should be 3 nodes", lastCommittedConfiguration.getNodeIds().size(), equalTo(3));
            assertFalse(lastCommittedConfiguration.getNodeIds().contains(disconnect1.getId()));
            assertFalse(lastCommittedConfiguration.getNodeIds().contains(disconnect2.getId()));
        }

        // we still tolerate the loss of one more node here

        final ClusterNode disconnect3 = cluster.getAnyNodeExcept(disconnect1, disconnect2);
        logger.info("--> disconnecting {}", disconnect3);
        disconnect3.disconnect();
        cluster.stabilise();

        {
            final ClusterNode newLeader = cluster.getAnyLeader();
            final VotingConfiguration lastCommittedConfiguration = newLeader.getLastAppliedClusterState().getLastCommittedConfiguration();
            assertThat(lastCommittedConfiguration + " should be 3 nodes", lastCommittedConfiguration.getNodeIds().size(), equalTo(3));
            assertFalse(lastCommittedConfiguration.getNodeIds().contains(disconnect1.getId()));
            assertFalse(lastCommittedConfiguration.getNodeIds().contains(disconnect2.getId()));
            assertTrue(lastCommittedConfiguration.getNodeIds().contains(disconnect3.getId()));
        }

        // however we do not tolerate the loss of yet another one

        final ClusterNode disconnect4 = cluster.getAnyNodeExcept(disconnect1, disconnect2, disconnect3);
        logger.info("--> disconnecting {}", disconnect4);
        disconnect4.disconnect();
        cluster.runFor(DEFAULT_STABILISATION_TIME, "allowing time for fault detection");

        for (final ClusterNode clusterNode : cluster.clusterNodes) {
            assertThat(clusterNode.getId() + " should be a candidate", clusterNode.coordinator.getMode(), equalTo(Mode.CANDIDATE));
        }

        // moreover we are still stuck even if two other nodes heal
        logger.info("--> healing {} and {}", disconnect1, disconnect2);
        disconnect1.heal();
        disconnect2.heal();
        cluster.runFor(DEFAULT_STABILISATION_TIME, "allowing time for fault detection");

        for (final ClusterNode clusterNode : cluster.clusterNodes) {
            assertThat(clusterNode.getId() + " should be a candidate", clusterNode.coordinator.getMode(), equalTo(Mode.CANDIDATE));
        }

        // we require another node to heal to recover
        final ClusterNode toHeal = randomBoolean() ? disconnect3 : disconnect4;
        logger.info("--> healing {}", toHeal);
        toHeal.heal();
        cluster.stabilise();
    }

    public void testCanShrinkFromFiveNodesToThree() {
        final Cluster cluster = new Cluster(5);
        cluster.runRandomly();
        cluster.stabilise();

        {
            final ClusterNode leader = cluster.getAnyLeader();
            logger.info("setting auto-shrink reconfiguration to false");
            leader.submitSetAutoShrinkVotingConfiguration(false);
            cluster.stabilise(DEFAULT_CLUSTER_STATE_UPDATE_DELAY);
            assertFalse(CLUSTER_AUTO_SHRINK_VOTING_CONFIGURATION.get(leader.getLastAppliedClusterState().metaData().settings()));
        }

        final ClusterNode disconnect1 = cluster.getAnyNode();
        final ClusterNode disconnect2 = cluster.getAnyNodeExcept(disconnect1);

        logger.info("--> disconnecting {} and {}", disconnect1, disconnect2);
        disconnect1.disconnect();
        disconnect2.disconnect();
        cluster.stabilise();

        final ClusterNode leader = cluster.getAnyLeader();

        {
            final VotingConfiguration lastCommittedConfiguration = leader.getLastAppliedClusterState().getLastCommittedConfiguration();
            assertThat(lastCommittedConfiguration + " should be all nodes", lastCommittedConfiguration.getNodeIds(),
                equalTo(cluster.clusterNodes.stream().map(ClusterNode::getId).collect(Collectors.toSet())));
        }

        logger.info("setting auto-shrink reconfiguration to true");
        leader.submitSetAutoShrinkVotingConfiguration(true);
        cluster.stabilise(DEFAULT_CLUSTER_STATE_UPDATE_DELAY * 2); // allow for a reconfiguration
        assertTrue(CLUSTER_AUTO_SHRINK_VOTING_CONFIGURATION.get(leader.getLastAppliedClusterState().metaData().settings()));

        {
            final VotingConfiguration lastCommittedConfiguration = leader.getLastAppliedClusterState().getLastCommittedConfiguration();
            assertThat(lastCommittedConfiguration + " should be 3 nodes", lastCommittedConfiguration.getNodeIds().size(), equalTo(3));
            assertFalse(lastCommittedConfiguration.getNodeIds().contains(disconnect1.getId()));
            assertFalse(lastCommittedConfiguration.getNodeIds().contains(disconnect2.getId()));
        }
    }

    public void testDoesNotShrinkConfigurationBelowThreeNodes() {
        final Cluster cluster = new Cluster(3);
        cluster.runRandomly();
        cluster.stabilise();

        final ClusterNode disconnect1 = cluster.getAnyNode();

        logger.info("--> disconnecting {}", disconnect1);
        disconnect1.disconnect();
        cluster.stabilise();

        final ClusterNode disconnect2 = cluster.getAnyNodeExcept(disconnect1);
        logger.info("--> disconnecting {}", disconnect2);
        disconnect2.disconnect();
        cluster.runFor(DEFAULT_STABILISATION_TIME, "allowing time for fault detection");

        for (final ClusterNode clusterNode : cluster.clusterNodes) {
            assertThat(clusterNode.getId() + " should be a candidate", clusterNode.coordinator.getMode(), equalTo(Mode.CANDIDATE));
        }

        disconnect1.heal();
        cluster.stabilise(); // would not work if disconnect1 were removed from the configuration
    }

    public void testDoesNotShrinkConfigurationBelowFiveNodesIfAutoShrinkDisabled() {
        final Cluster cluster = new Cluster(5);
        cluster.runRandomly();
        cluster.stabilise();

        cluster.getAnyLeader().submitSetAutoShrinkVotingConfiguration(false);
        cluster.stabilise(DEFAULT_ELECTION_DELAY);

        final ClusterNode disconnect1 = cluster.getAnyNode();
        final ClusterNode disconnect2 = cluster.getAnyNodeExcept(disconnect1);

        logger.info("--> disconnecting {} and {}", disconnect1, disconnect2);
        disconnect1.disconnect();
        disconnect2.disconnect();
        cluster.stabilise();

        final ClusterNode disconnect3 = cluster.getAnyNodeExcept(disconnect1, disconnect2);
        logger.info("--> disconnecting {}", disconnect3);
        disconnect3.disconnect();
        cluster.runFor(DEFAULT_STABILISATION_TIME, "allowing time for fault detection");

        for (final ClusterNode clusterNode : cluster.clusterNodes) {
            assertThat(clusterNode.getId() + " should be a candidate", clusterNode.coordinator.getMode(), equalTo(Mode.CANDIDATE));
        }

        disconnect1.heal();
        cluster.stabilise(); // would not work if disconnect1 were removed from the configuration
    }

    public void testLeaderDisconnectionWithDisconnectEventDetectedQuickly() {
        final Cluster cluster = new Cluster(randomIntBetween(3, 5));
        cluster.runRandomly();
        cluster.stabilise();

        final ClusterNode originalLeader = cluster.getAnyLeader();
        logger.info("--> disconnecting leader {}", originalLeader);
        originalLeader.disconnect();
        logger.info("--> followers get disconnect event for leader {} ", originalLeader);
        cluster.getAllNodesExcept(originalLeader).forEach(cn -> cn.onDisconnectEventFrom(originalLeader));
        // turn leader into candidate, which stabilisation asserts at the end
        cluster.getAllNodesExcept(originalLeader).forEach(cn -> originalLeader.onDisconnectEventFrom(cn));
        cluster.stabilise(DEFAULT_DELAY_VARIABILITY // disconnect is scheduled
            // then wait for a new election
            + DEFAULT_ELECTION_DELAY
            // wait for the removal to be committed
            + DEFAULT_CLUSTER_STATE_UPDATE_DELAY
            // then wait for the followup reconfiguration
            + DEFAULT_CLUSTER_STATE_UPDATE_DELAY);
        assertThat(cluster.getAnyLeader().getId(), not(equalTo(originalLeader.getId())));
    }

    public void testLeaderDisconnectionWithoutDisconnectEventDetectedQuickly() {
        final Cluster cluster = new Cluster(randomIntBetween(3, 5));
        cluster.runRandomly();
        cluster.stabilise();

        final ClusterNode originalLeader = cluster.getAnyLeader();
        logger.info("--> disconnecting leader {}", originalLeader);
        originalLeader.disconnect();

        cluster.stabilise(Math.max(
            // Each follower may have just sent a leader check, which receives no response
            defaultMillis(LEADER_CHECK_TIMEOUT_SETTING)
                // then wait for the follower to check the leader
                + defaultMillis(LEADER_CHECK_INTERVAL_SETTING)
                // then wait for the exception response
                + DEFAULT_DELAY_VARIABILITY
                // then wait for a new election
                + DEFAULT_ELECTION_DELAY,

            // ALSO the leader may have just sent a follower check, which receives no response
            defaultMillis(FOLLOWER_CHECK_TIMEOUT_SETTING)
                // wait for the leader to check its followers
                + defaultMillis(FOLLOWER_CHECK_INTERVAL_SETTING)
                // then wait for the exception response
                + DEFAULT_DELAY_VARIABILITY)

            // FINALLY:

            // wait for the removal to be committed
            + DEFAULT_CLUSTER_STATE_UPDATE_DELAY
            // then wait for the followup reconfiguration
            + DEFAULT_CLUSTER_STATE_UPDATE_DELAY);

        assertThat(cluster.getAnyLeader().getId(), not(equalTo(originalLeader.getId())));
    }

    public void testUnresponsiveLeaderDetectedEventually() {
        final Cluster cluster = new Cluster(randomIntBetween(3, 5));
        cluster.runRandomly();
        cluster.stabilise();

        final ClusterNode originalLeader = cluster.getAnyLeader();
        logger.info("--> blackholing leader {}", originalLeader);
        originalLeader.blackhole();

        // This stabilisation time bound is undesirably long. TODO try and reduce it.
        cluster.stabilise(Math.max(
            // first wait for all the followers to notice the leader has gone
            (defaultMillis(LEADER_CHECK_INTERVAL_SETTING) + defaultMillis(LEADER_CHECK_TIMEOUT_SETTING))
                * defaultInt(LEADER_CHECK_RETRY_COUNT_SETTING)
                // then wait for a follower to be promoted to leader
                + DEFAULT_ELECTION_DELAY
                // and the first publication times out because of the unresponsive node
                + defaultMillis(PUBLISH_TIMEOUT_SETTING)
                // there might be a term bump causing another election
                + DEFAULT_ELECTION_DELAY

                // then wait for both of:
                + Math.max(
                // 1. the term bumping publication to time out
                defaultMillis(PUBLISH_TIMEOUT_SETTING),
                // 2. the new leader to notice that the old leader is unresponsive
                (defaultMillis(FOLLOWER_CHECK_INTERVAL_SETTING) + defaultMillis(FOLLOWER_CHECK_TIMEOUT_SETTING))
                    * defaultInt(FOLLOWER_CHECK_RETRY_COUNT_SETTING))

                // then wait for the new leader to commit a state without the old leader
                + DEFAULT_CLUSTER_STATE_UPDATE_DELAY
                // then wait for the followup reconfiguration
                + DEFAULT_CLUSTER_STATE_UPDATE_DELAY,

            // ALSO wait for the leader to notice that its followers are unresponsive
            (defaultMillis(FOLLOWER_CHECK_INTERVAL_SETTING) + defaultMillis(FOLLOWER_CHECK_TIMEOUT_SETTING))
                * defaultInt(FOLLOWER_CHECK_RETRY_COUNT_SETTING)
                // then wait for the leader to try and commit a state removing them, causing it to stand down
                + DEFAULT_CLUSTER_STATE_UPDATE_DELAY
        ));

        assertThat(cluster.getAnyLeader().getId(), not(equalTo(originalLeader.getId())));
    }

    public void testFollowerDisconnectionDetectedQuickly() {
        final Cluster cluster = new Cluster(randomIntBetween(3, 5));
        cluster.runRandomly();
        cluster.stabilise();

        final ClusterNode leader = cluster.getAnyLeader();
        final ClusterNode follower = cluster.getAnyNodeExcept(leader);
        logger.info("--> disconnecting follower {}", follower);
        follower.disconnect();
        logger.info("--> leader {} and follower {} get disconnect event", leader, follower);
        leader.onDisconnectEventFrom(follower);
        follower.onDisconnectEventFrom(leader); // to turn follower into candidate, which stabilisation asserts at the end
        cluster.stabilise(DEFAULT_DELAY_VARIABILITY // disconnect is scheduled
            + DEFAULT_CLUSTER_STATE_UPDATE_DELAY
            // then wait for the followup reconfiguration
            + DEFAULT_CLUSTER_STATE_UPDATE_DELAY);
        assertThat(cluster.getAnyLeader().getId(), equalTo(leader.getId()));
    }

    public void testFollowerDisconnectionWithoutDisconnectEventDetectedQuickly() {
        final Cluster cluster = new Cluster(randomIntBetween(3, 5));
        cluster.runRandomly();
        cluster.stabilise();

        final ClusterNode leader = cluster.getAnyLeader();
        final ClusterNode follower = cluster.getAnyNodeExcept(leader);
        logger.info("--> disconnecting follower {}", follower);
        follower.disconnect();
        cluster.stabilise(Math.max(
            // the leader may have just sent a follower check, which receives no response
            defaultMillis(FOLLOWER_CHECK_TIMEOUT_SETTING)
                // wait for the leader to check the follower
                + defaultMillis(FOLLOWER_CHECK_INTERVAL_SETTING)
                // then wait for the exception response
                + DEFAULT_DELAY_VARIABILITY
                // then wait for the removal to be committed
                + DEFAULT_CLUSTER_STATE_UPDATE_DELAY
                // then wait for the followup reconfiguration
                + DEFAULT_CLUSTER_STATE_UPDATE_DELAY,

            // ALSO the follower may have just sent a leader check, which receives no response
            defaultMillis(LEADER_CHECK_TIMEOUT_SETTING)
                // then wait for the follower to check the leader
                + defaultMillis(LEADER_CHECK_INTERVAL_SETTING)
                // then wait for the exception response, causing the follower to become a candidate
                + DEFAULT_DELAY_VARIABILITY
        ));
        assertThat(cluster.getAnyLeader().getId(), equalTo(leader.getId()));
    }

    public void testUnresponsiveFollowerDetectedEventually() {
        final Cluster cluster = new Cluster(randomIntBetween(3, 5));
        cluster.runRandomly();
        cluster.stabilise();

        final ClusterNode leader = cluster.getAnyLeader();
        final ClusterNode follower = cluster.getAnyNodeExcept(leader);
        logger.info("--> blackholing follower {}", follower);
        follower.blackhole();

        cluster.stabilise(Math.max(
            // wait for the leader to notice that the follower is unresponsive
            (defaultMillis(FOLLOWER_CHECK_INTERVAL_SETTING) + defaultMillis(FOLLOWER_CHECK_TIMEOUT_SETTING))
                * defaultInt(FOLLOWER_CHECK_RETRY_COUNT_SETTING)
                // then wait for the leader to commit a state without the follower
                + DEFAULT_CLUSTER_STATE_UPDATE_DELAY
                // then wait for the followup reconfiguration
                + DEFAULT_CLUSTER_STATE_UPDATE_DELAY,

            // ALSO wait for the follower to notice the leader is unresponsive
            (defaultMillis(LEADER_CHECK_INTERVAL_SETTING) + defaultMillis(LEADER_CHECK_TIMEOUT_SETTING))
                * defaultInt(LEADER_CHECK_RETRY_COUNT_SETTING)
        ));
        assertThat(cluster.getAnyLeader().getId(), equalTo(leader.getId()));
    }

    public void testAckListenerReceivesAcksFromAllNodes() {
        final Cluster cluster = new Cluster(randomIntBetween(3, 5));
        cluster.runRandomly();
        cluster.stabilise();
        final ClusterNode leader = cluster.getAnyLeader();
        AckCollector ackCollector = leader.submitValue(randomLong());
        cluster.stabilise(DEFAULT_CLUSTER_STATE_UPDATE_DELAY);

        for (final ClusterNode clusterNode : cluster.clusterNodes) {
            assertTrue("expected ack from " + clusterNode, ackCollector.hasAckedSuccessfully(clusterNode));
        }
        assertThat("leader should be last to ack", ackCollector.getSuccessfulAckIndex(leader), equalTo(cluster.clusterNodes.size() - 1));
    }

    public void testAckListenerReceivesNackFromFollower() {
        final Cluster cluster = new Cluster(3);
        cluster.runRandomly();
        cluster.stabilise();
        final ClusterNode leader = cluster.getAnyLeader();
        final ClusterNode follower0 = cluster.getAnyNodeExcept(leader);
        final ClusterNode follower1 = cluster.getAnyNodeExcept(leader, follower0);

        follower0.setClusterStateApplyResponse(ClusterStateApplyResponse.FAIL);
        AckCollector ackCollector = leader.submitValue(randomLong());
        cluster.stabilise(DEFAULT_CLUSTER_STATE_UPDATE_DELAY);
        assertTrue("expected ack from " + leader, ackCollector.hasAckedSuccessfully(leader));
        assertTrue("expected nack from " + follower0, ackCollector.hasAckedUnsuccessfully(follower0));
        assertTrue("expected ack from " + follower1, ackCollector.hasAckedSuccessfully(follower1));
        assertThat("leader should be last to ack", ackCollector.getSuccessfulAckIndex(leader), equalTo(1));
    }

    public void testAckListenerReceivesNackFromLeader() {
        final Cluster cluster = new Cluster(3);
        cluster.runRandomly();
        cluster.stabilise();
        final ClusterNode leader = cluster.getAnyLeader();
        final ClusterNode follower0 = cluster.getAnyNodeExcept(leader);
        final ClusterNode follower1 = cluster.getAnyNodeExcept(leader, follower0);
        final long startingTerm = leader.coordinator.getCurrentTerm();

        leader.setClusterStateApplyResponse(ClusterStateApplyResponse.FAIL);
        AckCollector ackCollector = leader.submitValue(randomLong());
        cluster.runFor(DEFAULT_CLUSTER_STATE_UPDATE_DELAY, "committing value");
        assertTrue(leader.coordinator.getMode() != Coordinator.Mode.LEADER || leader.coordinator.getCurrentTerm() > startingTerm);
        leader.setClusterStateApplyResponse(ClusterStateApplyResponse.SUCCEED);
        cluster.stabilise();
        assertTrue("expected nack from " + leader, ackCollector.hasAckedUnsuccessfully(leader));
        assertTrue("expected ack from " + follower0, ackCollector.hasAckedSuccessfully(follower0));
        assertTrue("expected ack from " + follower1, ackCollector.hasAckedSuccessfully(follower1));
        assertTrue(leader.coordinator.getMode() != Coordinator.Mode.LEADER || leader.coordinator.getCurrentTerm() > startingTerm);
    }

    public void testAckListenerReceivesNoAckFromHangingFollower() {
        final Cluster cluster = new Cluster(3);
        cluster.runRandomly();
        cluster.stabilise();
        final ClusterNode leader = cluster.getAnyLeader();
        final ClusterNode follower0 = cluster.getAnyNodeExcept(leader);
        final ClusterNode follower1 = cluster.getAnyNodeExcept(leader, follower0);

        logger.info("--> blocking cluster state application on {}", follower0);
        follower0.setClusterStateApplyResponse(ClusterStateApplyResponse.HANG);

        logger.info("--> publishing another value");
        AckCollector ackCollector = leader.submitValue(randomLong());
        cluster.runFor(DEFAULT_CLUSTER_STATE_UPDATE_DELAY, "committing value");

        assertTrue("expected immediate ack from " + follower1, ackCollector.hasAckedSuccessfully(follower1));
        assertFalse("expected no ack from " + leader, ackCollector.hasAckedSuccessfully(leader));
        cluster.stabilise(defaultMillis(PUBLISH_TIMEOUT_SETTING));
        assertTrue("expected eventual ack from " + leader, ackCollector.hasAckedSuccessfully(leader));
        assertFalse("expected no ack from " + follower0, ackCollector.hasAcked(follower0));
    }

    public void testAckListenerReceivesNacksIfPublicationTimesOut() {
        final Cluster cluster = new Cluster(3);
        cluster.runRandomly();
        cluster.stabilise();
        final ClusterNode leader = cluster.getAnyLeader();
        final ClusterNode follower0 = cluster.getAnyNodeExcept(leader);
        final ClusterNode follower1 = cluster.getAnyNodeExcept(leader, follower0);

        follower0.blackhole();
        follower1.blackhole();
        AckCollector ackCollector = leader.submitValue(randomLong());
        cluster.runFor(DEFAULT_CLUSTER_STATE_UPDATE_DELAY, "committing value");
        assertFalse("expected no immediate ack from " + leader, ackCollector.hasAcked(leader));
        assertFalse("expected no immediate ack from " + follower0, ackCollector.hasAcked(follower0));
        assertFalse("expected no immediate ack from " + follower1, ackCollector.hasAcked(follower1));
        follower0.heal();
        follower1.heal();
        cluster.stabilise();
        assertTrue("expected eventual nack from " + follower0, ackCollector.hasAckedUnsuccessfully(follower0));
        assertTrue("expected eventual nack from " + follower1, ackCollector.hasAckedUnsuccessfully(follower1));
        assertTrue("expected eventual nack from " + leader, ackCollector.hasAckedUnsuccessfully(leader));
    }

    public void testAckListenerReceivesNacksIfLeaderStandsDown() {
        final Cluster cluster = new Cluster(3);
        cluster.runRandomly();
        cluster.stabilise();
        final ClusterNode leader = cluster.getAnyLeader();
        final ClusterNode follower0 = cluster.getAnyNodeExcept(leader);
        final ClusterNode follower1 = cluster.getAnyNodeExcept(leader, follower0);

        leader.blackhole();
        follower0.onDisconnectEventFrom(leader);
        follower1.onDisconnectEventFrom(leader);
        // let followers elect a leader among themselves before healing the leader and running the publication
        cluster.runFor(DEFAULT_DELAY_VARIABILITY // disconnect is scheduled
            + DEFAULT_ELECTION_DELAY, "elect new leader");
        // cluster has two nodes in mode LEADER, in different terms ofc, and the one in the lower term won’t be able to publish anything
        leader.heal();
        AckCollector ackCollector = leader.submitValue(randomLong());
        cluster.stabilise(); // TODO: check if can find a better bound here
        assertTrue("expected nack from " + leader, ackCollector.hasAckedUnsuccessfully(leader));
        assertTrue("expected nack from " + follower0, ackCollector.hasAckedUnsuccessfully(follower0));
        assertTrue("expected nack from " + follower1, ackCollector.hasAckedUnsuccessfully(follower1));
    }

    public void testAckListenerReceivesNacksFromFollowerInHigherTerm() {
        // TODO: needs proper term bumping
//        final Cluster cluster = new Cluster(3);
//        cluster.runRandomly();
//        cluster.stabilise();
//        final ClusterNode leader = cluster.getAnyLeader();
//        final ClusterNode follower0 = cluster.getAnyNodeExcept(leader);
//        final ClusterNode follower1 = cluster.getAnyNodeExcept(leader, follower0);
//
//        follower0.coordinator.joinLeaderInTerm(new StartJoinRequest(follower0.localNode, follower0.coordinator.getCurrentTerm() + 1));
//        AckCollector ackCollector = leader.submitValue(randomLong());
//        cluster.stabilise(DEFAULT_CLUSTER_STATE_UPDATE_DELAY);
//        assertTrue("expected ack from " + leader, ackCollector.hasAckedSuccessfully(leader));
//        assertTrue("expected nack from " + follower0, ackCollector.hasAckedUnsuccessfully(follower0));
//        assertTrue("expected ack from " + follower1, ackCollector.hasAckedSuccessfully(follower1));
    }

    public void testDiscoveryOfPeersTriggersNotification() {
        final Cluster cluster = new Cluster(randomIntBetween(2, 5));

        // register a listener and then deregister it again to show that it is not called after deregistration
        try (Releasable ignored = cluster.getAnyNode().coordinator.withDiscoveryListener(ActionListener.wrap(() -> {
            throw new AssertionError("should not be called");
        }))) {
            // do nothing
        }

        final long startTimeMillis = cluster.deterministicTaskQueue.getCurrentTimeMillis();
        final ClusterNode bootstrapNode = cluster.getAnyNode();
        final AtomicBoolean hasDiscoveredAllPeers = new AtomicBoolean();
        assertFalse(bootstrapNode.coordinator.getFoundPeers().iterator().hasNext());
        try (Releasable ignored = bootstrapNode.coordinator.withDiscoveryListener(
            new ActionListener<Iterable<DiscoveryNode>>() {
                @Override
                public void onResponse(Iterable<DiscoveryNode> discoveryNodes) {
                    int peerCount = 0;
                    for (final DiscoveryNode discoveryNode : discoveryNodes) {
                        peerCount++;
                    }
                    assertThat(peerCount, lessThan(cluster.size()));
                    if (peerCount == cluster.size() - 1 && hasDiscoveredAllPeers.get() == false) {
                        hasDiscoveredAllPeers.set(true);
                        final long elapsedTimeMillis = cluster.deterministicTaskQueue.getCurrentTimeMillis() - startTimeMillis;
                        logger.info("--> {} discovered {} peers in {}ms", bootstrapNode.getId(), cluster.size() - 1, elapsedTimeMillis);
                        assertThat(elapsedTimeMillis, lessThanOrEqualTo(defaultMillis(DISCOVERY_FIND_PEERS_INTERVAL_SETTING) * 2));
                    }
                }

                @Override
                public void onFailure(Exception e) {
                    throw new AssertionError("unexpected", e);
                }
            })) {
            cluster.runFor(defaultMillis(DISCOVERY_FIND_PEERS_INTERVAL_SETTING) * 2 + randomLongBetween(0, 60000), "discovery phase");
        }

        assertTrue(hasDiscoveredAllPeers.get());

        final AtomicBoolean receivedAlreadyBootstrappedException = new AtomicBoolean();
        try (Releasable ignored = bootstrapNode.coordinator.withDiscoveryListener(
            new ActionListener<Iterable<DiscoveryNode>>() {
                @Override
                public void onResponse(Iterable<DiscoveryNode> discoveryNodes) {
                    // ignore
                }

                @Override
                public void onFailure(Exception e) {
                    if (e instanceof ClusterAlreadyBootstrappedException) {
                        receivedAlreadyBootstrappedException.set(true);
                    } else {
                        throw new AssertionError("unexpected", e);
                    }
                }
            })) {

            cluster.stabilise();
        }
        assertTrue(receivedAlreadyBootstrappedException.get());
    }

    public void testSettingInitialConfigurationTriggersElection() {
        final Cluster cluster = new Cluster(randomIntBetween(1, 5));
        cluster.runFor(defaultMillis(DISCOVERY_FIND_PEERS_INTERVAL_SETTING) * 2 + randomLongBetween(0, 60000), "initial discovery phase");
        for (final ClusterNode clusterNode : cluster.clusterNodes) {
            final String nodeId = clusterNode.getId();
            assertThat(nodeId + " is CANDIDATE", clusterNode.coordinator.getMode(), is(CANDIDATE));
            assertThat(nodeId + " is in term 0", clusterNode.coordinator.getCurrentTerm(), is(0L));
            assertThat(nodeId + " last accepted in term 0", clusterNode.coordinator.getLastAcceptedState().term(), is(0L));
            assertThat(nodeId + " last accepted version 0", clusterNode.coordinator.getLastAcceptedState().version(), is(0L));
            assertFalse(nodeId + " has not received an initial configuration", clusterNode.coordinator.isInitialConfigurationSet());
            assertTrue(nodeId + " has an empty last-accepted configuration",
                clusterNode.coordinator.getLastAcceptedState().getLastAcceptedConfiguration().isEmpty());
            assertTrue(nodeId + " has an empty last-committed configuration",
                clusterNode.coordinator.getLastAcceptedState().getLastCommittedConfiguration().isEmpty());

            final Set<DiscoveryNode> foundPeers = new HashSet<>();
            clusterNode.coordinator.getFoundPeers().forEach(foundPeers::add);
            assertTrue(nodeId + " should not have discovered itself", foundPeers.add(clusterNode.getLocalNode()));
            assertThat(nodeId + " should have found all peers", foundPeers, hasSize(cluster.size()));
        }

        final ClusterNode bootstrapNode = cluster.getAnyNode();
        bootstrapNode.applyInitialConfiguration();
        assertTrue(bootstrapNode.getId() + " has been bootstrapped", bootstrapNode.coordinator.isInitialConfigurationSet());

        cluster.stabilise(
            // the first election should succeed, because only one node knows of the initial configuration and therefore can win a
            // pre-voting round and proceed to an election, so there cannot be any collisions
            defaultMillis(ELECTION_INITIAL_TIMEOUT_SETTING) // TODO this wait is unnecessary, we could trigger the election immediately
                // Allow two round-trip for pre-voting and voting
                + 4 * DEFAULT_DELAY_VARIABILITY
                // Then a commit of the new leader's first cluster state
                + DEFAULT_CLUSTER_STATE_UPDATE_DELAY
                // Then allow time for all the other nodes to join, each of which might cause a reconfiguration
                + (cluster.size() - 1) * 2 * DEFAULT_CLUSTER_STATE_UPDATE_DELAY
            // TODO Investigate whether 4 publications is sufficient due to batching? A bound linear in the number of nodes isn't great.
        );
    }

    public void testCannotSetInitialConfigurationTwice() {
        final Cluster cluster = new Cluster(randomIntBetween(1, 5));
        cluster.runRandomly();
        cluster.stabilise();

        final Coordinator coordinator = cluster.getAnyNode().coordinator;
        assertFalse(coordinator.setInitialConfiguration(coordinator.getLastAcceptedState().getLastCommittedConfiguration()));
    }

    public void testCannotSetInitialConfigurationWithoutQuorum() {
        final Cluster cluster = new Cluster(randomIntBetween(1, 5));
        final Coordinator coordinator = cluster.getAnyNode().coordinator;
        final VotingConfiguration unknownNodeConfiguration = new VotingConfiguration(Collections.singleton("unknown-node"));
        final String exceptionMessage = expectThrows(CoordinationStateRejectedException.class,
            () -> coordinator.setInitialConfiguration(unknownNodeConfiguration)).getMessage();
        assertThat(exceptionMessage,
            startsWith("not enough nodes discovered to form a quorum in the initial configuration [knownNodes=["));
        assertThat(exceptionMessage,
            endsWith("], VotingConfiguration{unknown-node}]"));
        assertThat(exceptionMessage, containsString(coordinator.getLocalNode().toString()));

        // This is VERY BAD: setting a _different_ initial configuration. Yet it works if the first attempt will never be a quorum.
        assertTrue(coordinator.setInitialConfiguration(new VotingConfiguration(Collections.singleton(coordinator.getLocalNode().getId()))));
        cluster.stabilise();
    }

    public void testDiffBasedPublishing() {
        final Cluster cluster = new Cluster(randomIntBetween(1, 5));
        cluster.runRandomly();
        cluster.stabilise();

        final ClusterNode leader = cluster.getAnyLeader();
        final long finalValue = randomLong();
        final Map<ClusterNode, PublishClusterStateStats> prePublishStats = cluster.clusterNodes.stream().collect(
            Collectors.toMap(Function.identity(), cn -> cn.coordinator.stats().getPublishStats()));
        logger.info("--> submitting value [{}] to [{}]", finalValue, leader);
        leader.submitValue(finalValue);
        cluster.stabilise(DEFAULT_CLUSTER_STATE_UPDATE_DELAY);
        final Map<ClusterNode, PublishClusterStateStats> postPublishStats = cluster.clusterNodes.stream().collect(
            Collectors.toMap(Function.identity(), cn -> cn.coordinator.stats().getPublishStats()));

        for (ClusterNode cn : cluster.clusterNodes) {
            assertThat(value(cn.getLastAppliedClusterState()), is(finalValue));
            if (cn == leader) {
                // leader does not update publish stats as it's not using the serialized state
                assertEquals(cn.toString(), prePublishStats.get(cn).getFullClusterStateReceivedCount(),
                    postPublishStats.get(cn).getFullClusterStateReceivedCount());
                assertEquals(cn.toString(), prePublishStats.get(cn).getCompatibleClusterStateDiffReceivedCount(),
                    postPublishStats.get(cn).getCompatibleClusterStateDiffReceivedCount());
                assertEquals(cn.toString(), prePublishStats.get(cn).getIncompatibleClusterStateDiffReceivedCount(),
                    postPublishStats.get(cn).getIncompatibleClusterStateDiffReceivedCount());
            } else {
                // followers receive a diff
                assertEquals(cn.toString(), prePublishStats.get(cn).getFullClusterStateReceivedCount(),
                    postPublishStats.get(cn).getFullClusterStateReceivedCount());
                assertEquals(cn.toString(), prePublishStats.get(cn).getCompatibleClusterStateDiffReceivedCount() + 1,
                    postPublishStats.get(cn).getCompatibleClusterStateDiffReceivedCount());
                assertEquals(cn.toString(), prePublishStats.get(cn).getIncompatibleClusterStateDiffReceivedCount(),
                    postPublishStats.get(cn).getIncompatibleClusterStateDiffReceivedCount());
            }
        }
    }

    public void testJoiningNodeReceivesFullState() {
        final Cluster cluster = new Cluster(randomIntBetween(1, 5));
        cluster.runRandomly();
        cluster.stabilise();

        cluster.addNodesAndStabilise(1);
        final ClusterNode newNode = cluster.clusterNodes.get(cluster.clusterNodes.size() - 1);
        final PublishClusterStateStats newNodePublishStats = newNode.coordinator.stats().getPublishStats();
        // initial cluster state send when joining
        assertEquals(1L, newNodePublishStats.getFullClusterStateReceivedCount());
        // possible follow-up reconfiguration was published as a diff
        assertEquals(cluster.size() % 2, newNodePublishStats.getCompatibleClusterStateDiffReceivedCount());
        assertEquals(0L, newNodePublishStats.getIncompatibleClusterStateDiffReceivedCount());
    }

    public void testIncompatibleDiffResendsFullState() {
        final Cluster cluster = new Cluster(randomIntBetween(3, 5));
        cluster.runRandomly();
        cluster.stabilise();

        final ClusterNode leader = cluster.getAnyLeader();
        final ClusterNode follower = cluster.getAnyNodeExcept(leader);
        logger.info("--> blackholing {}", follower);
        follower.blackhole();
        final PublishClusterStateStats prePublishStats = follower.coordinator.stats().getPublishStats();
        logger.info("--> submitting first value to {}", leader);
        leader.submitValue(randomLong());
        cluster.runFor(DEFAULT_CLUSTER_STATE_UPDATE_DELAY + defaultMillis(PUBLISH_TIMEOUT_SETTING), "publish first state");
        logger.info("--> healing {}", follower);
        follower.heal();
        logger.info("--> submitting second value to {}", leader);
        leader.submitValue(randomLong());
        cluster.stabilise(DEFAULT_CLUSTER_STATE_UPDATE_DELAY);
        final PublishClusterStateStats postPublishStats = follower.coordinator.stats().getPublishStats();
        assertEquals(prePublishStats.getFullClusterStateReceivedCount() + 1,
            postPublishStats.getFullClusterStateReceivedCount());
        assertEquals(prePublishStats.getCompatibleClusterStateDiffReceivedCount(),
            postPublishStats.getCompatibleClusterStateDiffReceivedCount());
        assertEquals(prePublishStats.getIncompatibleClusterStateDiffReceivedCount() + 1,
            postPublishStats.getIncompatibleClusterStateDiffReceivedCount());
    }

    /**
     * Simulates a situation where a follower becomes disconnected from the leader, but only for such a short time where
     * it becomes candidate and puts up a NO_MASTER_BLOCK, but then receives a follower check from the leader. If the leader
     * does not notice the node disconnecting, it is important for the node not to be turned back into a follower but try
     * and join the leader again.
     */
    public void testStayCandidateAfterReceivingFollowerCheckFromKnownMaster() {
        final Cluster cluster = new Cluster(2, false);
        cluster.runRandomly();
        cluster.stabilise();

        final ClusterNode leader = cluster.getAnyLeader();
        final ClusterNode nonLeader = cluster.getAnyNodeExcept(leader);
        nonLeader.onNode(() -> {
            logger.debug("forcing {} to become candidate", nonLeader.getId());
            synchronized (nonLeader.coordinator.mutex) {
                nonLeader.coordinator.becomeCandidate("forced");
            }
            logger.debug("simulate follower check coming through from {} to {}", leader.getId(), nonLeader.getId());
            nonLeader.coordinator.onFollowerCheckRequest(new FollowersChecker.FollowerCheckRequest(leader.coordinator.getCurrentTerm(),
                leader.getLocalNode()));
        }).run();
        cluster.stabilise();
    }

    public void testAppliesNoMasterBlockWritesByDefault() {
        testAppliesNoMasterBlock(null, NO_MASTER_BLOCK_WRITES);
    }

    public void testAppliesNoMasterBlockWritesIfConfigured() {
        testAppliesNoMasterBlock("write", NO_MASTER_BLOCK_WRITES);
    }

    public void testAppliesNoMasterBlockAllIfConfigured() {
        testAppliesNoMasterBlock("all", NO_MASTER_BLOCK_ALL);
    }

    private void testAppliesNoMasterBlock(String noMasterBlockSetting, ClusterBlock expectedBlock) {
        final Cluster cluster = new Cluster(3);
        cluster.runRandomly();
        cluster.stabilise();

        final ClusterNode leader = cluster.getAnyLeader();
        leader.submitUpdateTask("update NO_MASTER_BLOCK_SETTING", cs -> {
            final Builder settingsBuilder = Settings.builder().put(cs.metaData().persistentSettings());
            settingsBuilder.put(NO_MASTER_BLOCK_SETTING.getKey(), noMasterBlockSetting);
            return ClusterState.builder(cs).metaData(MetaData.builder(cs.metaData()).persistentSettings(settingsBuilder.build())).build();
        });
        cluster.runFor(DEFAULT_CLUSTER_STATE_UPDATE_DELAY, "committing setting update");

        leader.disconnect();
        cluster.runFor(defaultMillis(FOLLOWER_CHECK_TIMEOUT_SETTING) + defaultMillis(FOLLOWER_CHECK_INTERVAL_SETTING)
            + DEFAULT_CLUSTER_STATE_UPDATE_DELAY, "detecting disconnection");

        assertThat(leader.clusterApplier.lastAppliedClusterState.blocks().global(), hasItem(expectedBlock));

        // TODO reboot the leader and verify that the same block is applied when it restarts
    }

    public void testNodeCannotJoinIfJoinValidationFailsOnMaster() {
        final Cluster cluster = new Cluster(randomIntBetween(1, 3));
        cluster.runRandomly();
        cluster.stabilise();

        // check that if node join validation fails on master, the nodes can't join
        List<ClusterNode> addedNodes = cluster.addNodes(randomIntBetween(1, 2));
        final Set<DiscoveryNode> validatedNodes = new HashSet<>();
        cluster.getAnyLeader().extraJoinValidators.add((discoveryNode, clusterState) -> {
            validatedNodes.add(discoveryNode);
            throw new IllegalArgumentException("join validation failed");
        });
        final long previousClusterStateVersion = cluster.getAnyLeader().getLastAppliedClusterState().version();
        cluster.runFor(10000, "failing join validation");
        assertEquals(validatedNodes, addedNodes.stream().map(ClusterNode::getLocalNode).collect(Collectors.toSet()));
        assertTrue(addedNodes.stream().allMatch(ClusterNode::isCandidate));
        final long newClusterStateVersion = cluster.getAnyLeader().getLastAppliedClusterState().version();
        assertEquals(previousClusterStateVersion, newClusterStateVersion);

        cluster.getAnyLeader().extraJoinValidators.clear();
        cluster.stabilise();
    }

    public void testNodeCannotJoinIfJoinValidationFailsOnJoiningNode() {
        final Cluster cluster = new Cluster(randomIntBetween(1, 3));
        cluster.runRandomly();
        cluster.stabilise();

        // check that if node join validation fails on joining node, the nodes can't join
        List<ClusterNode> addedNodes = cluster.addNodes(randomIntBetween(1, 2));
        final Set<DiscoveryNode> validatedNodes = new HashSet<>();
        addedNodes.stream().forEach(cn -> cn.extraJoinValidators.add((discoveryNode, clusterState) -> {
            validatedNodes.add(discoveryNode);
            throw new IllegalArgumentException("join validation failed");
        }));
        final long previousClusterStateVersion = cluster.getAnyLeader().getLastAppliedClusterState().version();
        cluster.runFor(10000, "failing join validation");
        assertEquals(validatedNodes, addedNodes.stream().map(ClusterNode::getLocalNode).collect(Collectors.toSet()));
        assertTrue(addedNodes.stream().allMatch(ClusterNode::isCandidate));
        final long newClusterStateVersion = cluster.getAnyLeader().getLastAppliedClusterState().version();
        assertEquals(previousClusterStateVersion, newClusterStateVersion);

        addedNodes.stream().forEach(cn -> cn.extraJoinValidators.clear());
        cluster.stabilise();
    }

    public void testClusterCannotFormWithFailingJoinValidation() {
        final Cluster cluster = new Cluster(randomIntBetween(1, 5));
        // fail join validation on a majority of nodes in the initial configuration
        randomValueOtherThanMany(nodes ->
            cluster.initialConfiguration.hasQuorum(
                nodes.stream().map(ClusterNode::getLocalNode).map(DiscoveryNode::getId).collect(Collectors.toSet())) == false,
            () -> randomSubsetOf(cluster.clusterNodes))
        .forEach(cn -> cn.extraJoinValidators.add((discoveryNode, clusterState) -> {
            throw new IllegalArgumentException("join validation failed");
        }));
        cluster.bootstrapIfNecessary();
        cluster.runFor(10000, "failing join validation");
        assertTrue(cluster.clusterNodes.stream().allMatch(cn -> cn.getLastAppliedClusterState().version() == 0));
    }

    private static long defaultMillis(Setting<TimeValue> setting) {
        return setting.get(Settings.EMPTY).millis() + Cluster.DEFAULT_DELAY_VARIABILITY;
    }

    private static int defaultInt(Setting<Integer> setting) {
        return setting.get(Settings.EMPTY);
    }

    // Updating the cluster state involves up to 7 delays:
    // 1. submit the task to the master service
    // 2. send PublishRequest
    // 3. receive PublishResponse
    // 4. send ApplyCommitRequest
    // 5. apply committed cluster state
    // 6. receive ApplyCommitResponse
    // 7. apply committed state on master (last one to apply cluster state)
    private static final long DEFAULT_CLUSTER_STATE_UPDATE_DELAY = 7 * DEFAULT_DELAY_VARIABILITY;

    private static final int ELECTION_RETRIES = 10;

    // The time it takes to complete an election
    private static final long DEFAULT_ELECTION_DELAY
        // Pinging all peers twice should be enough to discover all nodes
        = defaultMillis(DISCOVERY_FIND_PEERS_INTERVAL_SETTING) * 2
        // Then wait for an election to be scheduled; we allow enough time for retries to allow for collisions
        + defaultMillis(ELECTION_INITIAL_TIMEOUT_SETTING) * ELECTION_RETRIES
        + defaultMillis(ELECTION_BACK_OFF_TIME_SETTING) * ELECTION_RETRIES * (ELECTION_RETRIES - 1) / 2
        + defaultMillis(ELECTION_DURATION_SETTING) * ELECTION_RETRIES
        // Allow two round-trip for pre-voting and voting
        + 4 * DEFAULT_DELAY_VARIABILITY
        // Then a commit of the new leader's first cluster state
        + DEFAULT_CLUSTER_STATE_UPDATE_DELAY;

    private static final long DEFAULT_STABILISATION_TIME =
        // If leader just blackholed, need to wait for this to be detected
        (defaultMillis(LEADER_CHECK_INTERVAL_SETTING) + defaultMillis(LEADER_CHECK_TIMEOUT_SETTING))
            * defaultInt(LEADER_CHECK_RETRY_COUNT_SETTING)
            // then wait for a follower to be promoted to leader
            + DEFAULT_ELECTION_DELAY
            // then wait for the new leader to notice that the old leader is unresponsive
            + (defaultMillis(FOLLOWER_CHECK_INTERVAL_SETTING) + defaultMillis(FOLLOWER_CHECK_TIMEOUT_SETTING))
            * defaultInt(FOLLOWER_CHECK_RETRY_COUNT_SETTING)
            // then wait for the new leader to commit a state without the old leader
            + DEFAULT_CLUSTER_STATE_UPDATE_DELAY;

    class Cluster {

        static final long EXTREME_DELAY_VARIABILITY = 10000L;
        static final long DEFAULT_DELAY_VARIABILITY = 100L;

        final List<ClusterNode> clusterNodes;
        final DeterministicTaskQueue deterministicTaskQueue = new DeterministicTaskQueue(
            // TODO does ThreadPool need a node name any more?
            Settings.builder().put(NODE_NAME_SETTING.getKey(), "deterministic-task-queue").build(), random());
        private boolean disruptStorage;
        private final VotingConfiguration initialConfiguration;

        private final Set<String> disconnectedNodes = new HashSet<>();
        private final Set<String> blackholedNodes = new HashSet<>();
        private final Map<Long, ClusterState> committedStatesByVersion = new HashMap<>();

        private final Function<DiscoveryNode, PersistedState> defaultPersistedStateSupplier = localNode -> new MockPersistedState(0L,
                clusterState(0L, 0L, localNode, VotingConfiguration.EMPTY_CONFIG, VotingConfiguration.EMPTY_CONFIG, 0L));

        Cluster(int initialNodeCount) {
            this(initialNodeCount, true);
        }

        Cluster(int initialNodeCount, boolean allNodesMasterEligible) {
            deterministicTaskQueue.setExecutionDelayVariabilityMillis(DEFAULT_DELAY_VARIABILITY);

            assertThat(initialNodeCount, greaterThan(0));

            final Set<String> masterEligibleNodeIds = new HashSet<>(initialNodeCount);
            clusterNodes = new ArrayList<>(initialNodeCount);
            for (int i = 0; i < initialNodeCount; i++) {
                final ClusterNode clusterNode = new ClusterNode(i, allNodesMasterEligible || i == 0 || randomBoolean());
                clusterNodes.add(clusterNode);
                if (clusterNode.getLocalNode().isMasterNode()) {
                    masterEligibleNodeIds.add(clusterNode.getId());
                }
            }

            initialConfiguration = new VotingConfiguration(new HashSet<>(
                randomSubsetOf(randomIntBetween(1, masterEligibleNodeIds.size()), masterEligibleNodeIds)));

            logger.info("--> creating cluster of {} nodes (master-eligible nodes: {}) with initial configuration {}",
                initialNodeCount, masterEligibleNodeIds, initialConfiguration);
        }

        List<ClusterNode> addNodesAndStabilise(int newNodesCount) {
            final List<ClusterNode> addedNodes = addNodes(newNodesCount);
            stabilise(
                // The first pinging discovers the master
                defaultMillis(DISCOVERY_FIND_PEERS_INTERVAL_SETTING)
                    // One message delay to send a join
                    + DEFAULT_DELAY_VARIABILITY
                    // Commit a new cluster state with the new node(s). Might be split into multiple commits, and each might need a
                    // followup reconfiguration
                    + newNodesCount * 2 * DEFAULT_CLUSTER_STATE_UPDATE_DELAY);
            // TODO Investigate whether 4 publications is sufficient due to batching? A bound linear in the number of nodes isn't great.
            return addedNodes;
        }

        List<ClusterNode> addNodes(int newNodesCount) {
            logger.info("--> adding {} nodes", newNodesCount);

            final int nodeSizeAtStart = clusterNodes.size();
            final List<ClusterNode> addedNodes = new ArrayList<>();
            for (int i = 0; i < newNodesCount; i++) {
                final ClusterNode clusterNode = new ClusterNode(nodeSizeAtStart + i, true);
                addedNodes.add(clusterNode);
            }
            clusterNodes.addAll(addedNodes);
            return addedNodes;
        }

        int size() {
            return clusterNodes.size();
        }

        void runRandomly() {

            // TODO supporting (preserving?) existing disruptions needs implementing if needed, for now we just forbid it
            assertThat("may reconnect disconnected nodes, probably unexpected", disconnectedNodes, empty());
            assertThat("may reconnect blackholed nodes, probably unexpected", blackholedNodes, empty());

            final List<Runnable> cleanupActions = new ArrayList<>();
            cleanupActions.add(disconnectedNodes::clear);
            cleanupActions.add(blackholedNodes::clear);
            cleanupActions.add(() -> disruptStorage = false);

            final int randomSteps = scaledRandomIntBetween(10, 10000);
            logger.info("--> start of safety phase of at least [{}] steps", randomSteps);

            deterministicTaskQueue.setExecutionDelayVariabilityMillis(EXTREME_DELAY_VARIABILITY);
            disruptStorage = true;
            int step = 0;
            long finishTime = -1;

            while (finishTime == -1 || deterministicTaskQueue.getCurrentTimeMillis() <= finishTime) {
                step++;
                final int thisStep = step; // for lambdas

                if (randomSteps <= step && finishTime == -1) {
                    finishTime = deterministicTaskQueue.getLatestDeferredExecutionTime();
                    deterministicTaskQueue.setExecutionDelayVariabilityMillis(DEFAULT_DELAY_VARIABILITY);
                    logger.debug("----> [runRandomly {}] reducing delay variability and running until [{}ms]", step, finishTime);
                }

                try {
                    if (rarely()) {
                        final ClusterNode clusterNode = getAnyNodePreferringLeaders();
                        final int newValue = randomInt();
                        clusterNode.onNode(() -> {
                            logger.debug("----> [runRandomly {}] proposing new value [{}] to [{}]",
                                thisStep, newValue, clusterNode.getId());
                            clusterNode.submitValue(newValue);
                        }).run();
                    } else if (rarely()) {
                        final ClusterNode clusterNode = getAnyNodePreferringLeaders();
                        final boolean autoShrinkVotingConfiguration = randomBoolean();
                        clusterNode.onNode(
                            () -> {
                                logger.debug("----> [runRandomly {}] setting auto-shrink configuration to {} on {}",
                                    thisStep, autoShrinkVotingConfiguration, clusterNode.getId());
                                clusterNode.submitSetAutoShrinkVotingConfiguration(autoShrinkVotingConfiguration);
                            }).run();
                    } else if (rarely()) {
                        // reboot random node
                        final ClusterNode clusterNode = getAnyNode();
                        logger.debug("----> [runRandomly {}] rebooting [{}]", thisStep, clusterNode.getId());
                        clusterNode.close();
                        clusterNodes.forEach(
                            cn -> deterministicTaskQueue.scheduleNow(cn.onNode(
                                new Runnable() {
                                    @Override
                                    public void run() {
                                        cn.transportService.disconnectFromNode(clusterNode.getLocalNode());
                                    }

                                    @Override
                                    public String toString() {
                                        return "disconnect from " + clusterNode.getLocalNode() + " after shutdown";
                                    }
                                })));
                        clusterNodes.replaceAll(cn -> cn == clusterNode ? cn.restartedNode() : cn);
                    } else if (rarely()) {
                        final ClusterNode clusterNode = getAnyNode();
                        clusterNode.onNode(() -> {
                            logger.debug("----> [runRandomly {}] forcing {} to become candidate", thisStep, clusterNode.getId());
                            synchronized (clusterNode.coordinator.mutex) {
                                clusterNode.coordinator.becomeCandidate("runRandomly");
                            }
                        }).run();
                    } else if (rarely()) {
                        final ClusterNode clusterNode = getAnyNode();

                        switch (randomInt(2)) {
                            case 0:
                                if (clusterNode.heal()) {
                                    logger.debug("----> [runRandomly {}] healing {}", step, clusterNode.getId());
                                }
                                break;
                            case 1:
                                if (clusterNode.disconnect()) {
                                    logger.debug("----> [runRandomly {}] disconnecting {}", step, clusterNode.getId());
                                }
                                break;
                            case 2:
                                if (clusterNode.blackhole()) {
                                    logger.debug("----> [runRandomly {}] blackholing {}", step, clusterNode.getId());
                                }
                                break;
                        }
                    } else if (rarely()) {
                        final ClusterNode clusterNode = getAnyNode();
                        clusterNode.onNode(
                            () -> {
                                logger.debug("----> [runRandomly {}] applying initial configuration {} to {}",
                                    thisStep, initialConfiguration, clusterNode.getId());
                                clusterNode.coordinator.setInitialConfiguration(initialConfiguration);
                            }).run();
                    } else {
                        if (deterministicTaskQueue.hasDeferredTasks() && randomBoolean()) {
                            deterministicTaskQueue.advanceTime();
                        } else if (deterministicTaskQueue.hasRunnableTasks()) {
                            deterministicTaskQueue.runRandomTask();
                        }
                    }

                    // TODO other random steps:
                    // - reboot a node
                    // - abdicate leadership

                } catch (CoordinationStateRejectedException | UncheckedIOException ignored) {
                    // This is ok: it just means a message couldn't currently be handled.
                }

                assertConsistentStates();
            }

            logger.debug("running {} cleanup actions", cleanupActions.size());
            cleanupActions.forEach(Runnable::run);
            logger.debug("finished running cleanup actions");
        }

        private void assertConsistentStates() {
            for (final ClusterNode clusterNode : clusterNodes) {
                clusterNode.coordinator.invariant();
            }
            updateCommittedStates();
        }

        private void updateCommittedStates() {
            for (final ClusterNode clusterNode : clusterNodes) {
                ClusterState applierState = clusterNode.coordinator.getApplierState();
                ClusterState storedState = committedStatesByVersion.get(applierState.getVersion());
                if (storedState == null) {
                    committedStatesByVersion.put(applierState.getVersion(), applierState);
                } else {
                    assertEquals("expected " + applierState + " but got " + storedState,
                        value(applierState), value(storedState));
                }
            }
        }

        void stabilise() {
            stabilise(DEFAULT_STABILISATION_TIME);
        }

        void stabilise(long stabilisationDurationMillis) {
            assertThat("stabilisation requires default delay variability (and proper cleanup of raised variability)",
                deterministicTaskQueue.getExecutionDelayVariabilityMillis(), lessThanOrEqualTo(DEFAULT_DELAY_VARIABILITY));
            assertFalse("stabilisation requires stable storage", disruptStorage);

            bootstrapIfNecessary();

            runFor(stabilisationDurationMillis, "stabilising");

            final ClusterNode leader = getAnyLeader();
            final long leaderTerm = leader.coordinator.getCurrentTerm();
            final Matcher<Long> isEqualToLeaderVersion = equalTo(leader.coordinator.getLastAcceptedState().getVersion());
            final String leaderId = leader.getId();

            assertTrue(leaderId + " has been bootstrapped", leader.coordinator.isInitialConfigurationSet());
            assertTrue(leaderId + " exists in its last-applied state", leader.getLastAppliedClusterState().getNodes().nodeExists(leaderId));
            assertThat(leaderId + " has applied its state ", leader.getLastAppliedClusterState().getVersion(), isEqualToLeaderVersion);

            for (final ClusterNode clusterNode : clusterNodes) {
                final String nodeId = clusterNode.getId();
                assertFalse(nodeId + " should not have an active publication", clusterNode.coordinator.publicationInProgress());

                if (clusterNode == leader) {
                    continue;
                }

                if (isConnectedPair(leader, clusterNode)) {
                    assertThat(nodeId + " is a follower of " + leaderId, clusterNode.coordinator.getMode(), is(FOLLOWER));
                    assertThat(nodeId + " has the same term as " + leaderId, clusterNode.coordinator.getCurrentTerm(), is(leaderTerm));
                    assertTrue(nodeId + " has voted for " + leaderId, leader.coordinator.hasJoinVoteFrom(clusterNode.getLocalNode()));
                    assertThat(nodeId + " has the same accepted state as " + leaderId,
                        clusterNode.coordinator.getLastAcceptedState().getVersion(), isEqualToLeaderVersion);
                    if (clusterNode.getClusterStateApplyResponse() == ClusterStateApplyResponse.SUCCEED) {
                        assertThat(nodeId + " has the same applied state as " + leaderId,
                            clusterNode.getLastAppliedClusterState().getVersion(), isEqualToLeaderVersion);
                        assertTrue(nodeId + " is in its own latest applied state",
                            clusterNode.getLastAppliedClusterState().getNodes().nodeExists(nodeId));
                    }
                    assertTrue(nodeId + " is in the latest applied state on " + leaderId,
                        leader.getLastAppliedClusterState().getNodes().nodeExists(nodeId));
                    assertTrue(nodeId + " has been bootstrapped", clusterNode.coordinator.isInitialConfigurationSet());
                    assertThat(nodeId + " has correct master", clusterNode.getLastAppliedClusterState().nodes().getMasterNode(),
                        equalTo(leader.getLocalNode()));
                    assertThat(nodeId + " has no NO_MASTER_BLOCK",
                        clusterNode.getLastAppliedClusterState().blocks().hasGlobalBlockWithId(NO_MASTER_BLOCK_ID), equalTo(false));
                } else {
                    assertThat(nodeId + " is not following " + leaderId, clusterNode.coordinator.getMode(), is(CANDIDATE));
                    assertThat(nodeId + " has no master", clusterNode.getLastAppliedClusterState().nodes().getMasterNode(), nullValue());
                    assertThat(nodeId + " has NO_MASTER_BLOCK",
                        clusterNode.getLastAppliedClusterState().blocks().hasGlobalBlockWithId(NO_MASTER_BLOCK_ID), equalTo(true));
                    assertFalse(nodeId + " is not in the applied state on " + leaderId,
                        leader.getLastAppliedClusterState().getNodes().nodeExists(nodeId));
                }
            }

            final Set<String> connectedNodeIds
                = clusterNodes.stream().filter(n -> isConnectedPair(leader, n)).map(ClusterNode::getId).collect(Collectors.toSet());

            assertThat(leader.getLastAppliedClusterState().getNodes().getSize(), equalTo(connectedNodeIds.size()));

            final ClusterState lastAcceptedState = leader.coordinator.getLastAcceptedState();
            final VotingConfiguration lastCommittedConfiguration = lastAcceptedState.getLastCommittedConfiguration();
            assertTrue(connectedNodeIds + " should be a quorum of " + lastCommittedConfiguration,
                lastCommittedConfiguration.hasQuorum(connectedNodeIds));

            assertThat("no reconfiguration is in progress",
                lastAcceptedState.getLastCommittedConfiguration(), equalTo(lastAcceptedState.getLastAcceptedConfiguration()));
            assertThat("current configuration is already optimal",
                leader.improveConfiguration(lastAcceptedState), sameInstance(lastAcceptedState));
        }

        void bootstrapIfNecessary() {
            if (clusterNodes.stream().allMatch(ClusterNode::isNotUsefullyBootstrapped)) {
                assertThat("setting initial configuration may fail with disconnected nodes", disconnectedNodes, empty());
                assertThat("setting initial configuration may fail with blackholed nodes", blackholedNodes, empty());
                runFor(defaultMillis(DISCOVERY_FIND_PEERS_INTERVAL_SETTING) * 2, "discovery prior to setting initial configuration");
                final ClusterNode bootstrapNode = getAnyMasterEligibleNode();
                bootstrapNode.applyInitialConfiguration();
            } else {
                logger.info("setting initial configuration not required");
            }
        }

        void runFor(long runDurationMillis, String description) {
            final long endTime = deterministicTaskQueue.getCurrentTimeMillis() + runDurationMillis;
            logger.info("--> runFor({}ms) running until [{}ms]: {}", runDurationMillis, endTime, description);

            while (deterministicTaskQueue.getCurrentTimeMillis() < endTime) {

                while (deterministicTaskQueue.hasRunnableTasks()) {
                    try {
                        deterministicTaskQueue.runRandomTask();
                    } catch (CoordinationStateRejectedException e) {
                        logger.debug("ignoring benign exception thrown when stabilising", e);
                    }
                    for (final ClusterNode clusterNode : clusterNodes) {
                        clusterNode.coordinator.invariant();
                    }
                    updateCommittedStates();
                }

                if (deterministicTaskQueue.hasDeferredTasks() == false) {
                    // A 1-node cluster has no need for fault detection etc so will eventually run out of things to do.
                    assert clusterNodes.size() == 1 : clusterNodes.size();
                    break;
                }

                deterministicTaskQueue.advanceTime();
            }

            logger.info("--> runFor({}ms) completed run until [{}ms]: {}", runDurationMillis, endTime, description);
        }

        private boolean isConnectedPair(ClusterNode n1, ClusterNode n2) {
            return n1 == n2 ||
                (getConnectionStatus(n1.getLocalNode(), n2.getLocalNode()) == ConnectionStatus.CONNECTED
                    && getConnectionStatus(n2.getLocalNode(), n1.getLocalNode()) == ConnectionStatus.CONNECTED);
        }

        ClusterNode getAnyLeader() {
            List<ClusterNode> allLeaders = clusterNodes.stream().filter(ClusterNode::isLeader).collect(Collectors.toList());
            assertThat("leaders", allLeaders, not(empty()));
            return randomFrom(allLeaders);
        }

        private final ConnectionStatus preferredUnknownNodeConnectionStatus =
            randomFrom(ConnectionStatus.DISCONNECTED, ConnectionStatus.BLACK_HOLE);

        private ConnectionStatus getConnectionStatus(DiscoveryNode sender, DiscoveryNode destination) {
            ConnectionStatus connectionStatus;
            if (blackholedNodes.contains(sender.getId()) || blackholedNodes.contains(destination.getId())) {
                connectionStatus = ConnectionStatus.BLACK_HOLE;
            } else if (disconnectedNodes.contains(sender.getId()) || disconnectedNodes.contains(destination.getId())) {
                connectionStatus = ConnectionStatus.DISCONNECTED;
            } else if (nodeExists(sender) && nodeExists(destination)) {
                connectionStatus = ConnectionStatus.CONNECTED;
            } else {
                connectionStatus = usually() ? preferredUnknownNodeConnectionStatus :
                    randomFrom(ConnectionStatus.DISCONNECTED, ConnectionStatus.BLACK_HOLE);
            }
            return connectionStatus;
        }

        boolean nodeExists(DiscoveryNode node) {
            return clusterNodes.stream().anyMatch(cn -> cn.getLocalNode().equals(node));
        }

        ClusterNode getAnyMasterEligibleNode() {
            return randomFrom(clusterNodes.stream().filter(n -> n.getLocalNode().isMasterNode()).collect(Collectors.toList()));
        }

        ClusterNode getAnyNode() {
            return getAnyNodeExcept();
        }

        ClusterNode getAnyNodeExcept(ClusterNode... clusterNodes) {
            List<ClusterNode> filteredNodes = getAllNodesExcept(clusterNodes);
            assert filteredNodes.isEmpty() == false;
            return randomFrom(filteredNodes);
        }

        List<ClusterNode> getAllNodesExcept(ClusterNode... clusterNodes) {
            Set<String> forbiddenIds = Arrays.stream(clusterNodes).map(ClusterNode::getId).collect(Collectors.toSet());
            List<ClusterNode> acceptableNodes
                = this.clusterNodes.stream().filter(n -> forbiddenIds.contains(n.getId()) == false).collect(Collectors.toList());
            return acceptableNodes;
        }

        ClusterNode getAnyNodePreferringLeaders() {
            for (int i = 0; i < 3; i++) {
                ClusterNode clusterNode = getAnyNode();
                if (clusterNode.coordinator.getMode() == LEADER) {
                    return clusterNode;
                }
            }
            return getAnyNode();
        }

        class MockPersistedState extends InMemoryPersistedState {
            MockPersistedState(long term, ClusterState acceptedState) {
                super(term, acceptedState);
            }

            private void possiblyFail(String description) {
                if (disruptStorage && rarely()) {
                    // TODO revisit this when we've decided how PersistedState should throw exceptions
                    logger.trace("simulating IO exception [{}]", description);
                    if (randomBoolean()) {
                        throw new UncheckedIOException(new IOException("simulated IO exception [" + description + ']'));
                    } else {
                        throw new CoordinationStateRejectedException("simulated IO exception [" + description + ']');
                    }
                }
            }

            @Override
            public void setCurrentTerm(long currentTerm) {
                possiblyFail("before writing term of " + currentTerm);
                super.setCurrentTerm(currentTerm);
                // TODO possiblyFail() here if that's a failure mode of the storage layer
            }

            @Override
            public void setLastAcceptedState(ClusterState clusterState) {
                possiblyFail("before writing last-accepted state of term=" + clusterState.term() + ", version=" + clusterState.version());
                super.setLastAcceptedState(clusterState);
                // TODO possiblyFail() here if that's a failure mode of the storage layer
            }
        }

        class ClusterNode {
            private final Logger logger = LogManager.getLogger(ClusterNode.class);

            private final int nodeIndex;
            private Coordinator coordinator;
            private final DiscoveryNode localNode;
            private final PersistedState persistedState;
            private FakeClusterApplier clusterApplier;
            private AckedFakeThreadPoolMasterService masterService;
            private TransportService transportService;
            private DisruptableMockTransport mockTransport;
            private List<BiConsumer<DiscoveryNode, ClusterState>> extraJoinValidators = new ArrayList<>();
            private ClusterStateApplyResponse clusterStateApplyResponse = ClusterStateApplyResponse.SUCCEED;

            ClusterNode(int nodeIndex, boolean masterEligible) {
                this(nodeIndex, createDiscoveryNode(nodeIndex, masterEligible), defaultPersistedStateSupplier);
            }

            ClusterNode(int nodeIndex, DiscoveryNode localNode, Function<DiscoveryNode, PersistedState> persistedStateSupplier) {
                this.nodeIndex = nodeIndex;
                this.localNode = localNode;
                persistedState = persistedStateSupplier.apply(localNode);
                onNodeLog(localNode, this::setUp).run();
            }

            private void setUp() {
                mockTransport = new DisruptableMockTransport(localNode, logger) {
                    @Override
                    protected void execute(Runnable runnable) {
                        deterministicTaskQueue.scheduleNow(onNode(runnable));
                    }

                    @Override
                    protected ConnectionStatus getConnectionStatus(DiscoveryNode destination) {
                        return Cluster.this.getConnectionStatus(getLocalNode(), destination);
                    }

                    @Override
                    protected Optional<DisruptableMockTransport> getDisruptableMockTransport(TransportAddress address) {
                        return clusterNodes.stream().map(cn -> cn.mockTransport)
                            .filter(transport -> transport.getLocalNode().getAddress().equals(address)).findAny();
                    }
                };

                final Settings settings = Settings.builder()
                    .putList(ClusterBootstrapService.INITIAL_MASTER_NODES_SETTING.getKey(),
                        ClusterBootstrapService.INITIAL_MASTER_NODES_SETTING.get(Settings.EMPTY)).build(); // suppress auto-bootstrap

                final ClusterSettings clusterSettings = new ClusterSettings(settings, ClusterSettings.BUILT_IN_CLUSTER_SETTINGS);
                clusterApplier = new FakeClusterApplier(settings, clusterSettings);
                masterService = new AckedFakeThreadPoolMasterService("test_node", "test",
                    runnable -> deterministicTaskQueue.scheduleNow(onNode(runnable)));
                transportService = mockTransport.createTransportService(
                    settings, deterministicTaskQueue.getThreadPool(this::onNode), NOOP_TRANSPORT_INTERCEPTOR,
                    a -> localNode, null, emptySet());
                final Collection<BiConsumer<DiscoveryNode, ClusterState>> onJoinValidators =
                    Collections.singletonList((dn, cs) -> extraJoinValidators.forEach(validator -> validator.accept(dn, cs)));
                coordinator = new Coordinator("test_node", settings, clusterSettings, transportService, writableRegistry(),
                    ESAllocationTestCase.createAllocationService(Settings.EMPTY), masterService, this::getPersistedState,
                    Cluster.this::provideUnicastHosts, clusterApplier, onJoinValidators, Randomness.get());
                masterService.setClusterStatePublisher(coordinator);

                logger.trace("starting up [{}]", localNode);
                transportService.start();
                transportService.acceptIncomingRequests();
                masterService.start();
                coordinator.start();
                coordinator.startInitialJoin();
            }

            void close() {
                logger.trace("taking down [{}]", localNode);
                //transportService.stop(); // does blocking stuff :/
                masterService.stop();
                coordinator.stop();
                //transportService.close(); // does blocking stuff :/
                masterService.close();
                coordinator.close();
            }

            ClusterNode restartedNode() {
                final TransportAddress address = randomBoolean() ? buildNewFakeTransportAddress() : localNode.getAddress();
                final DiscoveryNode newLocalNode = new DiscoveryNode(localNode.getName(), localNode.getId(),
                    UUIDs.randomBase64UUID(random()), // generated deterministically for repeatable tests
                    address.address().getHostString(), address.getAddress(), address, Collections.emptyMap(),
                    localNode.isMasterNode() ? EnumSet.allOf(Role.class) : emptySet(), Version.CURRENT);
                final PersistedState newPersistedState;
                try {
                    BytesStreamOutput outStream = new BytesStreamOutput();
                    outStream.setVersion(Version.CURRENT);
                    persistedState.getLastAcceptedState().writeTo(outStream);
                    StreamInput inStream = new NamedWriteableAwareStreamInput(outStream.bytes().streamInput(),
                        new NamedWriteableRegistry(ClusterModule.getNamedWriteables()));
                    newPersistedState = new MockPersistedState(persistedState.getCurrentTerm(),
                        ClusterState.readFrom(inStream, newLocalNode)); // adapts it to new localNode instance
                } catch (IOException e) {
                    throw new UncheckedIOException(e);
                }
                return new ClusterNode(nodeIndex, newLocalNode, node -> newPersistedState);
            }

            private PersistedState getPersistedState() {
                return persistedState;
            }

            String getId() {
                return localNode.getId();
            }

            DiscoveryNode getLocalNode() {
                return localNode;
            }

            boolean isLeader() {
                return coordinator.getMode() == LEADER;
            }

            boolean isCandidate() {
                return coordinator.getMode() == CANDIDATE;
            }

            ClusterState improveConfiguration(ClusterState currentState) {
                synchronized (coordinator.mutex) {
                    return coordinator.improveConfiguration(currentState);
                }
            }

            void setClusterStateApplyResponse(ClusterStateApplyResponse clusterStateApplyResponse) {
                this.clusterStateApplyResponse = clusterStateApplyResponse;
            }

            ClusterStateApplyResponse getClusterStateApplyResponse() {
                return clusterStateApplyResponse;
            }

            Runnable onNode(Runnable runnable) {
                final Runnable wrapped = onNodeLog(localNode, runnable);
                return new Runnable() {
                    @Override
                    public void run() {
                        if (clusterNodes.contains(ClusterNode.this) == false) {
                            logger.trace("ignoring runnable {} from node {} as node has been removed from cluster", runnable, localNode);
                            return;
                        }
                        wrapped.run();
                    }

                    @Override
                    public String toString() {
                        return wrapped.toString();
                    }
                };
            }

            void submitSetAutoShrinkVotingConfiguration(final boolean autoShrinkVotingConfiguration) {
                submitUpdateTask("set master nodes failure tolerance [" + autoShrinkVotingConfiguration + "]", cs ->
                    ClusterState.builder(cs).metaData(
                        MetaData.builder(cs.metaData())
                            .persistentSettings(Settings.builder()
                                .put(cs.metaData().persistentSettings())
                                .put(CLUSTER_AUTO_SHRINK_VOTING_CONFIGURATION.getKey(), autoShrinkVotingConfiguration)
                                .build())
                            .build())
                        .build());
            }

            AckCollector submitValue(final long value) {
                return submitUpdateTask("new value [" + value + "]", cs -> setValue(cs, value));
            }

            AckCollector submitUpdateTask(String source, UnaryOperator<ClusterState> clusterStateUpdate) {
                final AckCollector ackCollector = new AckCollector();
                onNode(() -> {
                    logger.trace("[{}] submitUpdateTask: enqueueing [{}]", localNode.getId(), source);
                    final long submittedTerm = coordinator.getCurrentTerm();
                    masterService.submitStateUpdateTask(source,
                        new ClusterStateUpdateTask() {
                            @Override
                            public ClusterState execute(ClusterState currentState) {
                                assertThat(currentState.term(), greaterThanOrEqualTo(submittedTerm));
                                masterService.nextAckCollector = ackCollector;
                                return clusterStateUpdate.apply(currentState);
                            }

                            @Override
                            public void onFailure(String source, Exception e) {
                                logger.debug(() -> new ParameterizedMessage("failed to publish: [{}]", source), e);
                            }

                            @Override
                            public void clusterStateProcessed(String source, ClusterState oldState, ClusterState newState) {
                                updateCommittedStates();
                                ClusterState state = committedStatesByVersion.get(newState.version());
                                assertNotNull("State not committed : " + newState.toString(), state);
                                assertEquals(value(state), value(newState));
                                logger.trace("successfully published: [{}]", newState);
                            }
                        });
                }).run();
                return ackCollector;
            }

            @Override
            public String toString() {
                return localNode.toString();
            }

            boolean heal() {
                boolean unBlackholed = blackholedNodes.remove(localNode.getId());
                boolean unDisconnected = disconnectedNodes.remove(localNode.getId());
                assert unBlackholed == false || unDisconnected == false;
                return unBlackholed || unDisconnected;
            }

            boolean disconnect() {
                boolean unBlackholed = blackholedNodes.remove(localNode.getId());
                boolean disconnected = disconnectedNodes.add(localNode.getId());
                assert disconnected || unBlackholed == false;
                return disconnected;
            }

            boolean blackhole() {
                boolean unDisconnected = disconnectedNodes.remove(localNode.getId());
                boolean blackholed = blackholedNodes.add(localNode.getId());
                assert blackholed || unDisconnected == false;
                return blackholed;
            }

            void onDisconnectEventFrom(ClusterNode clusterNode) {
                transportService.disconnectFromNode(clusterNode.localNode);
            }

            ClusterState getLastAppliedClusterState() {
                return clusterApplier.lastAppliedClusterState;
            }

            void applyInitialConfiguration() {
                onNode(() -> {
                    try {
                        coordinator.setInitialConfiguration(initialConfiguration);
                        logger.info("successfully set initial configuration to {}", initialConfiguration);
                    } catch (CoordinationStateRejectedException e) {
                        logger.info(new ParameterizedMessage("failed to set initial configuration to {}", initialConfiguration), e);
                    }
                }).run();
            }

            private boolean isNotUsefullyBootstrapped() {
                return getLocalNode().isMasterNode() == false || coordinator.isInitialConfigurationSet() == false;
            }

            private class FakeClusterApplier implements ClusterApplier {

                final ClusterName clusterName;
                private final ClusterSettings clusterSettings;
                ClusterState lastAppliedClusterState;

                private FakeClusterApplier(Settings settings, ClusterSettings clusterSettings) {
                    clusterName = ClusterName.CLUSTER_NAME_SETTING.get(settings);
                    this.clusterSettings = clusterSettings;
                }

                @Override
                public void setInitialState(ClusterState initialState) {
                    assert lastAppliedClusterState == null;
                    assert initialState != null;
                    lastAppliedClusterState = initialState;
                }

                @Override
                public void onNewClusterState(String source, Supplier<ClusterState> clusterStateSupplier, ClusterApplyListener listener) {
                    switch (clusterStateApplyResponse) {
                        case SUCCEED:
                            deterministicTaskQueue.scheduleNow(onNode(new Runnable() {
                                @Override
                                public void run() {
                                    final ClusterState oldClusterState = clusterApplier.lastAppliedClusterState;
                                    final ClusterState newClusterState = clusterStateSupplier.get();
                                    assert oldClusterState.version() <= newClusterState.version() : "updating cluster state from version "
                                        + oldClusterState.version() + " to stale version " + newClusterState.version();
                                    clusterApplier.lastAppliedClusterState = newClusterState;
                                    final Settings incomingSettings = newClusterState.metaData().settings();
                                    clusterSettings.applySettings(incomingSettings); // TODO validation might throw exceptions here.
                                    listener.onSuccess(source);
                                }

                                @Override
                                public String toString() {
                                    return "apply cluster state from [" + source + "]";
                                }
                            }));
                            break;
                        case FAIL:
                            deterministicTaskQueue.scheduleNow(onNode(new Runnable() {
                                @Override
                                public void run() {
                                    listener.onFailure(source, new ElasticsearchException("cluster state application failed"));
                                }

                                @Override
                                public String toString() {
                                    return "fail to apply cluster state from [" + source + "]";
                                }
                            }));
                            break;
                        case HANG:
                            if (randomBoolean()) {
                                deterministicTaskQueue.scheduleNow(onNode(new Runnable() {
                                    @Override
                                    public void run() {
                                        final ClusterState oldClusterState = clusterApplier.lastAppliedClusterState;
                                        final ClusterState newClusterState = clusterStateSupplier.get();
                                        assert oldClusterState.version() <= newClusterState.version() :
                                            "updating cluster state from version "
                                                + oldClusterState.version() + " to stale version " + newClusterState.version();
                                        clusterApplier.lastAppliedClusterState = newClusterState;
                                    }

                                    @Override
                                    public String toString() {
                                        return "apply cluster state from [" + source + "] without ack";
                                    }
                                }));
                            }
                            break;
                    }
                }
            }
        }

        private List<TransportAddress> provideUnicastHosts(HostsResolver ignored) {
            return clusterNodes.stream().map(ClusterNode::getLocalNode).map(DiscoveryNode::getAddress).collect(Collectors.toList());
        }
    }

    public static Runnable onNodeLog(DiscoveryNode node, Runnable runnable) {
        final String nodeId = "{" + node.getId() + "}{" + node.getEphemeralId() + "}";
        return new Runnable() {
            @Override
            public void run() {
                try (CloseableThreadContext.Instance ignored = CloseableThreadContext.put("nodeId", nodeId)) {
                    runnable.run();
                }
            }

            @Override
            public String toString() {
                return nodeId + ": " + runnable.toString();
            }
        };
    }

    static class AckCollector implements AckListener {

        private final Set<DiscoveryNode> ackedNodes = new HashSet<>();
        private final List<DiscoveryNode> successfulNodes = new ArrayList<>();
        private final List<DiscoveryNode> unsuccessfulNodes = new ArrayList<>();

        @Override
        public void onCommit(TimeValue commitTime) {
            // TODO we only currently care about per-node acks
        }

        @Override
        public void onNodeAck(DiscoveryNode node, Exception e) {
            assertTrue("duplicate ack from " + node, ackedNodes.add(node));
            if (e == null) {
                successfulNodes.add(node);
            } else {
                unsuccessfulNodes.add(node);
            }
        }

        boolean hasAckedSuccessfully(ClusterNode clusterNode) {
            return successfulNodes.contains(clusterNode.localNode);
        }

        boolean hasAckedUnsuccessfully(ClusterNode clusterNode) {
            return unsuccessfulNodes.contains(clusterNode.localNode);
        }

        boolean hasAcked(ClusterNode clusterNode) {
            return ackedNodes.contains(clusterNode.localNode);
        }

        int getSuccessfulAckIndex(ClusterNode clusterNode) {
            assert successfulNodes.contains(clusterNode.localNode) : "get index of " + clusterNode;
            return successfulNodes.indexOf(clusterNode.localNode);
        }
    }

    static class AckedFakeThreadPoolMasterService extends FakeThreadPoolMasterService {

        AckCollector nextAckCollector = new AckCollector();

        AckedFakeThreadPoolMasterService(String nodeName, String serviceName, Consumer<Runnable> onTaskAvailableToRun) {
            super(nodeName, serviceName, onTaskAvailableToRun);
        }

        @Override
        protected AckListener wrapAckListener(AckListener ackListener) {
            final AckCollector ackCollector = nextAckCollector;
            nextAckCollector = new AckCollector();
            return new AckListener() {
                @Override
                public void onCommit(TimeValue commitTime) {
                    ackCollector.onCommit(commitTime);
                    ackListener.onCommit(commitTime);
                }

                @Override
                public void onNodeAck(DiscoveryNode node, Exception e) {
                    ackCollector.onNodeAck(node, e);
                    ackListener.onNodeAck(node, e);
                }
            };
        }
    }

    private static DiscoveryNode createDiscoveryNode(int nodeIndex, boolean masterEligible) {
        final TransportAddress address = buildNewFakeTransportAddress();
        return new DiscoveryNode("", "node" + nodeIndex,
            UUIDs.randomBase64UUID(random()), // generated deterministically for repeatable tests
            address.address().getHostString(), address.getAddress(), address, Collections.emptyMap(),
            masterEligible ? EnumSet.allOf(Role.class) : emptySet(), Version.CURRENT);
    }

    /**
     * How to behave with a new cluster state
     */
    enum ClusterStateApplyResponse {
        /**
         * Apply the state (default)
         */
        SUCCEED,

        /**
         * Reject the state with an exception.
         */
        FAIL,

        /**
         * Never respond either way.
         */
        HANG,
    }

}
