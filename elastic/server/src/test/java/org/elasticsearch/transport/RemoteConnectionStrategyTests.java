/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */


package org.elasticsearch.transport;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.test.ESTestCase;

import static org.mockito.Mockito.mock;

public class RemoteConnectionStrategyTests extends ESTestCase {

    public void testStrategyChangeMeansThatStrategyMustBeRebuilt() {
        ClusterConnectionManager connectionManager = new ClusterConnectionManager(Settings.EMPTY, mock(Transport.class));
        RemoteConnectionManager remoteConnectionManager = new RemoteConnectionManager("cluster-alias", connectionManager);
        FakeConnectionStrategy first = new FakeConnectionStrategy("cluster-alias", mock(TransportService.class), remoteConnectionManager,
            RemoteConnectionStrategy.ConnectionStrategy.PROXY);
        Settings newSettings = Settings.builder()
            .put(RemoteConnectionStrategy.REMOTE_CONNECTION_MODE.getConcreteSettingForNamespace("cluster-alias").getKey(), "sniff")
            .put(SniffConnectionStrategy.REMOTE_CLUSTER_SEEDS.getConcreteSettingForNamespace("cluster-alias").getKey(), "127.0.0.1:9300")
            .build();
        assertTrue(first.shouldRebuildConnection(newSettings));
    }

    public void testSameStrategyChangeMeansThatStrategyDoesNotNeedToBeRebuilt() {
        ClusterConnectionManager connectionManager = new ClusterConnectionManager(Settings.EMPTY, mock(Transport.class));
        RemoteConnectionManager remoteConnectionManager = new RemoteConnectionManager("cluster-alias", connectionManager);
        FakeConnectionStrategy first = new FakeConnectionStrategy("cluster-alias", mock(TransportService.class), remoteConnectionManager,
            RemoteConnectionStrategy.ConnectionStrategy.PROXY);
        Settings newSettings = Settings.builder()
            .put(RemoteConnectionStrategy.REMOTE_CONNECTION_MODE.getConcreteSettingForNamespace("cluster-alias").getKey(), "proxy")
            .put(ProxyConnectionStrategy.PROXY_ADDRESS.getConcreteSettingForNamespace("cluster-alias").getKey(), "127.0.0.1:9300")
            .build();
        assertFalse(first.shouldRebuildConnection(newSettings));
    }

    public void testChangeInConnectionProfileMeansTheStrategyMustBeRebuilt() {
        ClusterConnectionManager connectionManager = new ClusterConnectionManager(TestProfiles.LIGHT_PROFILE, mock(Transport.class));
        assertEquals(TimeValue.MINUS_ONE, connectionManager.getConnectionProfile().getPingInterval());
        assertEquals(false, connectionManager.getConnectionProfile().getCompressionEnabled());
        RemoteConnectionManager remoteConnectionManager = new RemoteConnectionManager("cluster-alias", connectionManager);
        FakeConnectionStrategy first = new FakeConnectionStrategy("cluster-alias", mock(TransportService.class), remoteConnectionManager,
            RemoteConnectionStrategy.ConnectionStrategy.PROXY);

        Settings.Builder newBuilder = Settings.builder();
        newBuilder.put(RemoteConnectionStrategy.REMOTE_CONNECTION_MODE.getConcreteSettingForNamespace("cluster-alias").getKey(), "proxy");
        newBuilder.put(ProxyConnectionStrategy.PROXY_ADDRESS.getConcreteSettingForNamespace("cluster-alias").getKey(), "127.0.0.1:9300");
        if (randomBoolean()) {
            newBuilder.put(RemoteClusterService.REMOTE_CLUSTER_PING_SCHEDULE.getConcreteSettingForNamespace("cluster-alias").getKey(),
                TimeValue.timeValueSeconds(5));
        } else {
            newBuilder.put(RemoteClusterService.REMOTE_CLUSTER_COMPRESS.getConcreteSettingForNamespace("cluster-alias").getKey(), true);
        }
        assertTrue(first.shouldRebuildConnection(newBuilder.build()));
    }

    public void testCorrectChannelNumber() {
        String clusterAlias = "cluster-alias";

        for (RemoteConnectionStrategy.ConnectionStrategy strategy : RemoteConnectionStrategy.ConnectionStrategy.values()) {
            String settingKey = RemoteConnectionStrategy.REMOTE_CONNECTION_MODE.getConcreteSettingForNamespace(clusterAlias).getKey();
            Settings proxySettings = Settings.builder().put(settingKey, strategy.name()).build();
            ConnectionProfile proxyProfile = RemoteConnectionStrategy.buildConnectionProfile(clusterAlias, proxySettings);
            assertEquals("Incorrect number of channels for " + strategy.name(),
                strategy.getNumberOfChannels(), proxyProfile.getNumConnections());
        }
    }

    private static class FakeConnectionStrategy extends RemoteConnectionStrategy {

        private final ConnectionStrategy strategy;

        FakeConnectionStrategy(String clusterAlias, TransportService transportService, RemoteConnectionManager connectionManager,
                               RemoteConnectionStrategy.ConnectionStrategy strategy) {
            super(clusterAlias, transportService, connectionManager, Settings.EMPTY);
            this.strategy = strategy;
        }

        @Override
        protected boolean strategyMustBeRebuilt(Settings newSettings) {
            return false;
        }

        @Override
        protected ConnectionStrategy strategyType() {
            return this.strategy;
        }

        @Override
        protected boolean shouldOpenMoreConnections() {
            return false;
        }

        @Override
        protected void connectImpl(ActionListener<Void> listener) {

        }

        @Override
        protected RemoteConnectionInfo.ModeInfo getModeInfo() {
            return null;
        }
    }
}
