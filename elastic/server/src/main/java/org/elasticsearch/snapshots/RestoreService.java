/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.snapshots;

import com.carrotsearch.hppc.IntHashSet;
import com.carrotsearch.hppc.IntSet;
import com.carrotsearch.hppc.cursors.ObjectCursor;
import com.carrotsearch.hppc.cursors.ObjectObjectCursor;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.ActionRunnable;
import org.elasticsearch.action.StepListener;
import org.elasticsearch.action.admin.cluster.snapshots.restore.RestoreSnapshotRequest;
import org.elasticsearch.action.support.GroupedActionListener;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.cluster.ClusterChangedEvent;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateApplier;
import org.elasticsearch.cluster.ClusterStateUpdateTask;
import org.elasticsearch.cluster.RestoreInProgress;
import org.elasticsearch.cluster.RestoreInProgress.ShardRestoreStatus;
import org.elasticsearch.cluster.SnapshotDeletionsInProgress;
import org.elasticsearch.cluster.block.ClusterBlocks;
import org.elasticsearch.cluster.metadata.AliasMetadata;
import org.elasticsearch.cluster.metadata.DataStream;
import org.elasticsearch.cluster.metadata.DataStreamMetadata;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.metadata.IndexMetadataVerifier;
import org.elasticsearch.cluster.metadata.IndexTemplateMetadata;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.metadata.MetadataCreateIndexService;
import org.elasticsearch.cluster.metadata.MetadataDeleteIndexService;
import org.elasticsearch.cluster.metadata.MetadataIndexStateService;
import org.elasticsearch.cluster.metadata.RepositoriesMetadata;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.routing.RecoverySource;
import org.elasticsearch.cluster.routing.RecoverySource.SnapshotRecoverySource;
import org.elasticsearch.cluster.routing.RoutingChangesObserver;
import org.elasticsearch.cluster.routing.RoutingTable;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.cluster.routing.UnassignedInfo;
import org.elasticsearch.cluster.routing.allocation.AllocationService;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.CheckedConsumer;
import org.elasticsearch.common.Priority;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.collect.ImmutableOpenMap;
import org.elasticsearch.common.logging.DeprecationCategory;
import org.elasticsearch.common.logging.DeprecationLogger;
import org.elasticsearch.common.lucene.Lucene;
import org.elasticsearch.common.regex.Regex;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.Index;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.shard.IndexLongFieldRange;
import org.elasticsearch.index.shard.IndexShard;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.indices.ShardLimitValidator;
import org.elasticsearch.indices.SystemIndices;
import org.elasticsearch.repositories.IndexId;
import org.elasticsearch.repositories.RepositoriesService;
import org.elasticsearch.repositories.Repository;
import org.elasticsearch.repositories.RepositoryData;
import org.elasticsearch.repositories.blobstore.BlobStoreRepository;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;
import java.util.Set;
import java.util.function.BiConsumer;
import java.util.function.Function;
import java.util.function.Predicate;
import java.util.stream.Collectors;
import java.util.stream.Stream;

import static java.util.Collections.unmodifiableSet;
import static org.elasticsearch.cluster.metadata.IndexMetadata.SETTING_AUTO_EXPAND_REPLICAS;
import static org.elasticsearch.cluster.metadata.IndexMetadata.SETTING_CREATION_DATE;
import static org.elasticsearch.cluster.metadata.IndexMetadata.SETTING_HISTORY_UUID;
import static org.elasticsearch.cluster.metadata.IndexMetadata.SETTING_INDEX_UUID;
import static org.elasticsearch.cluster.metadata.IndexMetadata.SETTING_NUMBER_OF_REPLICAS;
import static org.elasticsearch.cluster.metadata.IndexMetadata.SETTING_NUMBER_OF_SHARDS;
import static org.elasticsearch.cluster.metadata.IndexMetadata.SETTING_VERSION_CREATED;
import static org.elasticsearch.common.util.set.Sets.newHashSet;
import static org.elasticsearch.snapshots.SnapshotUtils.filterIndices;
import static org.elasticsearch.snapshots.SnapshotsService.NO_FEATURE_STATES_VALUE;

/**
 * Service responsible for restoring snapshots
 * <p>
 * Restore operation is performed in several stages.
 * <p>
 * First {@link #restoreSnapshot(RestoreSnapshotRequest, org.elasticsearch.action.ActionListener)}
 * method reads information about snapshot and metadata from repository. In update cluster state task it checks restore
 * preconditions, restores global state if needed, creates {@link RestoreInProgress} record with list of shards that needs
 * to be restored and adds this shard to the routing table using
 * {@link RoutingTable.Builder#addAsRestore(IndexMetadata, SnapshotRecoverySource)} method.
 * <p>
 * Individual shards are getting restored as part of normal recovery process in
 * {@link IndexShard#restoreFromRepository} )}
 * method, which detects that shard should be restored from snapshot rather than recovered from gateway by looking
 * at the {@link ShardRouting#recoverySource()} property.
 * <p>
 * At the end of the successful restore process {@code RestoreService} calls {@link #removeCompletedRestoresFromClusterState()},
 * which removes {@link RestoreInProgress} when all shards are completed. In case of
 * restore failure a normal recovery fail-over process kicks in.
 */
public class RestoreService implements ClusterStateApplier {

    private static final Logger logger = LogManager.getLogger(RestoreService.class);
    private static final DeprecationLogger deprecationLogger = DeprecationLogger.getLogger(RestoreService.class);

    public static final Setting<Boolean> REFRESH_REPO_UUID_ON_RESTORE_SETTING = Setting.boolSetting(
            "snapshot.refresh_repo_uuid_on_restore",
            true,
            Setting.Property.NodeScope,
            Setting.Property.Dynamic);

    private static final Set<String> UNMODIFIABLE_SETTINGS = unmodifiableSet(newHashSet(
            SETTING_NUMBER_OF_SHARDS,
            SETTING_VERSION_CREATED,
            SETTING_INDEX_UUID,
            SETTING_CREATION_DATE,
            SETTING_HISTORY_UUID));

    // It's OK to change some settings, but we shouldn't allow simply removing them
    private static final Set<String> UNREMOVABLE_SETTINGS;

    static {
        Set<String> unremovable = new HashSet<>(UNMODIFIABLE_SETTINGS.size() + 4);
        unremovable.addAll(UNMODIFIABLE_SETTINGS);
        unremovable.add(SETTING_NUMBER_OF_REPLICAS);
        unremovable.add(SETTING_AUTO_EXPAND_REPLICAS);
        UNREMOVABLE_SETTINGS = unmodifiableSet(unremovable);
    }

    private final ClusterService clusterService;

    private final RepositoriesService repositoriesService;

    private final AllocationService allocationService;

    private final MetadataCreateIndexService createIndexService;

    private final IndexMetadataVerifier indexMetadataVerifier;

    private final MetadataDeleteIndexService metadataDeleteIndexService;

    private final ShardLimitValidator shardLimitValidator;

    private final ClusterSettings clusterSettings;

    private final SystemIndices systemIndices;

    private volatile boolean refreshRepositoryUuidOnRestore;

    public RestoreService(
        ClusterService clusterService,
        RepositoriesService repositoriesService,
        AllocationService allocationService,
        MetadataCreateIndexService createIndexService,
        MetadataDeleteIndexService metadataDeleteIndexService,
        IndexMetadataVerifier indexMetadataVerifier,
        ShardLimitValidator shardLimitValidator,
        SystemIndices systemIndices
    ) {
        this.clusterService = clusterService;
        this.repositoriesService = repositoriesService;
        this.allocationService = allocationService;
        this.createIndexService = createIndexService;
        this.indexMetadataVerifier = indexMetadataVerifier;
        this.metadataDeleteIndexService = metadataDeleteIndexService;
        if (DiscoveryNode.isMasterNode(clusterService.getSettings())) {
            clusterService.addStateApplier(this);
        }
        this.clusterSettings = clusterService.getClusterSettings();
        this.shardLimitValidator = shardLimitValidator;
        this.systemIndices = systemIndices;
        this.refreshRepositoryUuidOnRestore = REFRESH_REPO_UUID_ON_RESTORE_SETTING.get(clusterService.getSettings());
        clusterService.getClusterSettings().addSettingsUpdateConsumer(
                REFRESH_REPO_UUID_ON_RESTORE_SETTING,
                this::setRefreshRepositoryUuidOnRestore);
    }

    /**
     * Restores snapshot specified in the restore request.
     *
     * @param request  restore request
     * @param listener restore listener
     */
    public void restoreSnapshot(final RestoreSnapshotRequest request, final ActionListener<RestoreCompletionResponse> listener) {
        restoreSnapshot(request, listener, (clusterState, builder) -> {});
    }

    /**
     * Restores snapshot specified in the restore request.
     *
     * @param request  restore request
     * @param listener restore listener
     * @param updater  handler that allows callers to make modifications to {@link Metadata}
     *                 in the same cluster state update as the restore operation
     */
    public void restoreSnapshot(final RestoreSnapshotRequest request,
                                final ActionListener<RestoreCompletionResponse> listener,
                                final BiConsumer<ClusterState, Metadata.Builder> updater) {
        try {

            // Try and fill in any missing repository UUIDs in case they're needed during the restore
            final StepListener<Void> repositoryUuidRefreshListener = new StepListener<>();
            refreshRepositoryUuids(refreshRepositoryUuidOnRestore, repositoriesService, repositoryUuidRefreshListener);

            // Read snapshot info and metadata from the repository
            final String repositoryName = request.repository();
            Repository repository = repositoriesService.repository(repositoryName);
            final StepListener<RepositoryData> repositoryDataListener = new StepListener<>();
            repository.getRepositoryData(repositoryDataListener);

            final CheckedConsumer<RepositoryData, IOException> onRepositoryDataReceived = repositoryData -> {
                final String snapshotName = request.snapshot();
                final Optional<SnapshotId> matchingSnapshotId = repositoryData.getSnapshotIds().stream()
                    .filter(s -> snapshotName.equals(s.getName())).findFirst();
                if (matchingSnapshotId.isPresent() == false) {
                    throw new SnapshotRestoreException(repositoryName, snapshotName, "snapshot does not exist");
                }

                final SnapshotId snapshotId = matchingSnapshotId.get();
                if (request.snapshotUuid() != null && request.snapshotUuid().equals(snapshotId.getUUID()) == false) {
                    throw new SnapshotRestoreException(repositoryName, snapshotName,
                        "snapshot UUID mismatch: expected [" + request.snapshotUuid() + "] but got [" + snapshotId.getUUID() + "]");
                }

                final SnapshotInfo snapshotInfo = repository.getSnapshotInfo(snapshotId);
                final Snapshot snapshot = new Snapshot(repositoryName, snapshotId);

                // Make sure that we can restore from this snapshot
                validateSnapshotRestorable(repositoryName, snapshotInfo);

                // Get the global state if necessary
                Metadata globalMetadata = null;
                final Metadata.Builder metadataBuilder;
                if (request.includeGlobalState()) {
                    globalMetadata = repository.getSnapshotGlobalMetadata(snapshotId);
                    metadataBuilder = Metadata.builder(globalMetadata);
                } else {
                    metadataBuilder = Metadata.builder();
                }

                List<String> requestIndices = new ArrayList<>(Arrays.asList(request.indices()));

                // Get data stream metadata for requested data streams
                Map<String, DataStream> dataStreamsToRestore = getDataStreamsToRestore(repository, snapshotId, snapshotInfo, globalMetadata,
                    requestIndices);

                // Remove the data streams from the list of requested indices
                requestIndices.removeAll(dataStreamsToRestore.keySet());

                // And add the backing indices
                Set<String> dataStreamIndices = dataStreamsToRestore.values().stream()
                    .flatMap(ds -> ds.getIndices().stream())
                    .map(Index::getName)
                    .collect(Collectors.toSet());
                requestIndices.addAll(dataStreamIndices);

                // Determine system indices to restore from requested feature states
                final Map<String, List<String>> featureStatesToRestore = getFeatureStatesToRestore(request, snapshotInfo, snapshot);
                final Set<String> featureStateIndices = featureStatesToRestore.values().stream()
                    .flatMap(Collection::stream)
                    .collect(Collectors.toSet());

                // Resolve the indices that were directly requested
                final List<String> requestedIndicesInSnapshot = filterIndices(snapshotInfo.indices(), requestIndices.toArray(String[]::new),
                    request.indicesOptions());

                // Combine into the final list of indices to be restored
                final List<String> requestedIndicesIncludingSystem = Stream.concat(
                    requestedIndicesInSnapshot.stream(),
                    featureStateIndices.stream()
                ).distinct().collect(Collectors.toList());

                final Set<String> explicitlyRequestedSystemIndices = new HashSet<>();
                final List<IndexId> indexIdsInSnapshot = repositoryData.resolveIndices(requestedIndicesIncludingSystem);
                for (IndexId indexId : indexIdsInSnapshot) {
                    IndexMetadata snapshotIndexMetaData = repository.getSnapshotIndexMetaData(repositoryData, snapshotId, indexId);
                    if (snapshotIndexMetaData.isSystem()) {
                        if (requestedIndicesInSnapshot.contains(indexId.getName())) {
                            explicitlyRequestedSystemIndices.add(indexId.getName());
                        }
                    }
                    metadataBuilder.put(snapshotIndexMetaData, false);
                }

                // log a deprecation warning if the any of the indexes to delete were included in the request and the snapshot
                // is from a version that should have feature states
                if (snapshotInfo.version().onOrAfter(Version.V_7_12_0) && explicitlyRequestedSystemIndices.isEmpty() == false) {
                    deprecationLogger.deprecate(DeprecationCategory.API, "restore-system-index-from-snapshot",
                        "Restoring system indices by name is deprecated. Use feature states instead. System indices: "
                            + explicitlyRequestedSystemIndices);
                }

                final Metadata metadata = metadataBuilder.dataStreams(dataStreamsToRestore).build();

                // Apply renaming on index names, returning a map of names where
                // the key is the renamed index and the value is the original name
                final Map<String, String> indices = renamedIndices(request, requestedIndicesIncludingSystem, dataStreamIndices,
                    featureStateIndices);

                // Now we can start the actual restore process by adding shards to be recovered in the cluster state
                // and updating cluster metadata (global and index) as needed
                clusterService.submitStateUpdateTask(
                        "restore_snapshot[" + snapshotName + ']', new ClusterStateUpdateTask(request.masterNodeTimeout()) {
                    final String restoreUUID = UUIDs.randomBase64UUID();
                    RestoreInfo restoreInfo = null;

                    @Override
                    public ClusterState execute(ClusterState currentState) {
                        // Check if the snapshot to restore is currently being deleted
                        SnapshotDeletionsInProgress deletionsInProgress =
                            currentState.custom(SnapshotDeletionsInProgress.TYPE, SnapshotDeletionsInProgress.EMPTY);
                        if (deletionsInProgress.getEntries().stream().anyMatch(entry -> entry.getSnapshots().contains(snapshotId))) {
                            throw new ConcurrentSnapshotExecutionException(snapshot,
                                "cannot restore a snapshot while a snapshot deletion is in-progress [" +
                                    deletionsInProgress.getEntries().get(0) + "]");
                        }

                        // Clear out all existing indices which fall within a system index pattern being restored
                        final Set<Index> systemIndicesToDelete = resolveSystemIndicesToDelete(
                            currentState,
                            featureStatesToRestore.keySet()
                        );
                        currentState = metadataDeleteIndexService.deleteIndices(currentState, systemIndicesToDelete);

                        // Updating cluster state
                        ClusterState.Builder builder = ClusterState.builder(currentState);
                        Metadata.Builder mdBuilder = Metadata.builder(currentState.metadata());
                        ClusterBlocks.Builder blocks = ClusterBlocks.builder().blocks(currentState.blocks());
                        RoutingTable.Builder rtBuilder = RoutingTable.builder(currentState.routingTable());
                        ImmutableOpenMap<ShardId, RestoreInProgress.ShardRestoreStatus> shards;
                        Set<String> aliases = new HashSet<>();

                        if (indices.isEmpty() == false) {
                            // We have some indices to restore
                            ImmutableOpenMap.Builder<ShardId, RestoreInProgress.ShardRestoreStatus> shardsBuilder =
                                ImmutableOpenMap.builder();
                            final Version minIndexCompatibilityVersion = currentState.getNodes().getMaxNodeVersion()
                                .minimumIndexCompatibilityVersion();
                            for (Map.Entry<String, String> indexEntry : indices.entrySet()) {
                                String index = indexEntry.getValue();
                                boolean partial = checkPartial(index);
                                SnapshotRecoverySource recoverySource = new SnapshotRecoverySource(restoreUUID, snapshot,
                                    snapshotInfo.version(), repositoryData.resolveIndexId(index));
                                String renamedIndexName = indexEntry.getKey();
                                IndexMetadata snapshotIndexMetadata = metadata.index(index);
                                snapshotIndexMetadata = updateIndexSettings(snapshotIndexMetadata,
                                    request.indexSettings(), request.ignoreIndexSettings());
                                try {
                                    snapshotIndexMetadata = indexMetadataVerifier.verifyIndexMetadata(snapshotIndexMetadata,
                                        minIndexCompatibilityVersion);
                                } catch (Exception ex) {
                                    throw new SnapshotRestoreException(snapshot, "cannot restore index [" + index +
                                        "] because it cannot be upgraded", ex);
                                }
                                // Check that the index is closed or doesn't exist
                                IndexMetadata currentIndexMetadata = currentState.metadata().index(renamedIndexName);
                                IntSet ignoreShards = new IntHashSet();
                                final Index renamedIndex;
                                if (currentIndexMetadata == null) {
                                    // Index doesn't exist - create it and start recovery
                                    // Make sure that the index we are about to create has a validate name
                                    boolean isHidden = IndexMetadata.INDEX_HIDDEN_SETTING.get(snapshotIndexMetadata.getSettings());
                                    createIndexService.validateIndexName(renamedIndexName, currentState);
                                    createIndexService.validateDotIndex(renamedIndexName, isHidden);
                                    createIndexService.validateIndexSettings(renamedIndexName, snapshotIndexMetadata.getSettings(), false);
                                    IndexMetadata.Builder indexMdBuilder = IndexMetadata.builder(snapshotIndexMetadata)
                                        .state(IndexMetadata.State.OPEN)
                                        .index(renamedIndexName);
                                    indexMdBuilder.settings(Settings.builder()
                                        .put(snapshotIndexMetadata.getSettings())
                                        .put(IndexMetadata.SETTING_INDEX_UUID, UUIDs.randomBase64UUID()))
                                        .timestampRange(IndexLongFieldRange.NO_SHARDS);
                                    shardLimitValidator.validateShardLimit(snapshotIndexMetadata.getSettings(), currentState);
                                    if (request.includeAliases() == false && snapshotIndexMetadata.getAliases().isEmpty() == false
                                            && isSystemIndex(snapshotIndexMetadata) == false) {
                                        // Remove all aliases - they shouldn't be restored
                                        indexMdBuilder.removeAllAliases();
                                    } else {
                                        for (ObjectCursor<String> alias : snapshotIndexMetadata.getAliases().keys()) {
                                            aliases.add(alias.value);
                                        }
                                    }
                                    IndexMetadata updatedIndexMetadata = indexMdBuilder.build();
                                    if (partial) {
                                        populateIgnoredShards(index, ignoreShards);
                                    }
                                    rtBuilder.addAsNewRestore(updatedIndexMetadata, recoverySource, ignoreShards);
                                    blocks.addBlocks(updatedIndexMetadata);
                                    mdBuilder.put(updatedIndexMetadata, true);
                                    renamedIndex = updatedIndexMetadata.getIndex();
                                } else {
                                    validateExistingIndex(currentIndexMetadata, snapshotIndexMetadata, renamedIndexName, partial);
                                    // Index exists and it's closed - open it in metadata and start recovery
                                    IndexMetadata.Builder indexMdBuilder =
                                        IndexMetadata.builder(snapshotIndexMetadata).state(IndexMetadata.State.OPEN);
                                    indexMdBuilder.version(
                                        Math.max(snapshotIndexMetadata.getVersion(), 1 + currentIndexMetadata.getVersion()));
                                    indexMdBuilder.mappingVersion(
                                        Math.max(snapshotIndexMetadata.getMappingVersion(), 1 + currentIndexMetadata.getMappingVersion()));
                                    indexMdBuilder.settingsVersion(
                                        Math.max(
                                            snapshotIndexMetadata.getSettingsVersion(),
                                            1 + currentIndexMetadata.getSettingsVersion()));
                                    indexMdBuilder.aliasesVersion(
                                        Math.max(snapshotIndexMetadata.getAliasesVersion(), 1 + currentIndexMetadata.getAliasesVersion()));
                                    indexMdBuilder.timestampRange(IndexLongFieldRange.NO_SHARDS);

                                    for (int shard = 0; shard < snapshotIndexMetadata.getNumberOfShards(); shard++) {
                                        indexMdBuilder.primaryTerm(shard,
                                            Math.max(snapshotIndexMetadata.primaryTerm(shard), currentIndexMetadata.primaryTerm(shard)));
                                    }

                                    if (request.includeAliases() == false && isSystemIndex(snapshotIndexMetadata) == false) {
                                        // Remove all snapshot aliases
                                        if (snapshotIndexMetadata.getAliases().isEmpty() == false) {
                                            indexMdBuilder.removeAllAliases();
                                        }
                                        /// Add existing aliases
                                        for (ObjectCursor<AliasMetadata> alias : currentIndexMetadata.getAliases().values()) {
                                            indexMdBuilder.putAlias(alias.value);
                                        }
                                    } else {
                                        for (ObjectCursor<String> alias : snapshotIndexMetadata.getAliases().keys()) {
                                            aliases.add(alias.value);
                                        }
                                    }
                                    indexMdBuilder.settings(Settings.builder()
                                        .put(snapshotIndexMetadata.getSettings())
                                        .put(IndexMetadata.SETTING_INDEX_UUID, currentIndexMetadata.getIndexUUID())
                                        .put(IndexMetadata.SETTING_HISTORY_UUID, UUIDs.randomBase64UUID()));
                                    IndexMetadata updatedIndexMetadata = indexMdBuilder.index(renamedIndexName).build();
                                    rtBuilder.addAsRestore(updatedIndexMetadata, recoverySource);
                                    blocks.updateBlocks(updatedIndexMetadata);
                                    mdBuilder.put(updatedIndexMetadata, true);
                                    renamedIndex = updatedIndexMetadata.getIndex();
                                }

                                for (int shard = 0; shard < snapshotIndexMetadata.getNumberOfShards(); shard++) {
                                    if (ignoreShards.contains(shard) == false) {
                                        shardsBuilder.put(new ShardId(renamedIndex, shard),
                                            new RestoreInProgress.ShardRestoreStatus(clusterService.state().nodes().getLocalNodeId()));
                                    } else {
                                        shardsBuilder.put(new ShardId(renamedIndex, shard),
                                            new RestoreInProgress.ShardRestoreStatus(clusterService.state().nodes().getLocalNodeId(),
                                                RestoreInProgress.State.FAILURE));
                                    }
                                }
                            }

                            shards = shardsBuilder.build();
                            RestoreInProgress.Entry restoreEntry = new RestoreInProgress.Entry(
                                restoreUUID, snapshot, overallState(RestoreInProgress.State.INIT, shards),
                                List.copyOf(indices.keySet()),
                                shards
                            );
                            builder.putCustom(RestoreInProgress.TYPE, new RestoreInProgress.Builder(
                                currentState.custom(RestoreInProgress.TYPE, RestoreInProgress.EMPTY)).add(restoreEntry).build());
                        } else {
                            shards = ImmutableOpenMap.of();
                        }

                        checkAliasNameConflicts(indices, aliases);

                        Map<String, DataStream> updatedDataStreams = new HashMap<>(currentState.metadata().dataStreams());
                        updatedDataStreams.putAll(dataStreamsToRestore.values().stream()
                            .map(ds -> updateDataStream(ds, mdBuilder, request))
                            .collect(Collectors.toMap(DataStream::getName, Function.identity())));
                        mdBuilder.dataStreams(updatedDataStreams);

                        // Restore global state if needed
                        if (request.includeGlobalState()) {
                            if (metadata.persistentSettings() != null) {
                                Settings settings = metadata.persistentSettings();
                                if (request.skipOperatorOnlyState()) {
                                    // Skip any operator-only settings from the snapshot. This happens when operator privileges are enabled
                                    Set<String> operatorSettingKeys = Stream.concat(
                                        settings.keySet().stream(), currentState.metadata().persistentSettings().keySet().stream())
                                        .filter(k -> {
                                            final Setting<?> setting = clusterSettings.get(k);
                                            return setting != null && setting.isOperatorOnly();
                                        })
                                        .collect(Collectors.toSet());
                                    if (false == operatorSettingKeys.isEmpty()) {
                                        settings = Settings.builder()
                                            .put(settings.filter(k -> false == operatorSettingKeys.contains(k)))
                                            .put(currentState.metadata().persistentSettings().filter(operatorSettingKeys::contains))
                                            .build();
                                    }
                                }
                                clusterSettings.validateUpdate(settings);
                                mdBuilder.persistentSettings(settings);
                            }
                            if (metadata.templates() != null) {
                                // TODO: Should all existing templates be deleted first?
                                for (ObjectCursor<IndexTemplateMetadata> cursor : metadata.templates().values()) {
                                    mdBuilder.put(cursor.value);
                                }
                            }
                            if (metadata.customs() != null) {
                                for (ObjectObjectCursor<String, Metadata.Custom> cursor : metadata.customs()) {
                                    if (RepositoriesMetadata.TYPE.equals(cursor.key) == false
                                            && DataStreamMetadata.TYPE.equals(cursor.key) == false
                                            && cursor.value instanceof Metadata.NonRestorableCustom == false) {
                                        // TODO: Check request.skipOperatorOnly for Autoscaling policies (NonRestorableCustom)
                                        // Don't restore repositories while we are working with them
                                        // TODO: Should we restore them at the end?
                                        // Also, don't restore data streams here, we already added them to the metadata builder above
                                        mdBuilder.putCustom(cursor.key, cursor.value);
                                    }
                                }
                            }
                        }

                        if (completed(shards)) {
                            // We don't have any indices to restore - we are done
                            restoreInfo = new RestoreInfo(snapshotId.getName(),
                                List.copyOf(indices.keySet()),
                                shards.size(),
                                shards.size() - failedShards(shards));
                        }

                        RoutingTable rt = rtBuilder.build();
                        updater.accept(currentState, mdBuilder);
                        ClusterState updatedState = builder.metadata(mdBuilder).blocks(blocks).routingTable(rt).build();
                        return allocationService.reroute(updatedState, "restored snapshot [" + snapshot + "]");
                    }

                    private void checkAliasNameConflicts(Map<String, String> renamedIndices, Set<String> aliases) {
                        for (Map.Entry<String, String> renamedIndex : renamedIndices.entrySet()) {
                            if (aliases.contains(renamedIndex.getKey())) {
                                throw new SnapshotRestoreException(snapshot,
                                    "cannot rename index [" + renamedIndex.getValue() + "] into [" + renamedIndex.getKey()
                                        + "] because of conflict with an alias with the same name");
                            }
                        }
                    }

                    private void populateIgnoredShards(String index, IntSet ignoreShards) {
                        for (SnapshotShardFailure failure : snapshotInfo.shardFailures()) {
                            if (index.equals(failure.index())) {
                                ignoreShards.add(failure.shardId());
                            }
                        }
                    }

                    private boolean checkPartial(String index) {
                        // Make sure that index was fully snapshotted
                        if (failed(snapshotInfo, index)) {
                            if (request.partial()) {
                                return true;
                            } else {
                                throw new SnapshotRestoreException(snapshot, "index [" + index + "] wasn't fully snapshotted - cannot " +
                                    "restore");
                            }
                        } else {
                            return false;
                        }
                    }

                    private void validateExistingIndex(IndexMetadata currentIndexMetadata, IndexMetadata snapshotIndexMetadata,
                                                       String renamedIndex, boolean partial) {
                        // Index exist - checking that it's closed
                        if (currentIndexMetadata.getState() != IndexMetadata.State.CLOSE) {
                            // TODO: Enable restore for open indices
                            throw new SnapshotRestoreException(snapshot, "cannot restore index [" + renamedIndex
                                + "] because an open index " +
                                "with same name already exists in the cluster. Either close or delete the existing index or restore the " +
                                "index under a different name by providing a rename pattern and replacement name");
                        }
                        // Index exist - checking if it's partial restore
                        if (partial) {
                            throw new SnapshotRestoreException(snapshot, "cannot restore partial index [" + renamedIndex
                                + "] because such index already exists");
                        }
                        // Make sure that the number of shards is the same. That's the only thing that we cannot change
                        if (currentIndexMetadata.getNumberOfShards() != snapshotIndexMetadata.getNumberOfShards()) {
                            throw new SnapshotRestoreException(snapshot,
                                "cannot restore index [" + renamedIndex + "] with [" + currentIndexMetadata.getNumberOfShards()
                                    + "] shards from a snapshot of index [" + snapshotIndexMetadata.getIndex().getName() + "] with [" +
                                    snapshotIndexMetadata.getNumberOfShards() + "] shards");
                        }
                    }

                    /**
                     * Optionally updates index settings in indexMetadata by removing settings listed in ignoreSettings and
                     * merging them with settings in changeSettings.
                     */
                    private IndexMetadata updateIndexSettings(IndexMetadata indexMetadata, Settings changeSettings,
                                                              String[] ignoreSettings) {
                        Settings normalizedChangeSettings = Settings.builder()
                            .put(changeSettings)
                            .normalizePrefix(IndexMetadata.INDEX_SETTING_PREFIX)
                            .build();
                        if (IndexSettings.INDEX_SOFT_DELETES_SETTING.get(indexMetadata.getSettings()) &&
                            IndexSettings.INDEX_SOFT_DELETES_SETTING.exists(changeSettings) &&
                            IndexSettings.INDEX_SOFT_DELETES_SETTING.get(changeSettings) == false) {
                            throw new SnapshotRestoreException(snapshot,
                                "cannot disable setting [" + IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey() + "] on restore");
                        }
                        IndexMetadata.Builder builder = IndexMetadata.builder(indexMetadata);
                        Settings settings = indexMetadata.getSettings();
                        Set<String> keyFilters = new HashSet<>();
                        List<String> simpleMatchPatterns = new ArrayList<>();
                        for (String ignoredSetting : ignoreSettings) {
                            if (Regex.isSimpleMatchPattern(ignoredSetting) == false) {
                                if (UNREMOVABLE_SETTINGS.contains(ignoredSetting)) {
                                    throw new SnapshotRestoreException(
                                        snapshot, "cannot remove setting [" + ignoredSetting + "] on restore");
                                } else {
                                    keyFilters.add(ignoredSetting);
                                }
                            } else {
                                simpleMatchPatterns.add(ignoredSetting);
                            }
                        }
                        Predicate<String> settingsFilter = k -> {
                            if (UNREMOVABLE_SETTINGS.contains(k) == false) {
                                for (String filterKey : keyFilters) {
                                    if (k.equals(filterKey)) {
                                        return false;
                                    }
                                }
                                for (String pattern : simpleMatchPatterns) {
                                    if (Regex.simpleMatch(pattern, k)) {
                                        return false;
                                    }
                                }
                            }
                            return true;
                        };
                        Settings.Builder settingsBuilder = Settings.builder()
                            .put(settings.filter(settingsFilter))
                            .put(normalizedChangeSettings.filter(k -> {
                                if (UNMODIFIABLE_SETTINGS.contains(k)) {
                                    throw new SnapshotRestoreException(snapshot, "cannot modify setting [" + k + "] on restore");
                                } else {
                                    return true;
                                }
                            }));
                        settingsBuilder.remove(MetadataIndexStateService.VERIFIED_BEFORE_CLOSE_SETTING.getKey());
                        return builder.settings(settingsBuilder).build();
                    }

                    @Override
                    public void onFailure(String source, Exception e) {
                        logger.warn(() -> new ParameterizedMessage("[{}] failed to restore snapshot", snapshotId), e);
                        listener.onFailure(e);
                    }

                    @Override
                    public void clusterStateProcessed(String source, ClusterState oldState, ClusterState newState) {
                        listener.onResponse(new RestoreCompletionResponse(restoreUUID, snapshot, restoreInfo));
                    }
                });
            };

            // fork handling the above consumer to the generic pool since it loads various pieces of metadata from the repository over a
            // longer period of time
            repositoryDataListener.whenComplete(repositoryData -> repositoryUuidRefreshListener.whenComplete(ignored ->
                    clusterService.getClusterApplierService().threadPool().generic().execute(
                            ActionRunnable.wrap(listener, l -> onRepositoryDataReceived.accept(repositoryData))
                    ), listener::onFailure), listener::onFailure);

        } catch (Exception e) {
            logger.warn(() -> new ParameterizedMessage("[{}] failed to restore snapshot",
                request.repository() + ":" + request.snapshot()), e);
            listener.onFailure(e);
        }
    }

    private void setRefreshRepositoryUuidOnRestore(boolean refreshRepositoryUuidOnRestore) {
        this.refreshRepositoryUuidOnRestore = refreshRepositoryUuidOnRestore;
    }

    /**
     * Best-effort attempt to make sure that we know all the repository UUIDs. Calls {@link Repository#getRepositoryData} on every
     * {@link BlobStoreRepository} with a missing UUID.
     *
     * @param enabled If {@code false} this method completes the listener immediately
     * @param repositoriesService Supplies the repositories to check
     * @param refreshListener Listener that is completed when all repositories have been refreshed.
     */
    // Exposed for tests
    static void refreshRepositoryUuids(boolean enabled, RepositoriesService repositoriesService, ActionListener<Void> refreshListener) {

        if (enabled == false) {
            logger.debug("repository UUID refresh is disabled");
            refreshListener.onResponse(null);
            return;
        }

        // We only care about BlobStoreRepositories because they're the only ones that can contain a searchable snapshot, and we only care
        // about ones with missing UUIDs. It's possible to have the UUID change from under us if, e.g., the repository was wiped by an
        // external force, but in this case any searchable snapshots are lost anyway so it doesn't really matter.
        final List<Repository> repositories = repositoriesService.getRepositories().values().stream()
                .filter(repository -> repository instanceof BlobStoreRepository
                        && repository.getMetadata().uuid().equals(RepositoryData.MISSING_UUID)).collect(Collectors.toList());
        if (repositories.isEmpty()) {
            logger.debug("repository UUID refresh is not required");
            refreshListener.onResponse(null);
            return;
        }

        logger.info("refreshing repository UUIDs for repositories [{}]",
                repositories.stream().map(repository -> repository.getMetadata().name()).collect(Collectors.joining(",")));
        final ActionListener<RepositoryData> groupListener = new GroupedActionListener<>(new ActionListener<Collection<Void>>() {
            @Override
            public void onResponse(Collection<Void> ignored) {
                logger.debug("repository UUID refresh completed");
                refreshListener.onResponse(null);
            }

            @Override
            public void onFailure(Exception e) {
                logger.debug("repository UUID refresh failed", e);
                refreshListener.onResponse(null); // this refresh is best-effort, the restore should proceed either way
            }
        }, repositories.size()).map(repositoryData -> null /* don't collect the RepositoryData */);

        for (Repository repository : repositories) {
            repository.getRepositoryData(groupListener);
        }

    }

    private boolean isSystemIndex(IndexMetadata indexMetadata) {
        return indexMetadata.isSystem() || systemIndices.isSystemName(indexMetadata.getIndex().getName());
    }

    private Map<String, DataStream> getDataStreamsToRestore(Repository repository, SnapshotId snapshotId, SnapshotInfo snapshotInfo,
                                                           Metadata globalMetadata, List<String> requestIndices) {
        Map<String, DataStream> dataStreams;
        List<String> requestedDataStreams = filterIndices(snapshotInfo.dataStreams(), requestIndices.toArray(String[]::new),
            IndicesOptions.fromOptions(true, true, true, true));
        if (requestedDataStreams.isEmpty()) {
            dataStreams = Collections.emptyMap();
        } else {
            if (globalMetadata == null) {
                globalMetadata = repository.getSnapshotGlobalMetadata(snapshotId);
            }
            final Map<String, DataStream> dataStreamsInSnapshot = globalMetadata.dataStreams();
            dataStreams = new HashMap<>(requestedDataStreams.size());
            for (String requestedDataStream : requestedDataStreams) {
                final DataStream dataStreamInSnapshot = dataStreamsInSnapshot.get(requestedDataStream);
                assert dataStreamInSnapshot != null : "DataStream [" + requestedDataStream + "] not found in snapshot";
                dataStreams.put(requestedDataStream, dataStreamInSnapshot);
            }
        }
        return dataStreams;
    }

    private Map<String, List<String>> getFeatureStatesToRestore(RestoreSnapshotRequest request, SnapshotInfo snapshotInfo,
                                                                Snapshot snapshot) {
        if (snapshotInfo.featureStates() == null) {
            return Collections.emptyMap();
        }
        final Map<String, List<String>> snapshotFeatureStates = snapshotInfo.featureStates().stream()
            .collect(Collectors.toMap(SnapshotFeatureInfo::getPluginName, SnapshotFeatureInfo::getIndices));

        final Map<String, List<String>> featureStatesToRestore;
        final String[] requestedFeatureStates = request.featureStates();

        if (requestedFeatureStates == null || requestedFeatureStates.length == 0) {
            // Handle the default cases - defer to the global state value
            if (request.includeGlobalState()) {
                featureStatesToRestore = new HashMap<>(snapshotFeatureStates);
            } else {
                featureStatesToRestore = Collections.emptyMap();
            }
        } else if (requestedFeatureStates.length == 1 && NO_FEATURE_STATES_VALUE.equalsIgnoreCase(requestedFeatureStates[0])) {
            // If there's exactly one value and it's "none", include no states
            featureStatesToRestore = Collections.emptyMap();
        } else {
            // Otherwise, handle the list of requested feature states
            final Set<String> requestedStates = Set.of(requestedFeatureStates);
            if (requestedStates.contains(NO_FEATURE_STATES_VALUE)) {
                throw new SnapshotRestoreException(snapshot, "the feature_states value [" + NO_FEATURE_STATES_VALUE +
                    "] indicates that no feature states should be restored, but other feature states were requested: " + requestedStates);
            }
            if (snapshotFeatureStates.keySet().containsAll(requestedStates) == false) {
                Set<String> nonExistingRequestedStates = new HashSet<>(requestedStates);
                nonExistingRequestedStates.removeAll(snapshotFeatureStates.keySet());
                throw new SnapshotRestoreException(snapshot, "requested feature states [" + nonExistingRequestedStates +
                    "] are not present in snapshot");
            }
            featureStatesToRestore = new HashMap<>(snapshotFeatureStates);
            featureStatesToRestore.keySet().retainAll(requestedStates);
        }

        final List<String> featuresNotOnThisNode = featureStatesToRestore.keySet().stream()
            .filter(featureName -> systemIndices.getFeatures().containsKey(featureName) == false)
            .collect(Collectors.toList());
        if (featuresNotOnThisNode.isEmpty() == false) {
            throw new SnapshotRestoreException(snapshot, "requested feature states " + featuresNotOnThisNode + " are present in " +
                "snapshot but those features are not installed on the current master node");
        }
        return featureStatesToRestore;
    }

    /**
     * Resolves a set of index names that currently exist in the cluster that are part of a feature state which is about to be restored,
     * and should therefore be removed prior to restoring those feature states from the snapshot.
     *
     * @param currentState The current cluster state
     * @param featureStatesToRestore A set of feature state names that are about to be restored
     * @return A set of index names that should be removed based on the feature states being restored
     */
    private Set<Index> resolveSystemIndicesToDelete(ClusterState currentState, Set<String> featureStatesToRestore) {
        if (featureStatesToRestore == null) {
            return Collections.emptySet();
        }

        return featureStatesToRestore.stream()
            .map(featureName -> systemIndices.getFeatures().get(featureName))
            .filter(Objects::nonNull) // Features that aren't present on this node will be warned about in `getFeatureStatesToRestore`
            .flatMap(feature -> feature.getIndexDescriptors().stream())
            .flatMap(descriptor -> descriptor.getMatchingIndices(currentState.metadata()).stream())
            .map(indexName -> {
                assert currentState.metadata().hasIndex(indexName) : "index [" + indexName + "] not found in metadata but must be present";
                return currentState.metadata().getIndices().get(indexName).getIndex();
            })
            .collect(Collectors.toUnmodifiableSet());
    }

    //visible for testing
    static DataStream updateDataStream(DataStream dataStream, Metadata.Builder metadata, RestoreSnapshotRequest request) {
        String dataStreamName = dataStream.getName();
        if (request.renamePattern() != null && request.renameReplacement() != null) {
            dataStreamName = dataStreamName.replaceAll(request.renamePattern(), request.renameReplacement());
        }
        List<Index> updatedIndices = dataStream.getIndices().stream()
            .map(i -> metadata.get(renameIndex(i.getName(), request, true)).getIndex())
            .collect(Collectors.toList());
        return new DataStream(dataStreamName, dataStream.getTimeStampField(), updatedIndices, dataStream.getGeneration(),
            dataStream.getMetadata(), dataStream.isHidden(), dataStream.isReplicated());
    }

    public static RestoreInProgress updateRestoreStateWithDeletedIndices(RestoreInProgress oldRestore, Set<Index> deletedIndices) {
        boolean changesMade = false;
        RestoreInProgress.Builder builder = new RestoreInProgress.Builder();
        for (RestoreInProgress.Entry entry : oldRestore) {
            ImmutableOpenMap.Builder<ShardId, ShardRestoreStatus> shardsBuilder = null;
            for (ObjectObjectCursor<ShardId, ShardRestoreStatus> cursor : entry.shards()) {
                ShardId shardId = cursor.key;
                if (deletedIndices.contains(shardId.getIndex())) {
                    changesMade = true;
                    if (shardsBuilder == null) {
                        shardsBuilder = ImmutableOpenMap.builder(entry.shards());
                    }
                    shardsBuilder.put(shardId,
                        new ShardRestoreStatus(null, RestoreInProgress.State.FAILURE, "index was deleted"));
                }
            }
            if (shardsBuilder != null) {
                ImmutableOpenMap<ShardId, ShardRestoreStatus> shards = shardsBuilder.build();
                builder.add(new RestoreInProgress.Entry(entry.uuid(), entry.snapshot(),
                    overallState(RestoreInProgress.State.STARTED, shards), entry.indices(), shards));
            } else {
                builder.add(entry);
            }
        }
        if (changesMade) {
            return builder.build();
        } else {
            return oldRestore;
        }
    }

    public static final class RestoreCompletionResponse {
        private final String uuid;
        private final Snapshot snapshot;
        private final RestoreInfo restoreInfo;

        private RestoreCompletionResponse(final String uuid, final Snapshot snapshot, final RestoreInfo restoreInfo) {
            this.uuid = uuid;
            this.snapshot = snapshot;
            this.restoreInfo = restoreInfo;
        }

        public String getUuid() {
            return uuid;
        }

        public Snapshot getSnapshot() {
            return snapshot;
        }

        public RestoreInfo getRestoreInfo() {
            return restoreInfo;
        }
    }

    public static class RestoreInProgressUpdater extends RoutingChangesObserver.AbstractRoutingChangesObserver {
        // Map of RestoreUUID to a of changes to the shards' restore statuses
        private final Map<String, Map<ShardId, ShardRestoreStatus>> shardChanges = new HashMap<>();

        @Override
        public void shardStarted(ShardRouting initializingShard, ShardRouting startedShard) {
            // mark snapshot as completed
            if (initializingShard.primary()) {
                RecoverySource recoverySource = initializingShard.recoverySource();
                if (recoverySource.getType() == RecoverySource.Type.SNAPSHOT) {
                    changes(recoverySource).put(
                        initializingShard.shardId(),
                        new ShardRestoreStatus(initializingShard.currentNodeId(), RestoreInProgress.State.SUCCESS));
                }
            }
        }

        @Override
        public void shardFailed(ShardRouting failedShard, UnassignedInfo unassignedInfo) {
            if (failedShard.primary() && failedShard.initializing()) {
                RecoverySource recoverySource = failedShard.recoverySource();
                if (recoverySource.getType() == RecoverySource.Type.SNAPSHOT) {
                    // mark restore entry for this shard as failed when it's due to a file corruption. There is no need wait on retries
                    // to restore this shard on another node if the snapshot files are corrupt. In case where a node just left or crashed,
                    // however, we only want to acknowledge the restore operation once it has been successfully restored on another node.
                    if (unassignedInfo.getFailure() != null && Lucene.isCorruptionException(unassignedInfo.getFailure().getCause())) {
                        changes(recoverySource).put(
                            failedShard.shardId(), new ShardRestoreStatus(failedShard.currentNodeId(),
                                RestoreInProgress.State.FAILURE, unassignedInfo.getFailure().getCause().getMessage()));
                    }
                }
            }
        }

        @Override
        public void shardInitialized(ShardRouting unassignedShard, ShardRouting initializedShard) {
            // if we force an empty primary, we should also fail the restore entry
            if (unassignedShard.recoverySource().getType() == RecoverySource.Type.SNAPSHOT &&
                initializedShard.recoverySource().getType() != RecoverySource.Type.SNAPSHOT) {
                changes(unassignedShard.recoverySource()).put(
                    unassignedShard.shardId(),
                    new ShardRestoreStatus(null, RestoreInProgress.State.FAILURE,
                        "recovery source type changed from snapshot to " + initializedShard.recoverySource())
                );
            }
        }

        @Override
        public void unassignedInfoUpdated(ShardRouting unassignedShard, UnassignedInfo newUnassignedInfo) {
            RecoverySource recoverySource = unassignedShard.recoverySource();
            if (recoverySource.getType() == RecoverySource.Type.SNAPSHOT) {
                if (newUnassignedInfo.getLastAllocationStatus() == UnassignedInfo.AllocationStatus.DECIDERS_NO) {
                    String reason = "shard could not be allocated to any of the nodes";
                    changes(recoverySource).put(
                        unassignedShard.shardId(),
                        new ShardRestoreStatus(unassignedShard.currentNodeId(), RestoreInProgress.State.FAILURE, reason));
                }
            }
        }

        /**
         * Helper method that creates update entry for the given recovery source's restore uuid
         * if such an entry does not exist yet.
         */
        private Map<ShardId, ShardRestoreStatus> changes(RecoverySource recoverySource) {
            assert recoverySource.getType() == RecoverySource.Type.SNAPSHOT;
            return shardChanges.computeIfAbsent(((SnapshotRecoverySource) recoverySource).restoreUUID(), k -> new HashMap<>());
        }

        public RestoreInProgress applyChanges(final RestoreInProgress oldRestore) {
            if (shardChanges.isEmpty() == false) {
                RestoreInProgress.Builder builder = new RestoreInProgress.Builder();
                for (RestoreInProgress.Entry entry : oldRestore) {
                    Map<ShardId, ShardRestoreStatus> updates = shardChanges.get(entry.uuid());
                    ImmutableOpenMap<ShardId, ShardRestoreStatus> shardStates = entry.shards();
                    if (updates != null && updates.isEmpty() == false) {
                        ImmutableOpenMap.Builder<ShardId, ShardRestoreStatus> shardsBuilder = ImmutableOpenMap.builder(shardStates);
                        for (Map.Entry<ShardId, ShardRestoreStatus> shard : updates.entrySet()) {
                            ShardId shardId = shard.getKey();
                            ShardRestoreStatus status = shardStates.get(shardId);
                            if (status == null || status.state().completed() == false) {
                                shardsBuilder.put(shardId, shard.getValue());
                            }
                        }

                        ImmutableOpenMap<ShardId, ShardRestoreStatus> shards = shardsBuilder.build();
                        RestoreInProgress.State newState = overallState(RestoreInProgress.State.STARTED, shards);
                        builder.add(new RestoreInProgress.Entry(entry.uuid(), entry.snapshot(), newState, entry.indices(), shards));
                    } else {
                        builder.add(entry);
                    }
                }
                return builder.build();
            } else {
                return oldRestore;
            }
        }
    }

    private static RestoreInProgress.State overallState(RestoreInProgress.State nonCompletedState,
                                                        ImmutableOpenMap<ShardId, RestoreInProgress.ShardRestoreStatus> shards) {
        boolean hasFailed = false;
        for (ObjectCursor<RestoreInProgress.ShardRestoreStatus> status : shards.values()) {
            if (status.value.state().completed() == false) {
                return nonCompletedState;
            }
            if (status.value.state() == RestoreInProgress.State.FAILURE) {
                hasFailed = true;
            }
        }
        if (hasFailed) {
            return RestoreInProgress.State.FAILURE;
        } else {
            return RestoreInProgress.State.SUCCESS;
        }
    }

    public static boolean completed(ImmutableOpenMap<ShardId, RestoreInProgress.ShardRestoreStatus> shards) {
        for (ObjectCursor<RestoreInProgress.ShardRestoreStatus> status : shards.values()) {
            if (status.value.state().completed() == false) {
                return false;
            }
        }
        return true;
    }

    public static int failedShards(ImmutableOpenMap<ShardId, RestoreInProgress.ShardRestoreStatus> shards) {
        int failedShards = 0;
        for (ObjectCursor<RestoreInProgress.ShardRestoreStatus> status : shards.values()) {
            if (status.value.state() == RestoreInProgress.State.FAILURE) {
                failedShards++;
            }
        }
        return failedShards;
    }

    private static Map<String, String> renamedIndices(RestoreSnapshotRequest request, List<String> filteredIndices,
                                                      Set<String> dataStreamIndices, Set<String> featureIndices) {
        Map<String, String> renamedIndices = new HashMap<>();
        for (String index : filteredIndices) {
            String renamedIndex;
            if (featureIndices.contains(index)) {
                // Don't rename system indices
                renamedIndex = index;
            } else {
                renamedIndex = renameIndex(index, request, dataStreamIndices.contains(index));
            }
            String previousIndex = renamedIndices.put(renamedIndex, index);
            if (previousIndex != null) {
                throw new SnapshotRestoreException(request.repository(), request.snapshot(),
                        "indices [" + index + "] and [" + previousIndex + "] are renamed into the same index [" + renamedIndex + "]");
            }
        }
        return Collections.unmodifiableMap(renamedIndices);
    }

    private static String renameIndex(String index, RestoreSnapshotRequest request, boolean partOfDataStream) {
        String renamedIndex = index;
        if (request.renameReplacement() != null && request.renamePattern() != null) {
            partOfDataStream = partOfDataStream && index.startsWith(DataStream.BACKING_INDEX_PREFIX);
            if (partOfDataStream) {
                index = index.substring(DataStream.BACKING_INDEX_PREFIX.length());
            }
            renamedIndex = index.replaceAll(request.renamePattern(), request.renameReplacement());
            if (partOfDataStream) {
                renamedIndex = DataStream.BACKING_INDEX_PREFIX + renamedIndex;
            }
        }
        return renamedIndex;
    }

    /**
     * Checks that snapshots can be restored and have compatible version
     *
     * @param repository      repository name
     * @param snapshotInfo    snapshot metadata
     */
    private static void validateSnapshotRestorable(final String repository, final SnapshotInfo snapshotInfo) {
        if (snapshotInfo.state().restorable() == false) {
            throw new SnapshotRestoreException(new Snapshot(repository, snapshotInfo.snapshotId()),
                                               "unsupported snapshot state [" + snapshotInfo.state() + "]");
        }
        if (Version.CURRENT.before(snapshotInfo.version())) {
            throw new SnapshotRestoreException(new Snapshot(repository, snapshotInfo.snapshotId()),
                                               "the snapshot was created with Elasticsearch version [" + snapshotInfo.version() +
                                                   "] which is higher than the version of this node [" + Version.CURRENT + "]");
        }
        if (snapshotInfo.version().before(Version.CURRENT.minimumIndexCompatibilityVersion())) {
            throw new SnapshotRestoreException(new Snapshot(repository, snapshotInfo.snapshotId()),
                    "the snapshot was created with Elasticsearch version [" + snapshotInfo.version() +
                            "] which is below the current versions minimum index compatibility version [" +
                            Version.CURRENT.minimumIndexCompatibilityVersion() + "]");
        }
    }

    public static boolean failed(SnapshotInfo snapshot, String index) {
        for (SnapshotShardFailure failure : snapshot.shardFailures()) {
            if (index.equals(failure.index())) {
                return true;
            }
        }
        return false;
    }

    /**
     * Returns the indices that are currently being restored and that are contained in the indices-to-check set.
     */
    public static Set<Index> restoringIndices(final ClusterState currentState, final Set<Index> indicesToCheck) {
        final Set<Index> indices = new HashSet<>();
        for (RestoreInProgress.Entry entry : currentState.custom(RestoreInProgress.TYPE, RestoreInProgress.EMPTY)) {
            for (ObjectObjectCursor<ShardId, RestoreInProgress.ShardRestoreStatus> shard : entry.shards()) {
                Index index = shard.key.getIndex();
                if (indicesToCheck.contains(index)
                    && shard.value.state().completed() == false
                    && currentState.getMetadata().index(index) != null) {
                    indices.add(index);
                }
            }
        }
        return indices;
    }

    public static RestoreInProgress.Entry restoreInProgress(ClusterState state, String restoreUUID) {
        return state.custom(RestoreInProgress.TYPE, RestoreInProgress.EMPTY).get(restoreUUID);
    }

    /**
     * Set to true if {@link #removeCompletedRestoresFromClusterState()} already has an in-flight state update running that will clean up
     * all completed restores from the cluster state.
     */
    private volatile boolean cleanupInProgress = false;

    // run a cluster state update that removes all completed restores from the cluster state
    private void removeCompletedRestoresFromClusterState() {
        clusterService.submitStateUpdateTask("clean up snapshot restore status", new ClusterStateUpdateTask(Priority.URGENT) {
            @Override
            public ClusterState execute(ClusterState currentState) {
                RestoreInProgress.Builder restoreInProgressBuilder = new RestoreInProgress.Builder();
                boolean changed = false;
                for (RestoreInProgress.Entry entry : currentState.custom(RestoreInProgress.TYPE, RestoreInProgress.EMPTY)) {
                    if (entry.state().completed()) {
                        changed = true;
                    } else {
                        restoreInProgressBuilder.add(entry);
                    }
                }
                return changed == false ? currentState : ClusterState.builder(currentState).putCustom(
                        RestoreInProgress.TYPE, restoreInProgressBuilder.build()).build();
            }

            @Override
            public void onFailure(final String source, final Exception e) {
                cleanupInProgress = false;
                logger.warn(() -> new ParameterizedMessage("failed to remove completed restores from cluster state"), e);
            }

            @Override
            public void onNoLongerMaster(String source) {
                cleanupInProgress = false;
                logger.debug("no longer master while removing completed restores from cluster state");
            }

            @Override
            public void clusterStateProcessed(String source, ClusterState oldState, ClusterState newState) {
                cleanupInProgress = false;
            }
        });
    }

    @Override
    public void applyClusterState(ClusterChangedEvent event) {
        try {
            if (event.localNodeMaster() && cleanupInProgress == false) {
                for (RestoreInProgress.Entry entry : event.state().custom(RestoreInProgress.TYPE, RestoreInProgress.EMPTY)) {
                    if (entry.state().completed()) {
                        assert completed(entry.shards()) : "state says completed but restore entries are not";
                        removeCompletedRestoresFromClusterState();
                        cleanupInProgress = true;
                        // the above method cleans up all completed restores, no need to keep looping
                        break;
                    }
                }
            }
        } catch (Exception t) {
            assert false : t;
            logger.warn("Failed to update restore state ", t);
        }
    }
}
