/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml;

import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.xpack.core.XPackFeatureSet;
import org.elasticsearch.xpack.core.XPackField;

import java.io.IOException;
import java.util.Map;
import java.util.Objects;

public class MachineLearningFeatureSetUsage extends XPackFeatureSet.Usage {

    public static final String ALL = "_all";
    public static final String JOBS_FIELD = "jobs";
    public static final String DATAFEEDS_FIELD = "datafeeds";
    public static final String COUNT = "count";
    public static final String DETECTORS = "detectors";
    public static final String MODEL_SIZE = "model_size";

    private final Map<String, Object> jobsUsage;
    private final Map<String, Object> datafeedsUsage;

    public MachineLearningFeatureSetUsage(boolean available, boolean enabled, Map<String, Object> jobsUsage,
                                          Map<String, Object> datafeedsUsage) {
        super(XPackField.MACHINE_LEARNING, available, enabled);
        this.jobsUsage = Objects.requireNonNull(jobsUsage);
        this.datafeedsUsage = Objects.requireNonNull(datafeedsUsage);
    }

    public MachineLearningFeatureSetUsage(StreamInput in) throws IOException {
        super(in);
        this.jobsUsage = in.readMap();
        this.datafeedsUsage = in.readMap();
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        super.writeTo(out);
        out.writeMap(jobsUsage);
        out.writeMap(datafeedsUsage);
    }

    @Override
    protected void innerXContent(XContentBuilder builder, Params params) throws IOException {
        super.innerXContent(builder, params);
        if (jobsUsage != null) {
            builder.field(JOBS_FIELD, jobsUsage);
        }
        if (datafeedsUsage != null) {
            builder.field(DATAFEEDS_FIELD, datafeedsUsage);
        }
    }

}
