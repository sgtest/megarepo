/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.core.ml.inference.trainedmodel;

import org.elasticsearch.TransportVersion;
import org.elasticsearch.common.io.stream.VersionedNamedWriteable;
import org.elasticsearch.xcontent.ParseField;
import org.elasticsearch.xpack.core.ml.MlConfigVersion;
import org.elasticsearch.xpack.core.ml.utils.NamedXContentObject;

public interface InferenceConfig extends NamedXContentObject, VersionedNamedWriteable {

    String DEFAULT_TOP_CLASSES_RESULTS_FIELD = "top_classes";
    String DEFAULT_RESULTS_FIELD = "predicted_value";
    ParseField RESULTS_FIELD = new ParseField("results_field");

    boolean isTargetTypeSupported(TargetType targetType);

    @Override
    default TransportVersion getMinimalSupportedVersion() {
        return getMinimalSupportedTransportVersion();
    }

    /**
     * All nodes in the cluster must have at least this MlConfigVersion attribute
     */
    MlConfigVersion getMinimalSupportedMlConfigVersion();

    /**
     * All communication in the cluster must use at least this version
     */
    TransportVersion getMinimalSupportedTransportVersion();

    default boolean requestingImportance() {
        return false;
    }

    String getResultsField();

    boolean isAllocateOnly();

    default boolean supportsIngestPipeline() {
        return true;
    }

    default boolean supportsPipelineAggregation() {
        return true;
    }

    default boolean supportsSearchRescorer() {
        return false;
    }
}
