/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.vectortile;

import org.elasticsearch.Build;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.IndexScopedSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.settings.SettingsFilter;
import org.elasticsearch.core.Booleans;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.plugins.ActionPlugin;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestHandler;
import org.elasticsearch.threadpool.ExecutorBuilder;
import org.elasticsearch.threadpool.FixedExecutorBuilder;
import org.elasticsearch.xpack.core.XPackPlugin;
import org.elasticsearch.xpack.vectortile.rest.RestVectorTileAction;

import java.util.Collections;
import java.util.List;
import java.util.function.Supplier;

public class VectorTilePlugin extends Plugin implements ActionPlugin {

    // to be overriden by tests
    protected XPackLicenseState getLicenseState() {
        return XPackPlugin.getSharedLicenseState();
    }

    private static final Boolean VECTOR_TILE_FEATURE_FLAG_REGISTERED;

    static {
        final String property = System.getProperty("es.vector_tile_feature_flag_registered");
        if (Build.CURRENT.isSnapshot() && property != null) {
            throw new IllegalArgumentException("es.vector_tile_feature_flag_registered is only supported in non-snapshot builds");
        }
        VECTOR_TILE_FEATURE_FLAG_REGISTERED = Booleans.parseBoolean(property, null);
    }

    public boolean isVectorTileEnabled() {
        return Build.CURRENT.isSnapshot() || (VECTOR_TILE_FEATURE_FLAG_REGISTERED != null && VECTOR_TILE_FEATURE_FLAG_REGISTERED);
    }

    @Override
    public List<RestHandler> getRestHandlers(
        Settings settings,
        RestController restController,
        ClusterSettings clusterSettings,
        IndexScopedSettings indexScopedSettings,
        SettingsFilter settingsFilter,
        IndexNameExpressionResolver indexNameExpressionResolver,
        Supplier<DiscoveryNodes> nodesInCluster
    ) {
        if (isVectorTileEnabled()) {
            return List.of(new RestVectorTileAction());
        } else {
            return List.of();
        }
    }

    @Override
    public List<ExecutorBuilder<?>> getExecutorBuilders(Settings settings) {
        FixedExecutorBuilder indexing = new FixedExecutorBuilder(
            settings,
            "vector_tile_generation",
            1,
            -1,
            "thread_pool.vectortile",
            false
        );
        return Collections.singletonList(indexing);
    }
}
