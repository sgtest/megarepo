/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.indices.recovery.plan;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.index.store.Store;
import org.elasticsearch.index.store.StoreFileMetadata;
import org.elasticsearch.indices.recovery.RecoverySettings;

import java.util.List;
import java.util.Optional;
import java.util.function.Consumer;
import java.util.function.Function;
import java.util.stream.Collectors;

import static java.util.Collections.emptyMap;
import static org.elasticsearch.common.util.CollectionUtils.concatLists;

public class SnapshotsRecoveryPlannerService implements RecoveryPlannerService {
    private final Logger logger = LogManager.getLogger(SnapshotsRecoveryPlannerService.class);

    private final ShardSnapshotsService shardSnapshotsService;

    public SnapshotsRecoveryPlannerService(ShardSnapshotsService shardSnapshotsService) {
        this.shardSnapshotsService = shardSnapshotsService;
    }

    public void computeRecoveryPlan(ShardId shardId,
                                    Store.MetadataSnapshot sourceMetadata,
                                    Store.MetadataSnapshot targetMetadata,
                                    long startingSeqNo,
                                    int translogOps,
                                    Version targetVersion,
                                    boolean useSnapshots,
                                    ActionListener<ShardRecoveryPlan> listener) {
        // Fallback to source only recovery if the target node is in an incompatible version
        boolean canUseSnapshots = useSnapshots && targetVersion.onOrAfter(RecoverySettings.SNAPSHOT_RECOVERIES_SUPPORTED_VERSION);

        fetchLatestSnapshotsIgnoringErrors(shardId, canUseSnapshots, latestSnapshotOpt ->
            ActionListener.completeWith(listener, () ->
                computeRecoveryPlanWithSnapshots(
                    sourceMetadata,
                    targetMetadata,
                    startingSeqNo,
                    translogOps,
                    latestSnapshotOpt
                )
            )
        );
    }

    private ShardRecoveryPlan computeRecoveryPlanWithSnapshots(Store.MetadataSnapshot sourceMetadata,
                                                               Store.MetadataSnapshot targetMetadata,
                                                               long startingSeqNo,
                                                               int translogOps,
                                                               Optional<ShardSnapshot> latestSnapshotOpt) {
        Store.RecoveryDiff sourceTargetDiff = sourceMetadata.recoveryDiff(targetMetadata);
        List<StoreFileMetadata> filesMissingInTarget = concatLists(sourceTargetDiff.missing, sourceTargetDiff.different);

        if (latestSnapshotOpt.isEmpty()) {
            // If we couldn't find any valid  snapshots, fallback to the source
            return new ShardRecoveryPlan(ShardRecoveryPlan.SnapshotFilesToRecover.EMPTY,
                filesMissingInTarget,
                sourceTargetDiff.identical,
                startingSeqNo,
                translogOps,
                sourceMetadata
            );
        }

        ShardSnapshot latestSnapshot = latestSnapshotOpt.get();

        Store.MetadataSnapshot filesToRecoverFromSourceSnapshot = toMetadataSnapshot(filesMissingInTarget);
        Store.RecoveryDiff snapshotDiff = filesToRecoverFromSourceSnapshot.recoveryDiff(latestSnapshot.getMetadataSnapshot());
        final ShardRecoveryPlan.SnapshotFilesToRecover snapshotFilesToRecover;
        if (snapshotDiff.identical.isEmpty()) {
            snapshotFilesToRecover = ShardRecoveryPlan.SnapshotFilesToRecover.EMPTY;
        } else {
            snapshotFilesToRecover = new ShardRecoveryPlan.SnapshotFilesToRecover(latestSnapshot.getIndexId(),
                latestSnapshot.getRepository(),
                latestSnapshot.getSnapshotFiles(snapshotDiff.identical));
        }

        return new ShardRecoveryPlan(snapshotFilesToRecover,
            concatLists(snapshotDiff.missing, snapshotDiff.different),
            sourceTargetDiff.identical,
            startingSeqNo,
            translogOps,
            sourceMetadata
        );
    }

    private void fetchLatestSnapshotsIgnoringErrors(ShardId shardId, boolean useSnapshots, Consumer<Optional<ShardSnapshot>> listener) {
        if (useSnapshots == false) {
            listener.accept(Optional.empty());
            return;
        }

        ActionListener<Optional<ShardSnapshot>> listenerIgnoringErrors = new ActionListener<>() {
            @Override
            public void onResponse(Optional<ShardSnapshot> shardSnapshotData) {
                listener.accept(shardSnapshotData);
            }

            @Override
            public void onFailure(Exception e) {
                logger.warn(new ParameterizedMessage("Unable to fetch available snapshots for shard {}", shardId), e);
                listener.accept(Optional.empty());
            }
        };

        shardSnapshotsService.fetchLatestSnapshotsForShard(shardId, listenerIgnoringErrors);
    }

    private Store.MetadataSnapshot toMetadataSnapshot(List<StoreFileMetadata> files) {
        return new Store.MetadataSnapshot(
            files
                .stream()
                .collect(Collectors.toMap(StoreFileMetadata::name, Function.identity())),
            emptyMap(),
            0
        );
    }
}
