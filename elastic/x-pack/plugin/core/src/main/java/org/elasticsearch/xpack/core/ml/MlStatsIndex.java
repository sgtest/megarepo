/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml;

import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.xpack.core.ml.utils.MlIndexAndAlias;
import org.elasticsearch.xpack.core.template.TemplateUtils;

/**
 * Describes the indices where ML is storing various stats about the users jobs.
 */
public class MlStatsIndex {

    public static final String TEMPLATE_NAME = ".ml-stats";

    private static final String MAPPINGS_VERSION_VARIABLE = "xpack.ml.version";

    private MlStatsIndex() {}

    public static String mapping() {
        return TemplateUtils.loadTemplate("/org/elasticsearch/xpack/core/ml/stats_index_mappings.json",
            Version.CURRENT.toString(), MAPPINGS_VERSION_VARIABLE);
    }

    public static String indexPattern() {
        return TEMPLATE_NAME + "-*";
    }

    public static String writeAlias() {
        return ".ml-stats-write";
    }

    /**
     * Creates the first concrete .ml-stats-000001 index (if necessary)
     * Creates the .ml-stats-write alias for that index.
     * The listener will be notified with a boolean to indicate if the index was created because of this call,
     * but unless there is a failure after this method returns the index and alias should be present.
     */
    public static void createStatsIndexAndAliasIfNecessary(Client client, ClusterState state, IndexNameExpressionResolver resolver,
                                                           ActionListener<Boolean> listener) {
        MlIndexAndAlias.createIndexAndAliasIfNecessary(client, state, resolver, TEMPLATE_NAME, writeAlias(), listener);
    }
}
