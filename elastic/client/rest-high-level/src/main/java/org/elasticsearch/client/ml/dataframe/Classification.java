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
package org.elasticsearch.client.ml.dataframe;

import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;

import java.io.IOException;
import java.util.Objects;

public class Classification implements DataFrameAnalysis {

    public static Classification fromXContent(XContentParser parser) {
        return PARSER.apply(parser, null);
    }

    public static Builder builder(String dependentVariable) {
        return new Builder(dependentVariable);
    }

    public static final ParseField NAME = new ParseField("classification");

    static final ParseField DEPENDENT_VARIABLE = new ParseField("dependent_variable");
    static final ParseField LAMBDA = new ParseField("lambda");
    static final ParseField GAMMA = new ParseField("gamma");
    static final ParseField ETA = new ParseField("eta");
    static final ParseField MAXIMUM_NUMBER_TREES = new ParseField("maximum_number_trees");
    static final ParseField FEATURE_BAG_FRACTION = new ParseField("feature_bag_fraction");
    static final ParseField PREDICTION_FIELD_NAME = new ParseField("prediction_field_name");
    static final ParseField TRAINING_PERCENT = new ParseField("training_percent");

    private static final ConstructingObjectParser<Classification, Void> PARSER =
        new ConstructingObjectParser<>(
            NAME.getPreferredName(),
            true,
            a -> new Classification(
                (String) a[0],
                (Double) a[1],
                (Double) a[2],
                (Double) a[3],
                (Integer) a[4],
                (Double) a[5],
                (String) a[6],
                (Double) a[7]));

    static {
        PARSER.declareString(ConstructingObjectParser.constructorArg(), DEPENDENT_VARIABLE);
        PARSER.declareDouble(ConstructingObjectParser.optionalConstructorArg(), LAMBDA);
        PARSER.declareDouble(ConstructingObjectParser.optionalConstructorArg(), GAMMA);
        PARSER.declareDouble(ConstructingObjectParser.optionalConstructorArg(), ETA);
        PARSER.declareInt(ConstructingObjectParser.optionalConstructorArg(), MAXIMUM_NUMBER_TREES);
        PARSER.declareDouble(ConstructingObjectParser.optionalConstructorArg(), FEATURE_BAG_FRACTION);
        PARSER.declareString(ConstructingObjectParser.optionalConstructorArg(), PREDICTION_FIELD_NAME);
        PARSER.declareDouble(ConstructingObjectParser.optionalConstructorArg(), TRAINING_PERCENT);
    }

    private final String dependentVariable;
    private final Double lambda;
    private final Double gamma;
    private final Double eta;
    private final Integer maximumNumberTrees;
    private final Double featureBagFraction;
    private final String predictionFieldName;
    private final Double trainingPercent;

    private Classification(String dependentVariable, @Nullable Double lambda, @Nullable Double gamma, @Nullable Double eta,
                           @Nullable Integer maximumNumberTrees, @Nullable Double featureBagFraction, @Nullable String predictionFieldName,
                           @Nullable Double trainingPercent) {
        this.dependentVariable = Objects.requireNonNull(dependentVariable);
        this.lambda = lambda;
        this.gamma = gamma;
        this.eta = eta;
        this.maximumNumberTrees = maximumNumberTrees;
        this.featureBagFraction = featureBagFraction;
        this.predictionFieldName = predictionFieldName;
        this.trainingPercent = trainingPercent;
    }

    @Override
    public String getName() {
        return NAME.getPreferredName();
    }

    public String getDependentVariable() {
        return dependentVariable;
    }

    public Double getLambda() {
        return lambda;
    }

    public Double getGamma() {
        return gamma;
    }

    public Double getEta() {
        return eta;
    }

    public Integer getMaximumNumberTrees() {
        return maximumNumberTrees;
    }

    public Double getFeatureBagFraction() {
        return featureBagFraction;
    }

    public String getPredictionFieldName() {
        return predictionFieldName;
    }

    public Double getTrainingPercent() {
        return trainingPercent;
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.field(DEPENDENT_VARIABLE.getPreferredName(), dependentVariable);
        if (lambda != null) {
            builder.field(LAMBDA.getPreferredName(), lambda);
        }
        if (gamma != null) {
            builder.field(GAMMA.getPreferredName(), gamma);
        }
        if (eta != null) {
            builder.field(ETA.getPreferredName(), eta);
        }
        if (maximumNumberTrees != null) {
            builder.field(MAXIMUM_NUMBER_TREES.getPreferredName(), maximumNumberTrees);
        }
        if (featureBagFraction != null) {
            builder.field(FEATURE_BAG_FRACTION.getPreferredName(), featureBagFraction);
        }
        if (predictionFieldName != null) {
            builder.field(PREDICTION_FIELD_NAME.getPreferredName(), predictionFieldName);
        }
        if (trainingPercent != null) {
            builder.field(TRAINING_PERCENT.getPreferredName(), trainingPercent);
        }
        builder.endObject();
        return builder;
    }

    @Override
    public int hashCode() {
        return Objects.hash(dependentVariable, lambda, gamma, eta, maximumNumberTrees, featureBagFraction, predictionFieldName,
            trainingPercent);
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        Classification that = (Classification) o;
        return Objects.equals(dependentVariable, that.dependentVariable)
            && Objects.equals(lambda, that.lambda)
            && Objects.equals(gamma, that.gamma)
            && Objects.equals(eta, that.eta)
            && Objects.equals(maximumNumberTrees, that.maximumNumberTrees)
            && Objects.equals(featureBagFraction, that.featureBagFraction)
            && Objects.equals(predictionFieldName, that.predictionFieldName)
            && Objects.equals(trainingPercent, that.trainingPercent);
    }

    @Override
    public String toString() {
        return Strings.toString(this);
    }

    public static class Builder {
        private String dependentVariable;
        private Double lambda;
        private Double gamma;
        private Double eta;
        private Integer maximumNumberTrees;
        private Double featureBagFraction;
        private String predictionFieldName;
        private Double trainingPercent;

        private Builder(String dependentVariable) {
            this.dependentVariable = Objects.requireNonNull(dependentVariable);
        }

        public Builder setLambda(Double lambda) {
            this.lambda = lambda;
            return this;
        }

        public Builder setGamma(Double gamma) {
            this.gamma = gamma;
            return this;
        }

        public Builder setEta(Double eta) {
            this.eta = eta;
            return this;
        }

        public Builder setMaximumNumberTrees(Integer maximumNumberTrees) {
            this.maximumNumberTrees = maximumNumberTrees;
            return this;
        }

        public Builder setFeatureBagFraction(Double featureBagFraction) {
            this.featureBagFraction = featureBagFraction;
            return this;
        }

        public Builder setPredictionFieldName(String predictionFieldName) {
            this.predictionFieldName = predictionFieldName;
            return this;
        }

        public Builder setTrainingPercent(Double trainingPercent) {
            this.trainingPercent = trainingPercent;
            return this;
        }

        public Classification build() {
            return new Classification(dependentVariable, lambda, gamma, eta, maximumNumberTrees, featureBagFraction, predictionFieldName,
                trainingPercent);
        }
    }
}
