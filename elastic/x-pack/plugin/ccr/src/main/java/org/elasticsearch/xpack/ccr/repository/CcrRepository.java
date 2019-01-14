/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.ccr.repository;

import org.apache.lucene.index.IndexCommit;
import org.elasticsearch.Version;
import org.elasticsearch.action.admin.cluster.state.ClusterStateRequest;
import org.elasticsearch.action.admin.cluster.state.ClusterStateResponse;
import org.elasticsearch.action.admin.indices.mapping.put.PutMappingRequest;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.metadata.MappingMetaData;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.cluster.metadata.RepositoryMetaData;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.collect.ImmutableOpenMap;
import org.elasticsearch.common.component.AbstractLifecycleComponent;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.index.Index;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.engine.EngineException;
import org.elasticsearch.index.shard.IndexShard;
import org.elasticsearch.index.shard.IndexShardRecoveryException;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.index.snapshots.IndexShardRestoreFailedException;
import org.elasticsearch.index.snapshots.IndexShardSnapshotStatus;
import org.elasticsearch.index.snapshots.blobstore.BlobStoreIndexShardSnapshot;
import org.elasticsearch.index.snapshots.blobstore.SnapshotFiles;
import org.elasticsearch.index.store.Store;
import org.elasticsearch.index.store.StoreFileMetaData;
import org.elasticsearch.indices.recovery.RecoveryState;
import org.elasticsearch.repositories.IndexId;
import org.elasticsearch.repositories.Repository;
import org.elasticsearch.repositories.RepositoryData;
import org.elasticsearch.repositories.blobstore.FileRestoreContext;
import org.elasticsearch.snapshots.SnapshotId;
import org.elasticsearch.snapshots.SnapshotInfo;
import org.elasticsearch.snapshots.SnapshotShardFailure;
import org.elasticsearch.snapshots.SnapshotState;
import org.elasticsearch.xpack.ccr.Ccr;
import org.elasticsearch.xpack.ccr.CcrLicenseChecker;
import org.elasticsearch.xpack.ccr.action.CcrRequests;
import org.elasticsearch.xpack.ccr.action.repositories.ClearCcrRestoreSessionAction;
import org.elasticsearch.xpack.ccr.action.repositories.ClearCcrRestoreSessionRequest;
import org.elasticsearch.xpack.ccr.action.repositories.GetCcrRestoreFileChunkAction;
import org.elasticsearch.xpack.ccr.action.repositories.GetCcrRestoreFileChunkRequest;
import org.elasticsearch.xpack.ccr.action.repositories.PutCcrRestoreSessionAction;
import org.elasticsearch.xpack.ccr.action.repositories.PutCcrRestoreSessionRequest;

import java.io.Closeable;
import java.io.IOException;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * This repository relies on a remote cluster for Ccr restores. It is read-only so it can only be used to
 * restore shards/indexes that exist on the remote cluster.
 */
public class CcrRepository extends AbstractLifecycleComponent implements Repository {

    public static final String LATEST = "_latest_";
    public static final String TYPE = "_ccr_";
    public static final String NAME_PREFIX = "_ccr_";
    private static final SnapshotId SNAPSHOT_ID = new SnapshotId(LATEST, LATEST);

    private final RepositoryMetaData metadata;
    private final String remoteClusterAlias;
    private final Client client;
    private final CcrLicenseChecker ccrLicenseChecker;

    public CcrRepository(RepositoryMetaData metadata, Client client, CcrLicenseChecker ccrLicenseChecker, Settings settings) {
        super(settings);
        this.metadata = metadata;
        assert metadata.name().startsWith(NAME_PREFIX) : "CcrRepository metadata.name() must start with: " + NAME_PREFIX;
        this.remoteClusterAlias = Strings.split(metadata.name(), NAME_PREFIX)[1];
        this.ccrLicenseChecker = ccrLicenseChecker;
        this.client = client;
    }

    @Override
    protected void doStart() {

    }

    @Override
    protected void doStop() {

    }

    @Override
    protected void doClose() {

    }

    @Override
    public RepositoryMetaData getMetadata() {
        return metadata;
    }

    @Override
    public SnapshotInfo getSnapshotInfo(SnapshotId snapshotId) {
        assert SNAPSHOT_ID.equals(snapshotId) : "RemoteClusterRepository only supports " + SNAPSHOT_ID + " as the SnapshotId";
        Client remoteClient = client.getRemoteClusterClient(remoteClusterAlias);
        ClusterStateResponse response = remoteClient.admin().cluster().prepareState().clear().setMetaData(true).setNodes(true).get();
        ImmutableOpenMap<String, IndexMetaData> indicesMap = response.getState().metaData().indices();
        ArrayList<String> indices = new ArrayList<>(indicesMap.size());
        indicesMap.keysIt().forEachRemaining(indices::add);

        return new SnapshotInfo(snapshotId, indices, SnapshotState.SUCCESS, response.getState().getNodes().getMaxNodeVersion());
    }

    @Override
    public MetaData getSnapshotGlobalMetaData(SnapshotId snapshotId) {
        assert SNAPSHOT_ID.equals(snapshotId) : "RemoteClusterRepository only supports " + SNAPSHOT_ID + " as the SnapshotId";
        Client remoteClient = client.getRemoteClusterClient(remoteClusterAlias);
        // We set a single dummy index name to avoid fetching all the index data
        ClusterStateRequest clusterStateRequest = CcrRequests.metaDataRequest("dummy_index_name");
        ClusterStateResponse clusterState = remoteClient.admin().cluster().state(clusterStateRequest).actionGet();
        return clusterState.getState().metaData();
    }

    @Override
    public IndexMetaData getSnapshotIndexMetaData(SnapshotId snapshotId, IndexId index) throws IOException {
        assert SNAPSHOT_ID.equals(snapshotId) : "RemoteClusterRepository only supports " + SNAPSHOT_ID + " as the SnapshotId";
        String leaderIndex = index.getName();
        Client remoteClient = client.getRemoteClusterClient(remoteClusterAlias);

        ClusterStateRequest clusterStateRequest = CcrRequests.metaDataRequest(leaderIndex);
        ClusterStateResponse clusterState = remoteClient.admin().cluster().state(clusterStateRequest).actionGet();

        // Validates whether the leader cluster has been configured properly:
        PlainActionFuture<String[]> future = PlainActionFuture.newFuture();
        IndexMetaData leaderIndexMetaData = clusterState.getState().metaData().index(leaderIndex);
        ccrLicenseChecker.fetchLeaderHistoryUUIDs(remoteClient, leaderIndexMetaData, future::onFailure, future::onResponse);
        String[] leaderHistoryUUIDs = future.actionGet();

        IndexMetaData.Builder imdBuilder = IndexMetaData.builder(leaderIndexMetaData);
        // Adding the leader index uuid for each shard as custom metadata:
        Map<String, String> metadata = new HashMap<>();
        metadata.put(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_SHARD_HISTORY_UUIDS, String.join(",", leaderHistoryUUIDs));
        metadata.put(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_UUID_KEY, leaderIndexMetaData.getIndexUUID());
        metadata.put(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_NAME_KEY, leaderIndexMetaData.getIndex().getName());
        metadata.put(Ccr.CCR_CUSTOM_METADATA_REMOTE_CLUSTER_NAME_KEY, remoteClusterAlias);
        imdBuilder.putCustom(Ccr.CCR_CUSTOM_METADATA_KEY, metadata);

        return imdBuilder.build();
    }

    @Override
    public RepositoryData getRepositoryData() {
        Client remoteClient = client.getRemoteClusterClient(remoteClusterAlias);
        ClusterStateResponse response = remoteClient.admin().cluster().prepareState().clear().setMetaData(true).get();
        MetaData remoteMetaData = response.getState().getMetaData();

        Map<String, SnapshotId> copiedSnapshotIds = new HashMap<>();
        Map<String, SnapshotState> snapshotStates = new HashMap<>(copiedSnapshotIds.size());
        Map<IndexId, Set<SnapshotId>> indexSnapshots = new HashMap<>(copiedSnapshotIds.size());

        ImmutableOpenMap<String, IndexMetaData> remoteIndices = remoteMetaData.getIndices();
        for (String indexName : remoteMetaData.getConcreteAllIndices()) {
            // Both the Snapshot name and UUID are set to _latest_
            SnapshotId snapshotId = new SnapshotId(LATEST, LATEST);
            copiedSnapshotIds.put(indexName, snapshotId);
            snapshotStates.put(indexName, SnapshotState.SUCCESS);
            Index index = remoteIndices.get(indexName).getIndex();
            indexSnapshots.put(new IndexId(indexName, index.getUUID()), Collections.singleton(snapshotId));
        }

        return new RepositoryData(1, copiedSnapshotIds, snapshotStates, indexSnapshots, Collections.emptyList());
    }

    @Override
    public void initializeSnapshot(SnapshotId snapshotId, List<IndexId> indices, MetaData metaData) {
        throw new UnsupportedOperationException("Unsupported for repository of type: " + TYPE);
    }

    @Override
    public SnapshotInfo finalizeSnapshot(SnapshotId snapshotId, List<IndexId> indices, long startTime, String failure, int totalShards,
                                         List<SnapshotShardFailure> shardFailures, long repositoryStateId, boolean includeGlobalState) {
        throw new UnsupportedOperationException("Unsupported for repository of type: " + TYPE);
    }

    @Override
    public void deleteSnapshot(SnapshotId snapshotId, long repositoryStateId) {
        throw new UnsupportedOperationException("Unsupported for repository of type: " + TYPE);
    }

    @Override
    public long getSnapshotThrottleTimeInNanos() {
        throw new UnsupportedOperationException("Unsupported for repository of type: " + TYPE);
    }

    @Override
    public long getRestoreThrottleTimeInNanos() {
        return 0;
    }

    @Override
    public String startVerification() {
        throw new UnsupportedOperationException("Unsupported for repository of type: " + TYPE);
    }

    @Override
    public void endVerification(String verificationToken) {
        throw new UnsupportedOperationException("Unsupported for repository of type: " + TYPE);
    }

    @Override
    public void verify(String verificationToken, DiscoveryNode localNode) {
    }

    @Override
    public boolean isReadOnly() {
        return true;
    }

    @Override
    public void snapshotShard(IndexShard shard, Store store, SnapshotId snapshotId, IndexId indexId, IndexCommit snapshotIndexCommit,
                              IndexShardSnapshotStatus snapshotStatus) {
        throw new UnsupportedOperationException("Unsupported for repository of type: " + TYPE);
    }

    @Override
    public void restoreShard(IndexShard indexShard, SnapshotId snapshotId, Version version, IndexId indexId, ShardId shardId,
                             RecoveryState recoveryState) {
        // TODO: Add timeouts to network calls / the restore process.
        final Store store = indexShard.store();
        store.incRef();
        try {
            store.createEmpty(indexShard.indexSettings().getIndexMetaData().getCreationVersion().luceneVersion);
        } catch (EngineException | IOException e) {
            throw new IndexShardRecoveryException(shardId, "failed to create empty store", e);
        } finally {
            store.decRef();
        }

        Map<String, String> ccrMetaData = indexShard.indexSettings().getIndexMetaData().getCustomData(Ccr.CCR_CUSTOM_METADATA_KEY);
        String leaderUUID = ccrMetaData.get(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_UUID_KEY);
        Index leaderIndex = new Index(shardId.getIndexName(), leaderUUID);
        ShardId leaderShardId = new ShardId(leaderIndex, shardId.getId());

        Client remoteClient = client.getRemoteClusterClient(remoteClusterAlias);
        // TODO: There should be some local timeout. And if the remote cluster returns an unknown session
        //  response, we should be able to retry by creating a new session.
        String name = metadata.name();
        try (RestoreSession restoreSession = RestoreSession.openSession(name, remoteClient, leaderShardId, indexShard, recoveryState)) {
            restoreSession.restoreFiles();
        } catch (Exception e) {
            throw new IndexShardRestoreFailedException(indexShard.shardId(), "failed to restore snapshot [" + snapshotId + "]", e);
        }

        maybeUpdateMappings(client, remoteClient, leaderIndex, indexShard.indexSettings());
    }

    @Override
    public IndexShardSnapshotStatus getShardSnapshotStatus(SnapshotId snapshotId, Version version, IndexId indexId, ShardId leaderShardId) {
        throw new UnsupportedOperationException("Unsupported for repository of type: " + TYPE);
    }

    private void maybeUpdateMappings(Client localClient, Client remoteClient, Index leaderIndex, IndexSettings followerIndexSettings) {
        ClusterStateRequest clusterStateRequest = CcrRequests.metaDataRequest(leaderIndex.getName());
        ClusterStateResponse clusterState = remoteClient.admin().cluster().state(clusterStateRequest).actionGet();
        IndexMetaData leaderIndexMetadata = clusterState.getState().metaData().getIndexSafe(leaderIndex);
        long leaderMappingVersion = leaderIndexMetadata.getMappingVersion();

        if (leaderMappingVersion > followerIndexSettings.getIndexMetaData().getMappingVersion()) {
            Index followerIndex = followerIndexSettings.getIndex();
            MappingMetaData mappingMetaData = leaderIndexMetadata.mapping();
            PutMappingRequest putMappingRequest = CcrRequests.putMappingRequest(followerIndex.getName(), mappingMetaData);
            localClient.admin().indices().putMapping(putMappingRequest).actionGet();
        }
    }

    private static class RestoreSession extends FileRestoreContext implements Closeable {

        private static final int BUFFER_SIZE = 1 << 16;

        private final Client remoteClient;
        private final String sessionUUID;
        private final DiscoveryNode node;
        private final Store.MetadataSnapshot sourceMetaData;

        RestoreSession(String repositoryName, Client remoteClient, String sessionUUID, DiscoveryNode node, IndexShard indexShard,
                       RecoveryState recoveryState, Store.MetadataSnapshot sourceMetaData) {
            super(repositoryName, indexShard, SNAPSHOT_ID, recoveryState, BUFFER_SIZE);
            this.remoteClient = remoteClient;
            this.sessionUUID = sessionUUID;
            this.node = node;
            this.sourceMetaData = sourceMetaData;
        }

        static RestoreSession openSession(String repositoryName, Client remoteClient, ShardId leaderShardId, IndexShard indexShard,
                                          RecoveryState recoveryState) {
            String sessionUUID = UUIDs.randomBase64UUID();
            PutCcrRestoreSessionAction.PutCcrRestoreSessionResponse response = remoteClient.execute(PutCcrRestoreSessionAction.INSTANCE,
                new PutCcrRestoreSessionRequest(sessionUUID, leaderShardId)).actionGet();
            return new RestoreSession(repositoryName, remoteClient, sessionUUID, response.getNode(), indexShard, recoveryState,
                response.getStoreFileMetaData());
        }

        void restoreFiles() throws IOException {
            ArrayList<BlobStoreIndexShardSnapshot.FileInfo> fileInfos = new ArrayList<>();
            for (StoreFileMetaData fileMetaData : sourceMetaData) {
                ByteSizeValue fileSize = new ByteSizeValue(fileMetaData.length());
                fileInfos.add(new BlobStoreIndexShardSnapshot.FileInfo(fileMetaData.name(), fileMetaData, fileSize));
            }
            SnapshotFiles snapshotFiles = new SnapshotFiles(LATEST, fileInfos);
            restore(snapshotFiles);
        }

        @Override
        protected InputStream fileInputStream(BlobStoreIndexShardSnapshot.FileInfo fileInfo) {
            return new RestoreFileInputStream(remoteClient, sessionUUID, node, fileInfo.metadata());
        }

        @Override
        public void close() {
            ClearCcrRestoreSessionRequest clearRequest = new ClearCcrRestoreSessionRequest(sessionUUID, node);
            ClearCcrRestoreSessionAction.ClearCcrRestoreSessionResponse response =
                remoteClient.execute(ClearCcrRestoreSessionAction.INSTANCE, clearRequest).actionGet();
        }
    }

    private static class RestoreFileInputStream extends InputStream {

        private final Client remoteClient;
        private final String sessionUUID;
        private final DiscoveryNode node;
        private final StoreFileMetaData fileToRecover;

        private long pos = 0;

        private RestoreFileInputStream(Client remoteClient, String sessionUUID, DiscoveryNode node, StoreFileMetaData fileToRecover) {
            this.remoteClient = remoteClient;
            this.sessionUUID = sessionUUID;
            this.node = node;
            this.fileToRecover = fileToRecover;
        }


        @Override
        public int read() throws IOException {
            throw new UnsupportedOperationException();
        }

        @Override
        public int read(byte[] bytes, int off, int len) throws IOException {
            long remainingBytes = fileToRecover.length() - pos;
            if (remainingBytes <= 0) {
                return 0;
            }

            int bytesRequested = (int) Math.min(remainingBytes, len);
            String fileName = fileToRecover.name();
            GetCcrRestoreFileChunkRequest request = new GetCcrRestoreFileChunkRequest(node, sessionUUID, fileName, bytesRequested);
            GetCcrRestoreFileChunkAction.GetCcrRestoreFileChunkResponse response =
                remoteClient.execute(GetCcrRestoreFileChunkAction.INSTANCE, request).actionGet();
            BytesReference fileChunk = response.getChunk();

            int bytesReceived = fileChunk.length();
            if (bytesReceived > bytesRequested) {
                throw new IOException("More bytes [" + bytesReceived + "] received than requested [" + bytesRequested + "]");
            }

            long leaderOffset = response.getOffset();
            assert pos == leaderOffset : "Position [" + pos + "] should be equal to the leader file offset [" + leaderOffset + "].";

            try (StreamInput streamInput = fileChunk.streamInput()) {
                int bytesRead = streamInput.read(bytes, 0, bytesReceived);
                assert bytesRead == bytesReceived : "Did not read the correct number of bytes";
            }

            pos += bytesReceived;

            return bytesReceived;
        }
    }
}
