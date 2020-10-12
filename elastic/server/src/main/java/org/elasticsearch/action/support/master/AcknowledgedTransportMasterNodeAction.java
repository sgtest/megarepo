/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.action.support.master;

import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;

/**
 * Base class for the common case of a {@link TransportMasterNodeAction} that responds with an {@link AcknowledgedResponse}.
 */
public abstract class AcknowledgedTransportMasterNodeAction<Request extends MasterNodeRequest<Request>>
        extends TransportMasterNodeAction<Request, AcknowledgedResponse> {

    protected AcknowledgedTransportMasterNodeAction(String actionName, TransportService transportService, ClusterService clusterService,
                                                    ThreadPool threadPool, ActionFilters actionFilters, Writeable.Reader<Request> request,
                                                    IndexNameExpressionResolver indexNameExpressionResolver, String executor) {
        super(actionName, transportService, clusterService, threadPool, actionFilters, request, indexNameExpressionResolver,
                AcknowledgedResponse::readFrom, executor);
    }

    protected AcknowledgedTransportMasterNodeAction(String actionName, boolean canTripCircuitBreaker,
                                                    TransportService transportService, ClusterService clusterService,
                                                    ThreadPool threadPool, ActionFilters actionFilters,
                                                    Writeable.Reader<Request> request,
                                                    IndexNameExpressionResolver indexNameExpressionResolver, String executor) {
        super(actionName, canTripCircuitBreaker, transportService, clusterService, threadPool, actionFilters, request,
            indexNameExpressionResolver, AcknowledgedResponse::readFrom, executor);
    }
}
