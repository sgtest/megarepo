/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.ccr.repository;

import com.carrotsearch.hppc.cursors.IntObjectCursor;
import com.carrotsearch.hppc.cursors.ObjectObjectCursor;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.apache.lucene.index.IndexCommit;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.ElasticsearchSecurityException;
import org.elasticsearch.ExceptionsHelper;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.cluster.state.ClusterStateRequest;
import org.elasticsearch.action.admin.cluster.state.ClusterStateResponse;
import org.elasticsearch.action.admin.indices.mapping.put.PutMappingRequest;
import org.elasticsearch.action.support.ListenerTimeouts;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.metadata.MappingMetaData;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.cluster.metadata.RepositoryMetaData;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.collect.ImmutableOpenMap;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.component.AbstractLifecycleComponent;
import org.elasticsearch.common.metrics.CounterMetric;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.concurrent.AbstractRunnable;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.index.Index;
import org.elasticsearch.index.engine.EngineException;
import org.elasticsearch.index.seqno.LocalCheckpointTracker;
import org.elasticsearch.index.seqno.RetentionLeaseAlreadyExistsException;
import org.elasticsearch.index.seqno.RetentionLeaseNotFoundException;
import org.elasticsearch.index.shard.IndexShard;
import org.elasticsearch.index.shard.IndexShardRecoveryException;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.index.snapshots.IndexShardRestoreFailedException;
import org.elasticsearch.index.snapshots.IndexShardSnapshotStatus;
import org.elasticsearch.index.snapshots.blobstore.BlobStoreIndexShardSnapshot.FileInfo;
import org.elasticsearch.index.snapshots.blobstore.SnapshotFiles;
import org.elasticsearch.index.store.Store;
import org.elasticsearch.index.store.StoreFileMetaData;
import org.elasticsearch.indices.recovery.MultiFileWriter;
import org.elasticsearch.indices.recovery.RecoveryState;
import org.elasticsearch.repositories.IndexId;
import org.elasticsearch.repositories.Repository;
import org.elasticsearch.repositories.RepositoryData;
import org.elasticsearch.repositories.blobstore.FileRestoreContext;
import org.elasticsearch.snapshots.SnapshotId;
import org.elasticsearch.snapshots.SnapshotInfo;
import org.elasticsearch.snapshots.SnapshotShardFailure;
import org.elasticsearch.snapshots.SnapshotState;
import org.elasticsearch.threadpool.Scheduler;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.ccr.Ccr;
import org.elasticsearch.xpack.ccr.CcrLicenseChecker;
import org.elasticsearch.xpack.ccr.CcrRetentionLeases;
import org.elasticsearch.xpack.ccr.CcrSettings;
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
import java.util.Optional;
import java.util.Set;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.LongConsumer;
import java.util.function.Supplier;

import static org.elasticsearch.index.seqno.SequenceNumbers.NO_OPS_PERFORMED;
import static org.elasticsearch.xpack.ccr.CcrRetentionLeases.retentionLeaseId;
import static org.elasticsearch.xpack.ccr.CcrRetentionLeases.syncAddRetentionLease;
import static org.elasticsearch.xpack.ccr.CcrRetentionLeases.syncRenewRetentionLease;


/**
 * This repository relies on a remote cluster for Ccr restores. It is read-only so it can only be used to
 * restore shards/indexes that exist on the remote cluster.
 */
public class CcrRepository extends AbstractLifecycleComponent implements Repository {

    private static final Logger logger = LogManager.getLogger(CcrRepository.class);

    public static final String LATEST = "_latest_";
    public static final String TYPE = "_ccr_";
    public static final String NAME_PREFIX = "_ccr_";
    private static final SnapshotId SNAPSHOT_ID = new SnapshotId(LATEST, LATEST);
    private static final String IN_SYNC_ALLOCATION_ID = "ccr_restore";

    private final RepositoryMetaData metadata;
    private final CcrSettings ccrSettings;
    private final String localClusterName;
    private final String remoteClusterAlias;
    private final Client client;
    private final CcrLicenseChecker ccrLicenseChecker;
    private final ThreadPool threadPool;

    private final CounterMetric throttledTime = new CounterMetric();

    public CcrRepository(RepositoryMetaData metadata, Client client, CcrLicenseChecker ccrLicenseChecker, Settings settings,
                         CcrSettings ccrSettings, ThreadPool threadPool) {
        this.metadata = metadata;
        this.ccrSettings = ccrSettings;
        this.localClusterName = ClusterName.CLUSTER_NAME_SETTING.get(settings).value();
        assert metadata.name().startsWith(NAME_PREFIX) : "CcrRepository metadata.name() must start with: " + NAME_PREFIX;
        this.remoteClusterAlias = Strings.split(metadata.name(), NAME_PREFIX)[1];
        this.ccrLicenseChecker = ccrLicenseChecker;
        this.client = client;
        this.threadPool = threadPool;
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

    private Client getRemoteClusterClient() {
        return client.getRemoteClusterClient(remoteClusterAlias);
    }

    @Override
    public SnapshotInfo getSnapshotInfo(SnapshotId snapshotId) {
        assert SNAPSHOT_ID.equals(snapshotId) : "RemoteClusterRepository only supports " + SNAPSHOT_ID + " as the SnapshotId";
        Client remoteClient = getRemoteClusterClient();
        ClusterStateResponse response = remoteClient.admin().cluster().prepareState().clear().setMetaData(true).setNodes(true)
            .get(ccrSettings.getRecoveryActionTimeout());
        ImmutableOpenMap<String, IndexMetaData> indicesMap = response.getState().metaData().indices();
        ArrayList<String> indices = new ArrayList<>(indicesMap.size());
        indicesMap.keysIt().forEachRemaining(indices::add);

        return new SnapshotInfo(snapshotId, indices, SnapshotState.SUCCESS, response.getState().getNodes().getMaxNodeVersion());
    }

    @Override
    public MetaData getSnapshotGlobalMetaData(SnapshotId snapshotId) {
        assert SNAPSHOT_ID.equals(snapshotId) : "RemoteClusterRepository only supports " + SNAPSHOT_ID + " as the SnapshotId";
        Client remoteClient = getRemoteClusterClient();
        // We set a single dummy index name to avoid fetching all the index data
        ClusterStateRequest clusterStateRequest = CcrRequests.metaDataRequest("dummy_index_name");
        ClusterStateResponse clusterState = remoteClient.admin().cluster().state(clusterStateRequest)
            .actionGet(ccrSettings.getRecoveryActionTimeout());
        return clusterState.getState().metaData();
    }

    @Override
    public IndexMetaData getSnapshotIndexMetaData(SnapshotId snapshotId, IndexId index) throws IOException {
        assert SNAPSHOT_ID.equals(snapshotId) : "RemoteClusterRepository only supports " + SNAPSHOT_ID + " as the SnapshotId";
        String leaderIndex = index.getName();
        Client remoteClient = getRemoteClusterClient();

        ClusterStateRequest clusterStateRequest = CcrRequests.metaDataRequest(leaderIndex);
        ClusterStateResponse clusterState = remoteClient.admin().cluster().state(clusterStateRequest)
            .actionGet(ccrSettings.getRecoveryActionTimeout());

        // Validates whether the leader cluster has been configured properly:
        PlainActionFuture<String[]> future = PlainActionFuture.newFuture();
        IndexMetaData leaderIndexMetaData = clusterState.getState().metaData().index(leaderIndex);
        ccrLicenseChecker.fetchLeaderHistoryUUIDs(remoteClient, leaderIndexMetaData, future::onFailure, future::onResponse);
        String[] leaderHistoryUUIDs = future.actionGet(ccrSettings.getRecoveryActionTimeout());

        IndexMetaData.Builder imdBuilder = IndexMetaData.builder(leaderIndex);
        // Adding the leader index uuid for each shard as custom metadata:
        Map<String, String> metadata = new HashMap<>();
        metadata.put(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_SHARD_HISTORY_UUIDS, String.join(",", leaderHistoryUUIDs));
        metadata.put(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_UUID_KEY, leaderIndexMetaData.getIndexUUID());
        metadata.put(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_NAME_KEY, leaderIndexMetaData.getIndex().getName());
        metadata.put(Ccr.CCR_CUSTOM_METADATA_REMOTE_CLUSTER_NAME_KEY, remoteClusterAlias);
        imdBuilder.putCustom(Ccr.CCR_CUSTOM_METADATA_KEY, metadata);

        imdBuilder.settings(leaderIndexMetaData.getSettings());

        // Copy mappings from leader IMD to follow IMD
        for (ObjectObjectCursor<String, MappingMetaData> cursor : leaderIndexMetaData.getMappings()) {
            imdBuilder.putMapping(cursor.value);
        }

        imdBuilder.setRoutingNumShards(leaderIndexMetaData.getRoutingNumShards());
        // We assert that insync allocation ids are not empty in `PrimaryShardAllocator`
        for (IntObjectCursor<Set<String>> entry : leaderIndexMetaData.getInSyncAllocationIds()) {
            imdBuilder.putInSyncAllocationIds(entry.key, Collections.singleton(IN_SYNC_ALLOCATION_ID));
        }

        return imdBuilder.build();
    }

    @Override
    public RepositoryData getRepositoryData() {
        Client remoteClient = getRemoteClusterClient();
        ClusterStateResponse response = remoteClient.admin().cluster().prepareState().clear().setMetaData(true)
            .get(ccrSettings.getRecoveryActionTimeout());
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
        return throttledTime.count();
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
        createEmptyStore(indexShard, shardId);

        final Map<String, String> ccrMetaData = indexShard.indexSettings().getIndexMetaData().getCustomData(Ccr.CCR_CUSTOM_METADATA_KEY);
        final String leaderIndexName = ccrMetaData.get(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_NAME_KEY);
        final String leaderUUID = ccrMetaData.get(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_UUID_KEY);
        final Index leaderIndex = new Index(leaderIndexName, leaderUUID);
        final ShardId leaderShardId = new ShardId(leaderIndex, shardId.getId());

        final Client remoteClient = getRemoteClusterClient();

        final String retentionLeaseId =
                retentionLeaseId(localClusterName, indexShard.shardId().getIndex(), remoteClusterAlias, leaderIndex);

        acquireRetentionLeaseOnLeader(shardId, retentionLeaseId, leaderShardId, remoteClient);

        // schedule renewals to run during the restore
        final Scheduler.Cancellable renewable = threadPool.scheduleWithFixedDelay(
                () -> {
                    logger.trace("{} background renewal of retention lease [{}] during restore", indexShard.shardId(), retentionLeaseId);
                    final ThreadContext threadContext = threadPool.getThreadContext();
                    try (ThreadContext.StoredContext ignore = threadContext.stashContext()) {
                        // we have to execute under the system context so that if security is enabled the renewal is authorized
                        threadContext.markAsSystemContext();
                        CcrRetentionLeases.asyncRenewRetentionLease(
                                leaderShardId,
                                retentionLeaseId,
                                remoteClient,
                                ActionListener.wrap(
                                        r -> {},
                                        e -> {
                                            assert e instanceof ElasticsearchSecurityException == false : e;
                                            logger.warn(new ParameterizedMessage(
                                                            "{} background renewal of retention lease [{}] failed during restore",
                                                            shardId,
                                                            retentionLeaseId),
                                                    e);
                                        }));
                    }
                },
                RETENTION_LEASE_RENEW_INTERVAL_SETTING.get(indexShard.indexSettings().getSettings()),
                Ccr.CCR_THREAD_POOL_NAME);

        // TODO: There should be some local timeout. And if the remote cluster returns an unknown session
        //  response, we should be able to retry by creating a new session.
        try (RestoreSession restoreSession = openSession(metadata.name(), remoteClient, leaderShardId, indexShard, recoveryState)) {
            restoreSession.restoreFiles();
            updateMappings(remoteClient, leaderIndex, restoreSession.mappingVersion, client, indexShard.routingEntry().index());
        } catch (Exception e) {
            throw new IndexShardRestoreFailedException(indexShard.shardId(), "failed to restore snapshot [" + snapshotId + "]", e);
        } finally {
            logger.trace("{} canceling background renewal of retention lease [{}] at the end of restore", shardId, retentionLeaseId);
            renewable.cancel();
        }
    }

    private void createEmptyStore(final IndexShard indexShard, final ShardId shardId) {
        final Store store = indexShard.store();
        store.incRef();
        try {
            store.createEmpty(indexShard.indexSettings().getIndexMetaData().getCreationVersion().luceneVersion);
        } catch (final EngineException | IOException e) {
            throw new IndexShardRecoveryException(shardId, "failed to create empty store", e);
        } finally {
            store.decRef();
        }
    }

    void acquireRetentionLeaseOnLeader(
            final ShardId shardId,
            final String retentionLeaseId,
            final ShardId leaderShardId,
            final Client remoteClient) {
        logger.trace(
                () -> new ParameterizedMessage("{} requesting leader to add retention lease [{}]", shardId, retentionLeaseId));
        final TimeValue timeout = ccrSettings.getRecoveryActionTimeout();
        final Optional<RetentionLeaseAlreadyExistsException> maybeAddAlready =
                syncAddRetentionLease(leaderShardId, retentionLeaseId, remoteClient, timeout);
        maybeAddAlready.ifPresent(addAlready -> {
            logger.trace(() -> new ParameterizedMessage(
                            "{} retention lease [{}] already exists, requesting a renewal",
                            shardId,
                            retentionLeaseId),
                    addAlready);
            final Optional<RetentionLeaseNotFoundException> maybeRenewNotFound =
                    syncRenewRetentionLease(leaderShardId, retentionLeaseId, remoteClient, timeout);
            maybeRenewNotFound.ifPresent(renewNotFound -> {
                logger.trace(() -> new ParameterizedMessage(
                                "{} retention lease [{}] not found while attempting to renew, requesting a final add",
                                shardId,
                                retentionLeaseId),
                        renewNotFound);
                final Optional<RetentionLeaseAlreadyExistsException> maybeFallbackAddAlready =
                        syncAddRetentionLease(leaderShardId, retentionLeaseId, remoteClient, timeout);
                maybeFallbackAddAlready.ifPresent(fallbackAddAlready -> {
                    /*
                     * At this point we tried to add the lease and the retention lease already existed. By the time we tried to renew the
                     * lease, it expired or was removed. We tried to add the lease again and it already exists? Bail.
                     */
                    assert false : fallbackAddAlready;
                    throw fallbackAddAlready;
                });
            });
        });
    }

    // this setting is intentionally not registered, it is only used in tests
    public static final Setting<TimeValue> RETENTION_LEASE_RENEW_INTERVAL_SETTING =
            Setting.timeSetting(
                    "index.ccr.retention_lease.renew_interval",
                    new TimeValue(5, TimeUnit.MINUTES),
                    new TimeValue(0, TimeUnit.MILLISECONDS),
                    Setting.Property.Dynamic,
                    Setting.Property.IndexScope);

    @Override
    public IndexShardSnapshotStatus getShardSnapshotStatus(SnapshotId snapshotId, Version version, IndexId indexId, ShardId leaderShardId) {
        throw new UnsupportedOperationException("Unsupported for repository of type: " + TYPE);
    }

    private void updateMappings(Client leaderClient, Index leaderIndex, long leaderMappingVersion,
                                Client followerClient, Index followerIndex) {
        final PlainActionFuture<IndexMetaData> indexMetadataFuture = new PlainActionFuture<>();
        final long startTimeInNanos = System.nanoTime();
        final Supplier<TimeValue> timeout = () -> {
            final long elapsedInNanos = System.nanoTime() - startTimeInNanos;
            return TimeValue.timeValueNanos(ccrSettings.getRecoveryActionTimeout().nanos() - elapsedInNanos);
        };
        CcrRequests.getIndexMetadata(leaderClient, leaderIndex, leaderMappingVersion, 0L, timeout, indexMetadataFuture);
        final IndexMetaData leaderIndexMetadata = indexMetadataFuture.actionGet(ccrSettings.getRecoveryActionTimeout());
        final MappingMetaData mappingMetaData = leaderIndexMetadata.mapping();
        if (mappingMetaData != null) {
            final PutMappingRequest putMappingRequest = CcrRequests.putMappingRequest(followerIndex.getName(), mappingMetaData)
                .masterNodeTimeout(TimeValue.timeValueMinutes(30));
            followerClient.admin().indices().putMapping(putMappingRequest).actionGet(ccrSettings.getRecoveryActionTimeout());
        }
    }

    RestoreSession openSession(String repositoryName, Client remoteClient, ShardId leaderShardId, IndexShard indexShard,
                                       RecoveryState recoveryState) {
        String sessionUUID = UUIDs.randomBase64UUID();
        PutCcrRestoreSessionAction.PutCcrRestoreSessionResponse response = remoteClient.execute(PutCcrRestoreSessionAction.INSTANCE,
            new PutCcrRestoreSessionRequest(sessionUUID, leaderShardId)).actionGet(ccrSettings.getRecoveryActionTimeout());
        return new RestoreSession(repositoryName, remoteClient, sessionUUID, response.getNode(), indexShard, recoveryState,
            response.getStoreFileMetaData(), response.getMappingVersion(), threadPool, ccrSettings, throttledTime::inc);
    }

    private static class RestoreSession extends FileRestoreContext implements Closeable {

        private final Client remoteClient;
        private final String sessionUUID;
        private final DiscoveryNode node;
        private final Store.MetadataSnapshot sourceMetaData;
        private final long mappingVersion;
        private final CcrSettings ccrSettings;
        private final LongConsumer throttleListener;
        private final ThreadPool threadPool;

        RestoreSession(String repositoryName, Client remoteClient, String sessionUUID, DiscoveryNode node, IndexShard indexShard,
                       RecoveryState recoveryState, Store.MetadataSnapshot sourceMetaData, long mappingVersion,
                       ThreadPool threadPool, CcrSettings ccrSettings, LongConsumer throttleListener) {
            super(repositoryName, indexShard, SNAPSHOT_ID, recoveryState, Math.toIntExact(ccrSettings.getChunkSize().getBytes()));
            this.remoteClient = remoteClient;
            this.sessionUUID = sessionUUID;
            this.node = node;
            this.sourceMetaData = sourceMetaData;
            this.mappingVersion = mappingVersion;
            this.threadPool = threadPool;
            this.ccrSettings = ccrSettings;
            this.throttleListener = throttleListener;
        }

        void restoreFiles() throws IOException {
            ArrayList<FileInfo> fileInfos = new ArrayList<>();
            for (StoreFileMetaData fileMetaData : sourceMetaData) {
                ByteSizeValue fileSize = new ByteSizeValue(fileMetaData.length());
                fileInfos.add(new FileInfo(fileMetaData.name(), fileMetaData, fileSize));
            }
            SnapshotFiles snapshotFiles = new SnapshotFiles(LATEST, fileInfos);
            restore(snapshotFiles);
        }

        @Override
        protected void restoreFiles(List<FileInfo> filesToRecover, Store store) throws IOException {
            logger.trace("[{}] starting CCR restore of {} files", shardId, filesToRecover);

            try (MultiFileWriter multiFileWriter = new MultiFileWriter(store, recoveryState.getIndex(), "", logger, () -> {
            })) {
                final LocalCheckpointTracker requestSeqIdTracker = new LocalCheckpointTracker(NO_OPS_PERFORMED, NO_OPS_PERFORMED);
                final AtomicReference<Tuple<StoreFileMetaData, Exception>> error = new AtomicReference<>();

                for (FileInfo fileInfo : filesToRecover) {
                    final long fileLength = fileInfo.length();
                    long offset = 0;
                    while (offset < fileLength && error.get() == null) {
                        final long requestSeqId = requestSeqIdTracker.generateSeqNo();
                        try {
                            requestSeqIdTracker.waitForOpsToComplete(requestSeqId - ccrSettings.getMaxConcurrentFileChunks());

                            if (error.get() != null) {
                                requestSeqIdTracker.markSeqNoAsCompleted(requestSeqId);
                                break;
                            }

                            final int bytesRequested = Math.toIntExact(
                                Math.min(ccrSettings.getChunkSize().getBytes(), fileLength - offset));
                            offset += bytesRequested;

                            final GetCcrRestoreFileChunkRequest request =
                                new GetCcrRestoreFileChunkRequest(node, sessionUUID, fileInfo.name(), bytesRequested);
                            logger.trace("[{}] [{}] fetching chunk for file [{}], expected offset: {}, size: {}", shardId, snapshotId,
                                fileInfo.name(), offset, bytesRequested);

                            TimeValue timeout = ccrSettings.getRecoveryActionTimeout();
                            ActionListener<GetCcrRestoreFileChunkAction.GetCcrRestoreFileChunkResponse> listener =
                                ListenerTimeouts.wrapWithTimeout(threadPool, ActionListener.wrap(
                                    r -> threadPool.generic().execute(new AbstractRunnable() {
                                        @Override
                                        public void onFailure(Exception e) {
                                            error.compareAndSet(null, Tuple.tuple(fileInfo.metadata(), e));
                                            requestSeqIdTracker.markSeqNoAsCompleted(requestSeqId);
                                        }

                                        @Override
                                        protected void doRun() throws Exception {
                                            final int actualChunkSize = r.getChunk().length();
                                            logger.trace("[{}] [{}] got response for file [{}], offset: {}, length: {}", shardId,
                                                snapshotId, fileInfo.name(), r.getOffset(), actualChunkSize);
                                            final long nanosPaused = ccrSettings.getRateLimiter().maybePause(actualChunkSize);
                                            throttleListener.accept(nanosPaused);
                                            final boolean lastChunk = r.getOffset() + actualChunkSize >= fileLength;
                                            multiFileWriter.writeFileChunk(fileInfo.metadata(), r.getOffset(), r.getChunk(), lastChunk);
                                            requestSeqIdTracker.markSeqNoAsCompleted(requestSeqId);
                                        }
                                    }),
                                    e -> {
                                        error.compareAndSet(null, Tuple.tuple(fileInfo.metadata(), e));
                                        requestSeqIdTracker.markSeqNoAsCompleted(requestSeqId);
                                    }
                                    ), timeout, ThreadPool.Names.GENERIC, GetCcrRestoreFileChunkAction.NAME);
                            remoteClient.execute(GetCcrRestoreFileChunkAction.INSTANCE, request, listener);
                        } catch (Exception e) {
                            error.compareAndSet(null, Tuple.tuple(fileInfo.metadata(), e));
                            requestSeqIdTracker.markSeqNoAsCompleted(requestSeqId);
                        }
                    }
                }

                try {
                    requestSeqIdTracker.waitForOpsToComplete(requestSeqIdTracker.getMaxSeqNo());
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                    throw new ElasticsearchException(e);
                }
                if (error.get() != null) {
                    handleError(store, error.get().v2());
                }
            }

            logger.trace("[{}] completed CCR restore", shardId);
        }

        private void handleError(Store store, Exception e) throws IOException {
            final IOException corruptIndexException;
            if ((corruptIndexException = ExceptionsHelper.unwrapCorruption(e)) != null) {
                try {
                    store.markStoreCorrupted(corruptIndexException);
                } catch (IOException ioe) {
                    logger.warn("store cannot be marked as corrupted", e);
                }
                throw corruptIndexException;
            } else {
                ExceptionsHelper.reThrowIfNotNull(e);
            }
        }

        @Override
        protected InputStream fileInputStream(FileInfo fileInfo) {
            throw new UnsupportedOperationException();
        }

        @Override
        public void close() {
            ClearCcrRestoreSessionRequest clearRequest = new ClearCcrRestoreSessionRequest(sessionUUID, node);
            ClearCcrRestoreSessionAction.ClearCcrRestoreSessionResponse response =
                remoteClient.execute(ClearCcrRestoreSessionAction.INSTANCE, clearRequest).actionGet(ccrSettings.getRecoveryActionTimeout());
        }
    }
}
