/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.client.ml.inference.trainedmodel.ensemble;


import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;

import java.io.IOException;
import java.util.List;
import java.util.Objects;

public class WeightedSum implements OutputAggregator {

    public static final String NAME = "weighted_sum";
    public static final ParseField WEIGHTS = new ParseField("weights");

    @SuppressWarnings("unchecked")
    private static final ConstructingObjectParser<WeightedSum, Void> PARSER = new ConstructingObjectParser<>(
        NAME,
        true,
        a -> new WeightedSum((List<Double>)a[0]));

    static {
        PARSER.declareDoubleArray(ConstructingObjectParser.optionalConstructorArg(), WEIGHTS);
    }

    public static WeightedSum fromXContent(XContentParser parser) {
        return PARSER.apply(parser, null);
    }

    private final List<Double> weights;

    public WeightedSum(List<Double> weights) {
        this.weights = weights;
    }

    @Override
    public String getName() {
        return NAME;
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, ToXContent.Params params) throws IOException {
        builder.startObject();
        if (weights != null) {
            builder.field(WEIGHTS.getPreferredName(), weights);
        }
        builder.endObject();
        return builder;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        WeightedSum that = (WeightedSum) o;
        return Objects.equals(weights, that.weights);
    }

    @Override
    public int hashCode() {
        return Objects.hash(weights);
    }
}
