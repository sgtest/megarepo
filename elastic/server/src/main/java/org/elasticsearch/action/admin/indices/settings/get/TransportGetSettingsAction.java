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

package org.elasticsearch.action.admin.indices.settings.get;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.TransportMasterNodeReadAction;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.collect.ImmutableOpenMap;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.regex.Regex;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.settings.SettingsFilter;
import org.elasticsearch.common.util.CollectionUtils;
import org.elasticsearch.index.Index;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;

import java.util.Map;

public class TransportGetSettingsAction extends TransportMasterNodeReadAction<GetSettingsRequest, GetSettingsResponse> {

    private final SettingsFilter settingsFilter;

    @Inject
    public TransportGetSettingsAction(Settings settings, TransportService transportService, ClusterService clusterService,
                                      ThreadPool threadPool, SettingsFilter settingsFilter, ActionFilters actionFilters,
                                      IndexNameExpressionResolver indexNameExpressionResolver) {
        super(settings, GetSettingsAction.NAME, transportService, clusterService, threadPool, actionFilters, GetSettingsRequest::new, indexNameExpressionResolver);
        this.settingsFilter = settingsFilter;
    }

    @Override
    protected String executor() {
        // Very lightweight operation
        return ThreadPool.Names.SAME;
    }

    @Override
    protected ClusterBlockException checkBlock(GetSettingsRequest request, ClusterState state) {
        return state.blocks().indicesBlockedException(ClusterBlockLevel.METADATA_READ, indexNameExpressionResolver.concreteIndexNames(state, request));
    }


    @Override
    protected GetSettingsResponse newResponse() {
        return new GetSettingsResponse();
    }

    @Override
    protected void masterOperation(GetSettingsRequest request, ClusterState state, ActionListener<GetSettingsResponse> listener) {
        Index[] concreteIndices = indexNameExpressionResolver.concreteIndices(state, request);
        ImmutableOpenMap.Builder<String, Settings> indexToSettingsBuilder = ImmutableOpenMap.builder();
        for (Index concreteIndex : concreteIndices) {
            IndexMetaData indexMetaData = state.getMetaData().index(concreteIndex);
            if (indexMetaData == null) {
                continue;
            }

            Settings settings = settingsFilter.filter(indexMetaData.getSettings());
            if (request.humanReadable()) {
                settings = IndexMetaData.addHumanReadableSettings(settings);
            }
            if (CollectionUtils.isEmpty(request.names()) == false) {
                settings = settings.filter(k -> Regex.simpleMatch(request.names(), k));
            }
            indexToSettingsBuilder.put(concreteIndex.getName(), settings);
        }
        listener.onResponse(new GetSettingsResponse(indexToSettingsBuilder.build()));
    }
}
