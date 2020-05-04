/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.vectors;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.MappingMetadata;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.protocol.xpack.XPackUsageRequest;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.action.XPackUsageFeatureAction;
import org.elasticsearch.xpack.core.action.XPackUsageFeatureResponse;
import org.elasticsearch.xpack.core.action.XPackUsageFeatureTransportAction;
import org.elasticsearch.xpack.core.vectors.VectorsFeatureSetUsage;
import org.elasticsearch.xpack.vectors.mapper.DenseVectorFieldMapper;
import org.elasticsearch.xpack.vectors.mapper.SparseVectorFieldMapper;

import java.util.Map;

public class VectorsUsageTransportAction extends XPackUsageFeatureTransportAction {

    private final XPackLicenseState licenseState;

    @Inject
    public VectorsUsageTransportAction(TransportService transportService, ClusterService clusterService, ThreadPool threadPool,
                                       ActionFilters actionFilters, IndexNameExpressionResolver indexNameExpressionResolver,
                                       XPackLicenseState licenseState) {
        super(XPackUsageFeatureAction.VECTORS.name(), transportService, clusterService,
            threadPool, actionFilters, indexNameExpressionResolver);
        this.licenseState = licenseState;
    }

    @Override
    protected void masterOperation(Task task, XPackUsageRequest request, ClusterState state,
                                   ActionListener<XPackUsageFeatureResponse> listener) {
        boolean vectorsAvailable = licenseState.isAllowed(XPackLicenseState.Feature.VECTORS);
        int numDenseVectorFields = 0;
        int numSparseVectorFields = 0;
        int avgDenseVectorDims = 0;

        if (vectorsAvailable && state != null) {
            for (IndexMetadata indexMetadata : state.metadata()) {
                MappingMetadata mappingMetadata = indexMetadata.mapping();
                if (mappingMetadata != null) {
                    Map<String, Object> mappings = mappingMetadata.getSourceAsMap();
                    if (mappings.containsKey("properties")) {
                        @SuppressWarnings("unchecked") Map<String, Map<String, Object>> fieldMappings =
                            (Map<String, Map<String, Object>>) mappings.get("properties");
                        for (Map<String, Object> typeDefinition : fieldMappings.values()) {
                            String fieldType = (String) typeDefinition.get("type");
                            if (fieldType != null) {
                                if (fieldType.equals(DenseVectorFieldMapper.CONTENT_TYPE)) {
                                    numDenseVectorFields++;
                                    int dims = (Integer) typeDefinition.get("dims");
                                    avgDenseVectorDims += dims;
                                } else if (fieldType.equals(SparseVectorFieldMapper.CONTENT_TYPE)) {
                                    numSparseVectorFields++;
                                }
                            }
                        }
                    }
                }
            }
            if (numDenseVectorFields > 0) {
                avgDenseVectorDims = avgDenseVectorDims / numDenseVectorFields;
            }
        }
        VectorsFeatureSetUsage usage =
            new VectorsFeatureSetUsage(vectorsAvailable, numDenseVectorFields, avgDenseVectorDims);
        listener.onResponse(new XPackUsageFeatureResponse(usage));
    }
}
