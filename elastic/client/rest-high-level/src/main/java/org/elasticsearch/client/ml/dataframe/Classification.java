/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.client.ml.dataframe;

import org.elasticsearch.client.ml.inference.NamedXContentObjectHelper;
import org.elasticsearch.client.ml.inference.preprocessing.PreProcessor;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;

import java.io.IOException;
import java.util.List;
import java.util.Locale;
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
    static final ParseField MAX_TREES = new ParseField("max_trees");
    static final ParseField FEATURE_BAG_FRACTION = new ParseField("feature_bag_fraction");
    static final ParseField NUM_TOP_FEATURE_IMPORTANCE_VALUES = new ParseField("num_top_feature_importance_values");
    static final ParseField PREDICTION_FIELD_NAME = new ParseField("prediction_field_name");
    static final ParseField TRAINING_PERCENT = new ParseField("training_percent");
    static final ParseField CLASS_ASSIGNMENT_OBJECTIVE = new ParseField("class_assignment_objective");
    static final ParseField NUM_TOP_CLASSES = new ParseField("num_top_classes");
    static final ParseField RANDOMIZE_SEED = new ParseField("randomize_seed");
    static final ParseField FEATURE_PROCESSORS = new ParseField("feature_processors");
    static final ParseField ALPHA = new ParseField("alpha");
    static final ParseField ETA_GROWTH_RATE_PER_TREE = new ParseField("eta_growth_rate_per_tree");
    static final ParseField SOFT_TREE_DEPTH_LIMIT = new ParseField("soft_tree_depth_limit");
    static final ParseField SOFT_TREE_DEPTH_TOLERANCE = new ParseField("soft_tree_depth_tolerance");
    static final ParseField DOWNSAMPLE_FACTOR = new ParseField("downsample_factor");
    static final ParseField MAX_OPTIMIZATION_ROUNDS_PER_HYPERPARAMETER = new ParseField("max_optimization_rounds_per_hyperparameter");
    static final ParseField EARLY_STOPPING_ENABLED = new ParseField("early_stopping_enabled");

    @SuppressWarnings("unchecked")
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
                (Integer) a[6],
                (String) a[7],
                (Double) a[8],
                (Integer) a[9],
                (Long) a[10],
                (ClassAssignmentObjective) a[11],
                (List<PreProcessor>) a[12],
                (Double) a[13],
                (Double) a[14],
                (Double) a[15],
                (Double) a[16],
                (Double) a[17],
                (Integer) a[18],
                (Boolean) a[19]
            ));

    static {
        PARSER.declareString(ConstructingObjectParser.constructorArg(), DEPENDENT_VARIABLE);
        PARSER.declareDouble(ConstructingObjectParser.optionalConstructorArg(), LAMBDA);
        PARSER.declareDouble(ConstructingObjectParser.optionalConstructorArg(), GAMMA);
        PARSER.declareDouble(ConstructingObjectParser.optionalConstructorArg(), ETA);
        PARSER.declareInt(ConstructingObjectParser.optionalConstructorArg(), MAX_TREES);
        PARSER.declareDouble(ConstructingObjectParser.optionalConstructorArg(), FEATURE_BAG_FRACTION);
        PARSER.declareInt(ConstructingObjectParser.optionalConstructorArg(), NUM_TOP_FEATURE_IMPORTANCE_VALUES);
        PARSER.declareString(ConstructingObjectParser.optionalConstructorArg(), PREDICTION_FIELD_NAME);
        PARSER.declareDouble(ConstructingObjectParser.optionalConstructorArg(), TRAINING_PERCENT);
        PARSER.declareInt(ConstructingObjectParser.optionalConstructorArg(), NUM_TOP_CLASSES);
        PARSER.declareLong(ConstructingObjectParser.optionalConstructorArg(), RANDOMIZE_SEED);
        PARSER.declareString(
            ConstructingObjectParser.optionalConstructorArg(), ClassAssignmentObjective::fromString, CLASS_ASSIGNMENT_OBJECTIVE);
        PARSER.declareNamedObjects(ConstructingObjectParser.optionalConstructorArg(),
            (p, c, n) -> p.namedObject(PreProcessor.class, n, c),
            (classification) -> {},
            FEATURE_PROCESSORS);
        PARSER.declareDouble(ConstructingObjectParser.optionalConstructorArg(), ALPHA);
        PARSER.declareDouble(ConstructingObjectParser.optionalConstructorArg(), ETA_GROWTH_RATE_PER_TREE);
        PARSER.declareDouble(ConstructingObjectParser.optionalConstructorArg(), SOFT_TREE_DEPTH_LIMIT);
        PARSER.declareDouble(ConstructingObjectParser.optionalConstructorArg(), SOFT_TREE_DEPTH_TOLERANCE);
        PARSER.declareDouble(ConstructingObjectParser.optionalConstructorArg(), DOWNSAMPLE_FACTOR);
        PARSER.declareInt(ConstructingObjectParser.optionalConstructorArg(), MAX_OPTIMIZATION_ROUNDS_PER_HYPERPARAMETER);
        PARSER.declareBoolean(ConstructingObjectParser.optionalConstructorArg(), EARLY_STOPPING_ENABLED);
    }

    private final String dependentVariable;
    private final Double lambda;
    private final Double gamma;
    private final Double eta;
    private final Integer maxTrees;
    private final Double featureBagFraction;
    private final Integer numTopFeatureImportanceValues;
    private final String predictionFieldName;
    private final Double trainingPercent;
    private final ClassAssignmentObjective classAssignmentObjective;
    private final Integer numTopClasses;
    private final Long randomizeSeed;
    private final List<PreProcessor> featureProcessors;
    private final Double alpha;
    private final Double etaGrowthRatePerTree;
    private final Double softTreeDepthLimit;
    private final Double softTreeDepthTolerance;
    private final Double downsampleFactor;
    private final Integer maxOptimizationRoundsPerHyperparameter;
    private final Boolean earlyStoppingEnabled;

    private Classification(String dependentVariable, @Nullable Double lambda, @Nullable Double gamma, @Nullable Double eta,
                           @Nullable Integer maxTrees, @Nullable Double featureBagFraction,
                           @Nullable Integer numTopFeatureImportanceValues, @Nullable String predictionFieldName,
                           @Nullable Double trainingPercent, @Nullable Integer numTopClasses, @Nullable Long randomizeSeed,
                           @Nullable ClassAssignmentObjective classAssignmentObjective, @Nullable List<PreProcessor> featureProcessors,
                           @Nullable Double alpha, @Nullable Double etaGrowthRatePerTree, @Nullable Double softTreeDepthLimit,
                           @Nullable Double softTreeDepthTolerance, @Nullable Double downsampleFactor,
                           @Nullable Integer maxOptimizationRoundsPerHyperparameter, @Nullable Boolean earlyStoppingEnabled) {
        this.dependentVariable = Objects.requireNonNull(dependentVariable);
        this.lambda = lambda;
        this.gamma = gamma;
        this.eta = eta;
        this.maxTrees = maxTrees;
        this.featureBagFraction = featureBagFraction;
        this.numTopFeatureImportanceValues = numTopFeatureImportanceValues;
        this.predictionFieldName = predictionFieldName;
        this.trainingPercent = trainingPercent;
        this.classAssignmentObjective = classAssignmentObjective;
        this.numTopClasses = numTopClasses;
        this.randomizeSeed = randomizeSeed;
        this.featureProcessors = featureProcessors;
        this.alpha = alpha;
        this.etaGrowthRatePerTree = etaGrowthRatePerTree;
        this.softTreeDepthLimit = softTreeDepthLimit;
        this.softTreeDepthTolerance = softTreeDepthTolerance;
        this.downsampleFactor = downsampleFactor;
        this.maxOptimizationRoundsPerHyperparameter = maxOptimizationRoundsPerHyperparameter;
        this.earlyStoppingEnabled = earlyStoppingEnabled;
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

    public Integer getMaxTrees() {
        return maxTrees;
    }

    public Double getFeatureBagFraction() {
        return featureBagFraction;
    }

    public Integer getNumTopFeatureImportanceValues() {
        return numTopFeatureImportanceValues;
    }

    public String getPredictionFieldName() {
        return predictionFieldName;
    }

    public Double getTrainingPercent() {
        return trainingPercent;
    }

    public Long getRandomizeSeed() {
        return randomizeSeed;
    }

    public ClassAssignmentObjective getClassAssignmentObjective() {
        return classAssignmentObjective;
    }

    public Integer getNumTopClasses() {
        return numTopClasses;
    }

    public List<PreProcessor> getFeatureProcessors() {
        return featureProcessors;
    }

    public Double getAlpha() {
        return alpha;
    }

    public Double getEtaGrowthRatePerTree() {
        return etaGrowthRatePerTree;
    }

    public Double getSoftTreeDepthLimit() {
        return softTreeDepthLimit;
    }

    public Double getSoftTreeDepthTolerance() {
        return softTreeDepthTolerance;
    }

    public Double getDownsampleFactor() {
        return downsampleFactor;
    }

    public Integer getMaxOptimizationRoundsPerHyperparameter() {
        return maxOptimizationRoundsPerHyperparameter;
    }

    public Boolean getEarlyStoppingEnable() {
        return earlyStoppingEnabled;
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
        if (maxTrees != null) {
            builder.field(MAX_TREES.getPreferredName(), maxTrees);
        }
        if (featureBagFraction != null) {
            builder.field(FEATURE_BAG_FRACTION.getPreferredName(), featureBagFraction);
        }
        if (numTopFeatureImportanceValues != null) {
            builder.field(NUM_TOP_FEATURE_IMPORTANCE_VALUES.getPreferredName(), numTopFeatureImportanceValues);
        }
        if (predictionFieldName != null) {
            builder.field(PREDICTION_FIELD_NAME.getPreferredName(), predictionFieldName);
        }
        if (trainingPercent != null) {
            builder.field(TRAINING_PERCENT.getPreferredName(), trainingPercent);
        }
        if (randomizeSeed != null) {
            builder.field(RANDOMIZE_SEED.getPreferredName(), randomizeSeed);
        }
        if (classAssignmentObjective != null) {
            builder.field(CLASS_ASSIGNMENT_OBJECTIVE.getPreferredName(), classAssignmentObjective);
        }
        if (numTopClasses != null) {
            builder.field(NUM_TOP_CLASSES.getPreferredName(), numTopClasses);
        }
        if (featureProcessors != null) {
            NamedXContentObjectHelper.writeNamedObjects(builder, params, true, FEATURE_PROCESSORS.getPreferredName(), featureProcessors);
        }
        if (alpha != null) {
            builder.field(ALPHA.getPreferredName(), alpha);
        }
        if (etaGrowthRatePerTree != null) {
            builder.field(ETA_GROWTH_RATE_PER_TREE.getPreferredName(), etaGrowthRatePerTree);
        }
        if (softTreeDepthLimit != null) {
            builder.field(SOFT_TREE_DEPTH_LIMIT.getPreferredName(), softTreeDepthLimit);
        }
        if (softTreeDepthTolerance != null) {
            builder.field(SOFT_TREE_DEPTH_TOLERANCE.getPreferredName(), softTreeDepthTolerance);
        }
        if (downsampleFactor != null) {
            builder.field(DOWNSAMPLE_FACTOR.getPreferredName(), downsampleFactor);
        }
        if (maxOptimizationRoundsPerHyperparameter != null) {
            builder.field(MAX_OPTIMIZATION_ROUNDS_PER_HYPERPARAMETER.getPreferredName(), maxOptimizationRoundsPerHyperparameter);
        }
        if (earlyStoppingEnabled != null) {
            builder.field(EARLY_STOPPING_ENABLED.getPreferredName(), earlyStoppingEnabled);
        }
        builder.endObject();
        return builder;
    }

    @Override
    public int hashCode() {
        return Objects.hash(dependentVariable, lambda, gamma, eta, maxTrees, featureBagFraction, numTopFeatureImportanceValues,
            predictionFieldName, trainingPercent, randomizeSeed, numTopClasses, classAssignmentObjective, featureProcessors, alpha,
            etaGrowthRatePerTree, softTreeDepthLimit, softTreeDepthTolerance, downsampleFactor, maxOptimizationRoundsPerHyperparameter,
            earlyStoppingEnabled);
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
            && Objects.equals(maxTrees, that.maxTrees)
            && Objects.equals(featureBagFraction, that.featureBagFraction)
            && Objects.equals(numTopFeatureImportanceValues, that.numTopFeatureImportanceValues)
            && Objects.equals(predictionFieldName, that.predictionFieldName)
            && Objects.equals(trainingPercent, that.trainingPercent)
            && Objects.equals(randomizeSeed, that.randomizeSeed)
            && Objects.equals(numTopClasses, that.numTopClasses)
            && Objects.equals(classAssignmentObjective, that.classAssignmentObjective)
            && Objects.equals(featureProcessors, that.featureProcessors)
            && Objects.equals(alpha, that.alpha)
            && Objects.equals(etaGrowthRatePerTree, that.etaGrowthRatePerTree)
            && Objects.equals(softTreeDepthLimit, that.softTreeDepthLimit)
            && Objects.equals(softTreeDepthTolerance, that.softTreeDepthTolerance)
            && Objects.equals(downsampleFactor, that.downsampleFactor)
            && Objects.equals(maxOptimizationRoundsPerHyperparameter, that.maxOptimizationRoundsPerHyperparameter)
            && Objects.equals(earlyStoppingEnabled, that.earlyStoppingEnabled);
    }

    @Override
    public String toString() {
        return Strings.toString(this);
    }

    public enum ClassAssignmentObjective {
        MAXIMIZE_ACCURACY, MAXIMIZE_MINIMUM_RECALL;

        public static ClassAssignmentObjective fromString(String value) {
            return ClassAssignmentObjective.valueOf(value.toUpperCase(Locale.ROOT));
        }

        @Override
        public String toString() {
            return name().toLowerCase(Locale.ROOT);
        }
    }

    public static class Builder {
        private String dependentVariable;
        private Double lambda;
        private Double gamma;
        private Double eta;
        private Integer maxTrees;
        private Double featureBagFraction;
        private Integer numTopFeatureImportanceValues;
        private String predictionFieldName;
        private Double trainingPercent;
        private Integer numTopClasses;
        private Long randomizeSeed;
        private ClassAssignmentObjective classAssignmentObjective;
        private List<PreProcessor> featureProcessors;
        private Double alpha;
        private Double etaGrowthRatePerTree;
        private Double softTreeDepthLimit;
        private Double softTreeDepthTolerance;
        private Double downsampleFactor;
        private Integer maxOptimizationRoundsPerHyperparameter;
        private Boolean earlyStoppingEnabled;

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

        public Builder setMaxTrees(Integer maxTrees) {
            this.maxTrees = maxTrees;
            return this;
        }

        public Builder setFeatureBagFraction(Double featureBagFraction) {
            this.featureBagFraction = featureBagFraction;
            return this;
        }

        public Builder setNumTopFeatureImportanceValues(Integer numTopFeatureImportanceValues) {
            this.numTopFeatureImportanceValues = numTopFeatureImportanceValues;
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

        public Builder setRandomizeSeed(Long randomizeSeed) {
            this.randomizeSeed = randomizeSeed;
            return this;
        }

        public Builder setNumTopClasses(Integer numTopClasses) {
            this.numTopClasses = numTopClasses;
            return this;
        }

        public Builder setClassAssignmentObjective(ClassAssignmentObjective classAssignmentObjective) {
            this.classAssignmentObjective = classAssignmentObjective;
            return this;
        }

        public Builder setFeatureProcessors(List<PreProcessor> featureProcessors) {
            this.featureProcessors = featureProcessors;
            return this;
        }

        public Builder setAlpha(Double alpha) {
            this.alpha = alpha;
            return this;
        }

        public Builder setEtaGrowthRatePerTree(Double etaGrowthRatePerTree) {
            this.etaGrowthRatePerTree = etaGrowthRatePerTree;
            return this;
        }

        public Builder setSoftTreeDepthLimit(Double softTreeDepthLimit) {
            this.softTreeDepthLimit = softTreeDepthLimit;
            return this;
        }

        public Builder setSoftTreeDepthTolerance(Double softTreeDepthTolerance) {
            this.softTreeDepthTolerance = softTreeDepthTolerance;
            return this;
        }

        public Builder setDownsampleFactor(Double downsampleFactor) {
            this.downsampleFactor = downsampleFactor;
            return this;
        }

        public Builder setMaxOptimizationRoundsPerHyperparameter(Integer maxOptimizationRoundsPerHyperparameter) {
            this.maxOptimizationRoundsPerHyperparameter = maxOptimizationRoundsPerHyperparameter;
            return this;
        }

        public Builder setEarlyStoppingEnabled(Boolean earlyStoppingEnabled) {
            this.earlyStoppingEnabled = earlyStoppingEnabled;
            return this;
        }

        public Classification build() {
            return new Classification(dependentVariable, lambda, gamma, eta, maxTrees, featureBagFraction,
                numTopFeatureImportanceValues, predictionFieldName, trainingPercent, numTopClasses, randomizeSeed,
                classAssignmentObjective, featureProcessors, alpha, etaGrowthRatePerTree, softTreeDepthLimit, softTreeDepthTolerance,
                downsampleFactor, maxOptimizationRoundsPerHyperparameter, earlyStoppingEnabled);
        }
    }
}
