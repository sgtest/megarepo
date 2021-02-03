/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.core.vectors;

import org.elasticsearch.Version;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.xpack.core.XPackFeatureSet;
import org.elasticsearch.xpack.core.XPackField;

import java.io.IOException;
import java.util.Objects;

public class VectorsFeatureSetUsage extends XPackFeatureSet.Usage {

    private final int numDenseVectorFields;
    private final int avgDenseVectorDims;

    public VectorsFeatureSetUsage(StreamInput input) throws IOException {
        super(input);
        numDenseVectorFields = input.readVInt();
        // Older versions recorded the number of sparse vector fields.
        if (input.getVersion().before(Version.V_8_0_0)) {
            input.readVInt();
        }
        avgDenseVectorDims = input.readVInt();
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        super.writeTo(out);
        out.writeVInt(numDenseVectorFields);
        // Older versions recorded the number of sparse vector fields.
        if (out.getVersion().before(Version.V_8_0_0)) {
            out.writeVInt(0);
        }
        out.writeVInt(avgDenseVectorDims);
    }

    @Override
    public Version getMinimalSupportedVersion() {
        return Version.V_7_3_0;
    }

    public VectorsFeatureSetUsage(boolean available, int numDenseVectorFields, int avgDenseVectorDims) {
        super(XPackField.VECTORS, available, true);
        this.numDenseVectorFields = numDenseVectorFields;
        this.avgDenseVectorDims = avgDenseVectorDims;
    }


    @Override
    protected void innerXContent(XContentBuilder builder, Params params) throws IOException {
        super.innerXContent(builder, params);
        builder.field("dense_vector_fields_count", numDenseVectorFields);
        builder.field("dense_vector_dims_avg_count", avgDenseVectorDims);
    }

    public int numDenseVectorFields() {
        return numDenseVectorFields;
    }

    public int avgDenseVectorDims() {
        return avgDenseVectorDims;
    }

    @Override
    public int hashCode() {
        return Objects.hash(available, enabled, numDenseVectorFields, avgDenseVectorDims);
    }

    @Override
    public boolean equals(Object obj) {
        if (obj instanceof VectorsFeatureSetUsage == false) return false;
        VectorsFeatureSetUsage other = (VectorsFeatureSetUsage) obj;
        return available == other.available && enabled == other.enabled && numDenseVectorFields == other.numDenseVectorFields
            && avgDenseVectorDims == other.avgDenseVectorDims;
    }
}
