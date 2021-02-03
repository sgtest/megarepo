/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.client.ml.inference.trainedmodel;

import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;

import java.io.IOException;
import java.util.Objects;

import static org.elasticsearch.common.xcontent.ConstructingObjectParser.optionalConstructorArg;

public class RegressionConfig implements InferenceConfig {

    public static final ParseField NAME = new ParseField("regression");
    public static final ParseField RESULTS_FIELD = new ParseField("results_field");
    public static final ParseField NUM_TOP_FEATURE_IMPORTANCE_VALUES = new ParseField("num_top_feature_importance_values");

    private static final ConstructingObjectParser<RegressionConfig, Void> PARSER =
            new ConstructingObjectParser<>(NAME.getPreferredName(),
                true,
                args -> new RegressionConfig((String) args[0], (Integer)args[1]));

    static {
        PARSER.declareString(optionalConstructorArg(), RESULTS_FIELD);
        PARSER.declareInt(optionalConstructorArg(), NUM_TOP_FEATURE_IMPORTANCE_VALUES);
    }

    public static RegressionConfig fromXContent(XContentParser parser) {
        return PARSER.apply(parser, null);
    }

    private final String resultsField;
    private final Integer numTopFeatureImportanceValues;

    public RegressionConfig() {
        this(null, null);
    }

    public RegressionConfig(String resultsField, Integer numTopFeatureImportanceValues) {
        this.resultsField = resultsField;
        this.numTopFeatureImportanceValues = numTopFeatureImportanceValues;
    }

    public Integer getNumTopFeatureImportanceValues() {
        return numTopFeatureImportanceValues;
    }

    public String getResultsField() {
        return resultsField;
    }

    @Override
    public String getName() {
        return NAME.getPreferredName();
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, ToXContent.Params params) throws IOException {
        builder.startObject();
        if (resultsField != null) {
            builder.field(RESULTS_FIELD.getPreferredName(), resultsField);
        }
        if (numTopFeatureImportanceValues != null) {
            builder.field(NUM_TOP_FEATURE_IMPORTANCE_VALUES.getPreferredName(), numTopFeatureImportanceValues);
        }
        builder.endObject();
        return builder;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        RegressionConfig that = (RegressionConfig)o;
        return Objects.equals(this.resultsField, that.resultsField)
            && Objects.equals(this.numTopFeatureImportanceValues, that.numTopFeatureImportanceValues);
    }

    @Override
    public int hashCode() {
        return Objects.hash(resultsField, numTopFeatureImportanceValues);
    }

}
