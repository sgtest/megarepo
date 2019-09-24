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
package org.elasticsearch.gateway;

import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.metadata.Manifest;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.cluster.routing.RoutingNode;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.index.Index;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Tracks the metadata written to disk, allowing updated metadata to be written incrementally (i.e. only writing out the changed metadata).
 */
class IncrementalClusterStateWriter {

    private final MetaStateService metaStateService;

    // On master-eligible nodes we call updateClusterState under the Coordinator's mutex; on master-ineligible data nodes we call
    // updateClusterState on the (unique) cluster applier thread; on other nodes we never call updateClusterState. In all cases there's
    // no need to synchronize access to these fields.
    private Manifest previousManifest;
    private ClusterState previousClusterState;
    private boolean incrementalWrite;

    IncrementalClusterStateWriter(MetaStateService metaStateService, Manifest manifest, ClusterState clusterState) {
        this.metaStateService = metaStateService;
        this.previousManifest = manifest;
        this.previousClusterState = clusterState;
        this.incrementalWrite = false;
    }

    void setCurrentTerm(long currentTerm) throws WriteStateException {
        Manifest manifest = new Manifest(currentTerm, previousManifest.getClusterStateVersion(), previousManifest.getGlobalGeneration(),
            new HashMap<>(previousManifest.getIndexGenerations()));
        metaStateService.writeManifestAndCleanup("current term changed", manifest);
        previousManifest = manifest;
    }

    Manifest getPreviousManifest() {
        return previousManifest;
    }

    ClusterState getPreviousClusterState() {
        return previousClusterState;
    }

    void setIncrementalWrite(boolean incrementalWrite) {
        this.incrementalWrite = incrementalWrite;
    }

    /**
     * Updates manifest and meta data on disk.
     *
     * @param newState new {@link ClusterState}
     * @param previousState previous {@link ClusterState}
     *
     * @throws WriteStateException if exception occurs. See also {@link WriteStateException#isDirty()}.
     */
    void updateClusterState(ClusterState newState, ClusterState previousState) throws WriteStateException {
        MetaData newMetaData = newState.metaData();

        final AtomicClusterStateWriter writer = new AtomicClusterStateWriter(metaStateService, previousManifest);
        long globalStateGeneration = writeGlobalState(writer, newMetaData);
        Map<Index, Long> indexGenerations = writeIndicesMetadata(writer, newState, previousState);
        Manifest manifest = new Manifest(previousManifest.getCurrentTerm(), newState.version(), globalStateGeneration, indexGenerations);
        writeManifest(writer, manifest);

        previousManifest = manifest;
        previousClusterState = newState;
    }

    private void writeManifest(AtomicClusterStateWriter writer, Manifest manifest) throws WriteStateException {
        if (manifest.equals(previousManifest) == false) {
            writer.writeManifestAndCleanup("changed", manifest);
        }
    }

    private Map<Index, Long> writeIndicesMetadata(AtomicClusterStateWriter writer, ClusterState newState, ClusterState previousState)
        throws WriteStateException {
        Map<Index, Long> previouslyWrittenIndices = previousManifest.getIndexGenerations();
        Set<Index> relevantIndices = getRelevantIndices(newState, previousState, previouslyWrittenIndices.keySet());

        Map<Index, Long> newIndices = new HashMap<>();

        MetaData previousMetaData = incrementalWrite ? previousState.metaData() : null;
        Iterable<IndexMetaDataAction> actions = resolveIndexMetaDataActions(previouslyWrittenIndices, relevantIndices, previousMetaData,
            newState.metaData());

        for (IndexMetaDataAction action : actions) {
            long generation = action.execute(writer);
            newIndices.put(action.getIndex(), generation);
        }

        return newIndices;
    }

    private long writeGlobalState(AtomicClusterStateWriter writer, MetaData newMetaData) throws WriteStateException {
        if (incrementalWrite == false || MetaData.isGlobalStateEquals(previousClusterState.metaData(), newMetaData) == false) {
            return writer.writeGlobalState("changed", newMetaData);
        }
        return previousManifest.getGlobalGeneration();
    }


    /**
     * Returns list of {@link IndexMetaDataAction} for each relevant index.
     * For each relevant index there are 3 options:
     * <ol>
     * <li>
     * {@link KeepPreviousGeneration} - index metadata is already stored to disk and index metadata version is not changed, no
     * action is required.
     * </li>
     * <li>
     * {@link WriteNewIndexMetaData} - there is no index metadata on disk and index metadata for this index should be written.
     * </li>
     * <li>
     * {@link WriteChangedIndexMetaData} - index metadata is already on disk, but index metadata version has changed. Updated
     * index metadata should be written to disk.
     * </li>
     * </ol>
     *
     * @param previouslyWrittenIndices A list of indices for which the state was already written before
     * @param relevantIndices          The list of indices for which state should potentially be written
     * @param previousMetaData         The last meta data we know of
     * @param newMetaData              The new metadata
     * @return list of {@link IndexMetaDataAction} for each relevant index.
     */
    // exposed for tests
    static List<IndexMetaDataAction> resolveIndexMetaDataActions(Map<Index, Long> previouslyWrittenIndices,
                                                                 Set<Index> relevantIndices,
                                                                 MetaData previousMetaData,
                                                                 MetaData newMetaData) {
        List<IndexMetaDataAction> actions = new ArrayList<>();
        for (Index index : relevantIndices) {
            IndexMetaData newIndexMetaData = newMetaData.getIndexSafe(index);
            IndexMetaData previousIndexMetaData = previousMetaData == null ? null : previousMetaData.index(index);

            if (previouslyWrittenIndices.containsKey(index) == false || previousIndexMetaData == null) {
                actions.add(new WriteNewIndexMetaData(newIndexMetaData));
            } else if (previousIndexMetaData.getVersion() != newIndexMetaData.getVersion()) {
                actions.add(new WriteChangedIndexMetaData(previousIndexMetaData, newIndexMetaData));
            } else {
                actions.add(new KeepPreviousGeneration(index, previouslyWrittenIndices.get(index)));
            }
        }
        return actions;
    }

    private static Set<Index> getRelevantIndicesOnDataOnlyNode(ClusterState state, ClusterState previousState, Set<Index>
        previouslyWrittenIndices) {
        RoutingNode newRoutingNode = state.getRoutingNodes().node(state.nodes().getLocalNodeId());
        if (newRoutingNode == null) {
            throw new IllegalStateException("cluster state does not contain this node - cannot write index meta state");
        }
        Set<Index> indices = new HashSet<>();
        for (ShardRouting routing : newRoutingNode) {
            indices.add(routing.index());
        }
        // we have to check the meta data also: closed indices will not appear in the routing table, but we must still write the state if
        // we have it written on disk previously
        for (IndexMetaData indexMetaData : state.metaData()) {
            boolean isOrWasClosed = indexMetaData.getState().equals(IndexMetaData.State.CLOSE);
            // if the index is open we might still have to write the state if it just transitioned from closed to open
            // so we have to check for that as well.
            IndexMetaData previousMetaData = previousState.metaData().index(indexMetaData.getIndex());
            if (previousMetaData != null) {
                isOrWasClosed = isOrWasClosed || previousMetaData.getState().equals(IndexMetaData.State.CLOSE);
            }
            if (previouslyWrittenIndices.contains(indexMetaData.getIndex()) && isOrWasClosed) {
                indices.add(indexMetaData.getIndex());
            }
        }
        return indices;
    }

    private static Set<Index> getRelevantIndicesForMasterEligibleNode(ClusterState state) {
        Set<Index> relevantIndices = new HashSet<>();
        // we have to iterate over the metadata to make sure we also capture closed indices
        for (IndexMetaData indexMetaData : state.metaData()) {
            relevantIndices.add(indexMetaData.getIndex());
        }
        return relevantIndices;
    }

    // exposed for tests
    static Set<Index> getRelevantIndices(ClusterState state, ClusterState previousState, Set<Index> previouslyWrittenIndices) {
        Set<Index> relevantIndices;
        if (isDataOnlyNode(state)) {
            relevantIndices = getRelevantIndicesOnDataOnlyNode(state, previousState, previouslyWrittenIndices);
        } else if (state.nodes().getLocalNode().isMasterNode()) {
            relevantIndices = getRelevantIndicesForMasterEligibleNode(state);
        } else {
            relevantIndices = Collections.emptySet();
        }
        return relevantIndices;
    }

    private static boolean isDataOnlyNode(ClusterState state) {
        return state.nodes().getLocalNode().isMasterNode() == false && state.nodes().getLocalNode().isDataNode();
    }

    /**
     * Action to perform with index metadata.
     */
    interface IndexMetaDataAction {
        /**
         * @return index for index metadata.
         */
        Index getIndex();

        /**
         * Executes this action using provided {@link AtomicClusterStateWriter}.
         *
         * @return new index metadata state generation, to be used in manifest file.
         * @throws WriteStateException if exception occurs.
         */
        long execute(AtomicClusterStateWriter writer) throws WriteStateException;
    }

    /**
     * This class is used to write changed global {@link MetaData}, {@link IndexMetaData} and {@link Manifest} to disk.
     * This class delegates <code>write*</code> calls to corresponding write calls in {@link MetaStateService} and
     * additionally it keeps track of cleanup actions to be performed if transaction succeeds or fails.
     */
    static class AtomicClusterStateWriter {
        private static final String FINISHED_MSG = "AtomicClusterStateWriter is finished";
        private final List<Runnable> commitCleanupActions;
        private final List<Runnable> rollbackCleanupActions;
        private final Manifest previousManifest;
        private final MetaStateService metaStateService;
        private boolean finished;

        AtomicClusterStateWriter(MetaStateService metaStateService, Manifest previousManifest) {
            this.metaStateService = metaStateService;
            assert previousManifest != null;
            this.previousManifest = previousManifest;
            this.commitCleanupActions = new ArrayList<>();
            this.rollbackCleanupActions = new ArrayList<>();
            this.finished = false;
        }

        long writeGlobalState(String reason, MetaData metaData) throws WriteStateException {
            assert finished == false : FINISHED_MSG;
            try {
                rollbackCleanupActions.add(() -> metaStateService.cleanupGlobalState(previousManifest.getGlobalGeneration()));
                long generation = metaStateService.writeGlobalState(reason, metaData);
                commitCleanupActions.add(() -> metaStateService.cleanupGlobalState(generation));
                return generation;
            } catch (WriteStateException e) {
                rollback();
                throw e;
            }
        }

        long writeIndex(String reason, IndexMetaData metaData) throws WriteStateException {
            assert finished == false : FINISHED_MSG;
            try {
                Index index = metaData.getIndex();
                Long previousGeneration = previousManifest.getIndexGenerations().get(index);
                if (previousGeneration != null) {
                    // we prefer not to clean-up index metadata in case of rollback,
                    // if it's not referenced by previous manifest file
                    // not to break dangling indices functionality
                    rollbackCleanupActions.add(() -> metaStateService.cleanupIndex(index, previousGeneration));
                }
                long generation = metaStateService.writeIndex(reason, metaData);
                commitCleanupActions.add(() -> metaStateService.cleanupIndex(index, generation));
                return generation;
            } catch (WriteStateException e) {
                rollback();
                throw e;
            }
        }

        void writeManifestAndCleanup(String reason, Manifest manifest) throws WriteStateException {
            assert finished == false : FINISHED_MSG;
            try {
                metaStateService.writeManifestAndCleanup(reason, manifest);
                commitCleanupActions.forEach(Runnable::run);
                finished = true;
            } catch (WriteStateException e) {
                // If the Manifest write results in a dirty WriteStateException it's not safe to roll back, removing the new metadata files,
                // because if the Manifest was actually written to disk and its deletion fails it will reference these new metadata files.
                // On master-eligible nodes a dirty WriteStateException here is fatal to the node since we no longer really have any idea
                // what the state on disk is and the only sensible response is to start again from scratch.
                if (e.isDirty() == false) {
                    rollback();
                }
                throw e;
            }
        }

        void rollback() {
            rollbackCleanupActions.forEach(Runnable::run);
            finished = true;
        }
    }

    static class KeepPreviousGeneration implements IndexMetaDataAction {
        private final Index index;
        private final long generation;

        KeepPreviousGeneration(Index index, long generation) {
            this.index = index;
            this.generation = generation;
        }

        @Override
        public Index getIndex() {
            return index;
        }

        @Override
        public long execute(AtomicClusterStateWriter writer) {
            return generation;
        }
    }

    static class WriteNewIndexMetaData implements IndexMetaDataAction {
        private final IndexMetaData indexMetaData;

        WriteNewIndexMetaData(IndexMetaData indexMetaData) {
            this.indexMetaData = indexMetaData;
        }

        @Override
        public Index getIndex() {
            return indexMetaData.getIndex();
        }

        @Override
        public long execute(AtomicClusterStateWriter writer) throws WriteStateException {
            return writer.writeIndex("freshly created", indexMetaData);
        }
    }

    static class WriteChangedIndexMetaData implements IndexMetaDataAction {
        private final IndexMetaData newIndexMetaData;
        private final IndexMetaData oldIndexMetaData;

        WriteChangedIndexMetaData(IndexMetaData oldIndexMetaData, IndexMetaData newIndexMetaData) {
            this.oldIndexMetaData = oldIndexMetaData;
            this.newIndexMetaData = newIndexMetaData;
        }

        @Override
        public Index getIndex() {
            return newIndexMetaData.getIndex();
        }

        @Override
        public long execute(AtomicClusterStateWriter writer) throws WriteStateException {
            return writer.writeIndex(
                    "version changed from [" + oldIndexMetaData.getVersion() + "] to [" + newIndexMetaData.getVersion() + "]",
                    newIndexMetaData);
        }
    }
}
