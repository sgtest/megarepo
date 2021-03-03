/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.admin.indices.alias;

import com.carrotsearch.hppc.cursors.ObjectCursor;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.RequestValidators;
import org.elasticsearch.action.admin.indices.alias.IndicesAliasesRequest.AliasActions;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.action.support.master.AcknowledgedTransportMasterNodeAction;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.AliasAction;
import org.elasticsearch.cluster.metadata.AliasMetadata;
import org.elasticsearch.cluster.metadata.IndexAbstraction;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.metadata.MetadataIndexAliasesService;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.collect.ImmutableOpenMap;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.index.Index;
import org.elasticsearch.rest.action.admin.indices.AliasesNotFoundException;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.Set;

import static java.util.Collections.unmodifiableList;

/**
 * Add/remove aliases action
 */
public class TransportIndicesAliasesAction extends AcknowledgedTransportMasterNodeAction<IndicesAliasesRequest> {

    private static final Logger logger = LogManager.getLogger(TransportIndicesAliasesAction.class);

    private final MetadataIndexAliasesService indexAliasesService;
    private final RequestValidators<IndicesAliasesRequest> requestValidators;

    @Inject
    public TransportIndicesAliasesAction(
            final TransportService transportService,
            final ClusterService clusterService,
            final ThreadPool threadPool,
            final MetadataIndexAliasesService indexAliasesService,
            final ActionFilters actionFilters,
            final IndexNameExpressionResolver indexNameExpressionResolver,
            final RequestValidators<IndicesAliasesRequest> requestValidators) {
        super(IndicesAliasesAction.NAME, transportService, clusterService, threadPool, actionFilters, IndicesAliasesRequest::new,
            indexNameExpressionResolver, ThreadPool.Names.SAME);
        this.indexAliasesService = indexAliasesService;
        this.requestValidators = Objects.requireNonNull(requestValidators);
    }

    @Override
    protected ClusterBlockException checkBlock(IndicesAliasesRequest request, ClusterState state) {
        Set<String> indices = new HashSet<>();
        for (AliasActions aliasAction : request.aliasActions()) {
            Collections.addAll(indices, aliasAction.indices());
        }
        return state.blocks().indicesBlockedException(ClusterBlockLevel.METADATA_WRITE, indices.toArray(new String[indices.size()]));
    }

    @Override
    protected void masterOperation(Task task, final IndicesAliasesRequest request, final ClusterState state,
                                   final ActionListener<AcknowledgedResponse> listener) {

        //Expand the indices names
        List<AliasActions> actions = request.aliasActions();
        List<AliasAction> finalActions = new ArrayList<>();
        // Resolve all the AliasActions into AliasAction instances and gather all the aliases
        Set<String> aliases = new HashSet<>();
        for (AliasActions action : actions) {
            final Index[] concreteIndices = indexNameExpressionResolver.concreteIndices(state, request.indicesOptions(), false,
                action.indices());
            for (Index concreteIndex : concreteIndices) {
                IndexAbstraction indexAbstraction = state.metadata().getIndicesLookup().get(concreteIndex.getName());
                assert indexAbstraction != null : "invalid cluster metadata. index [" + concreteIndex.getName() + "] was not found";
                if (indexAbstraction.getParentDataStream() != null) {
                    throw new IllegalArgumentException("The provided expressions [" + String.join(",", action.indices())
                        + "] match a backing index belonging to data stream [" + indexAbstraction.getParentDataStream().getName()
                        + "]. Data streams and their backing indices don't support aliases.");
                }
            }
            final Optional<Exception> maybeException = requestValidators.validateRequest(request, state, concreteIndices);
            if (maybeException.isPresent()) {
                listener.onFailure(maybeException.get());
                return;
            }

            Collections.addAll(aliases, action.getOriginalAliases());
            long now = System.currentTimeMillis();
            for (final Index index : concreteIndices) {
                switch (action.actionType()) {
                case ADD:
                    for (String alias : concreteAliases(action, state.metadata(), index.getName())) {
                        String resolvedName = this.indexNameExpressionResolver.resolveDateMathExpression(alias, now);
                        finalActions.add(new AliasAction.Add(index.getName(), resolvedName,
                            action.filter(), action.indexRouting(),
                            action.searchRouting(), action.writeIndex(), action.isHidden()));
                    }
                    break;
                case REMOVE:
                    for (String alias : concreteAliases(action, state.metadata(), index.getName())) {
                        finalActions.add(new AliasAction.Remove(index.getName(), alias, action.mustExist()));
                    }
                    break;
                case REMOVE_INDEX:
                    finalActions.add(new AliasAction.RemoveIndex(index.getName()));
                    break;
                default:
                    throw new IllegalArgumentException("Unsupported action [" + action.actionType() + "]");
                }
            }
        }
        if (finalActions.isEmpty() && false == actions.isEmpty()) {
            throw new AliasesNotFoundException(aliases.toArray(new String[aliases.size()]));
        }
        request.aliasActions().clear();
        IndicesAliasesClusterStateUpdateRequest updateRequest = new IndicesAliasesClusterStateUpdateRequest(unmodifiableList(finalActions))
                .ackTimeout(request.timeout()).masterNodeTimeout(request.masterNodeTimeout());

        indexAliasesService.indicesAliases(updateRequest, listener.delegateResponse((l, e) -> {
            logger.debug("failed to perform aliases", e);
            l.onFailure(e);
        }));
    }

    private static String[] concreteAliases(AliasActions action, Metadata metadata, String concreteIndex) {
        if (action.expandAliasesWildcards()) {
            //for DELETE we expand the aliases
            String[] indexAsArray = {concreteIndex};
            ImmutableOpenMap<String, List<AliasMetadata>> aliasMetadata = metadata.findAliases(action, indexAsArray);
            List<String> finalAliases = new ArrayList<>();
            for (ObjectCursor<List<AliasMetadata>> curAliases : aliasMetadata.values()) {
                for (AliasMetadata aliasMeta: curAliases.value) {
                    finalAliases.add(aliasMeta.alias());
                }
            }
            if (finalAliases.isEmpty() && action.mustExist() != null && action.mustExist()) {
                return action.aliases();
            }
            return finalAliases.toArray(new String[finalAliases.size()]);
        } else {
            //for ADD and REMOVE_INDEX we just return the current aliases
            return action.aliases();
        }
    }
}
