/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.datastreams.action;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.action.support.master.AcknowledgedTransportMasterNodeAction;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateUpdateTask;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.DataStream;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.metadata.MetadataDeleteIndexService;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Priority;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.index.Index;
import org.elasticsearch.snapshots.SnapshotInProgressException;
import org.elasticsearch.snapshots.SnapshotsService;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.action.DeleteDataStreamAction;

import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

import static org.elasticsearch.xpack.datastreams.action.DataStreamsActionUtil.getDataStreamNames;

public class DeleteDataStreamTransportAction extends AcknowledgedTransportMasterNodeAction<DeleteDataStreamAction.Request> {

    private static final Logger LOGGER = LogManager.getLogger(DeleteDataStreamTransportAction.class);

    private final MetadataDeleteIndexService deleteIndexService;

    @Inject
    public DeleteDataStreamTransportAction(
        TransportService transportService,
        ClusterService clusterService,
        ThreadPool threadPool,
        ActionFilters actionFilters,
        IndexNameExpressionResolver indexNameExpressionResolver,
        MetadataDeleteIndexService deleteIndexService
    ) {
        super(
            DeleteDataStreamAction.NAME,
            transportService,
            clusterService,
            threadPool,
            actionFilters,
            DeleteDataStreamAction.Request::new,
            indexNameExpressionResolver,
            ThreadPool.Names.SAME
        );
        this.deleteIndexService = deleteIndexService;
    }

    @Override
    protected void masterOperation(
        Task task,
        DeleteDataStreamAction.Request request,
        ClusterState state,
        ActionListener<AcknowledgedResponse> listener
    ) throws Exception {
        clusterService.submitStateUpdateTask(
            "remove-data-stream [" + Strings.arrayToCommaDelimitedString(request.getNames()) + "]",
            new ClusterStateUpdateTask(Priority.HIGH, request.masterNodeTimeout()) {

                @Override
                public void onFailure(String source, Exception e) {
                    listener.onFailure(e);
                }

                @Override
                public ClusterState execute(ClusterState currentState) {
                    return removeDataStream(deleteIndexService, indexNameExpressionResolver, currentState, request);
                }

                @Override
                public void clusterStateProcessed(String source, ClusterState oldState, ClusterState newState) {
                    listener.onResponse(AcknowledgedResponse.TRUE);
                }
            }
        );
    }

    static ClusterState removeDataStream(
        MetadataDeleteIndexService deleteIndexService,
        IndexNameExpressionResolver indexNameExpressionResolver,
        ClusterState currentState,
        DeleteDataStreamAction.Request request
    ) {
        List<String> names = getDataStreamNames(indexNameExpressionResolver, currentState, request.getNames(), request.indicesOptions());
        Set<String> dataStreams = new HashSet<>(names);
        Set<String> snapshottingDataStreams = SnapshotsService.snapshottingDataStreams(currentState, dataStreams);

        if (dataStreams.isEmpty()) {
            if (request.isWildcardExpressionsOriginallySpecified()) {
                return currentState;
            } else {
                throw new ResourceNotFoundException("data streams " + Arrays.toString(request.getNames()) + " not found");
            }
        }

        if (snapshottingDataStreams.isEmpty() == false) {
            throw new SnapshotInProgressException(
                "Cannot delete data streams that are being snapshotted: "
                    + snapshottingDataStreams
                    + ". Try again after snapshot finishes or cancel the currently running snapshot."
            );
        }

        Set<Index> backingIndicesToRemove = new HashSet<>();
        for (String dataStreamName : dataStreams) {
            DataStream dataStream = currentState.metadata().dataStreams().get(dataStreamName);
            assert dataStream != null;
            backingIndicesToRemove.addAll(dataStream.getIndices());
        }

        // first delete the data streams and then the indices:
        // (this to avoid data stream validation from failing when deleting an index that is part of a data stream
        // without updating the data stream)
        // TODO: change order when delete index api also updates the data stream the index to be removed is member of
        Metadata.Builder metadata = Metadata.builder(currentState.metadata());
        for (String ds : dataStreams) {
            LOGGER.info("removing data stream [{}]", ds);
            metadata.removeDataStream(ds);
        }
        currentState = ClusterState.builder(currentState).metadata(metadata).build();
        return deleteIndexService.deleteIndices(currentState, backingIndicesToRemove);
    }

    @Override
    protected ClusterBlockException checkBlock(DeleteDataStreamAction.Request request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }
}
