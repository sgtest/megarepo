/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.dataframe.evaluation.classification;

import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.search.aggregations.AggregationBuilder;
import org.elasticsearch.search.aggregations.AggregationBuilders;
import org.elasticsearch.search.aggregations.Aggregations;
import org.elasticsearch.search.aggregations.BucketOrder;
import org.elasticsearch.search.aggregations.bucket.filter.Filters;
import org.elasticsearch.search.aggregations.bucket.filter.FiltersAggregator.KeyedFilter;
import org.elasticsearch.search.aggregations.bucket.terms.Terms;
import org.elasticsearch.search.aggregations.metrics.Cardinality;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.EvaluationMetricResult;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.stream.Collectors;

import static java.util.Comparator.comparing;
import static org.elasticsearch.common.xcontent.ConstructingObjectParser.constructorArg;
import static org.elasticsearch.common.xcontent.ConstructingObjectParser.optionalConstructorArg;

/**
 * {@link MulticlassConfusionMatrix} is a metric that answers the question:
 *   "How many examples belonging to class X were classified as Y by the classifier?"
 * for all the possible class pairs {X, Y}.
 */
public class MulticlassConfusionMatrix implements ClassificationMetric {

    public static final ParseField NAME = new ParseField("multiclass_confusion_matrix");

    public static final ParseField SIZE = new ParseField("size");

    private static final ConstructingObjectParser<MulticlassConfusionMatrix, Void> PARSER = createParser();

    private static ConstructingObjectParser<MulticlassConfusionMatrix, Void> createParser() {
        ConstructingObjectParser<MulticlassConfusionMatrix, Void>  parser =
            new ConstructingObjectParser<>(NAME.getPreferredName(), true, args -> new MulticlassConfusionMatrix((Integer) args[0]));
        parser.declareInt(optionalConstructorArg(), SIZE);
        return parser;
    }

    public static MulticlassConfusionMatrix fromXContent(XContentParser parser) {
        return PARSER.apply(parser, null);
    }

    private static final String STEP_1_AGGREGATE_BY_ACTUAL_CLASS = NAME.getPreferredName() + "_step_1_by_actual_class";
    private static final String STEP_2_AGGREGATE_BY_ACTUAL_CLASS = NAME.getPreferredName() + "_step_2_by_actual_class";
    private static final String STEP_2_AGGREGATE_BY_PREDICTED_CLASS = NAME.getPreferredName() + "_step_2_by_predicted_class";
    private static final String STEP_2_CARDINALITY_OF_ACTUAL_CLASS = NAME.getPreferredName() + "_step_2_cardinality_of_actual_class";
    private static final String OTHER_BUCKET_KEY = "_other_";
    private static final int DEFAULT_SIZE = 10;
    private static final int MAX_SIZE = 1000;

    private final int size;
    private List<String> topActualClassNames;
    private Result result;

    public MulticlassConfusionMatrix() {
        this((Integer) null);
    }

    public MulticlassConfusionMatrix(@Nullable Integer size) {
        if (size != null && (size <= 0 || size > MAX_SIZE)) {
            throw ExceptionsHelper.badRequestException("[{}] must be an integer in [1, {}]", SIZE.getPreferredName(), MAX_SIZE);
        }
        this.size = size != null ? size : DEFAULT_SIZE;
    }

    public MulticlassConfusionMatrix(StreamInput in) throws IOException {
        this.size = in.readVInt();
    }

    @Override
    public String getWriteableName() {
        return NAME.getPreferredName();
    }

    @Override
    public String getName() {
        return NAME.getPreferredName();
    }

    public int getSize() {
        return size;
    }

    @Override
    public final List<AggregationBuilder> aggs(String actualField, String predictedField) {
        if (topActualClassNames == null) {  // This is step 1
            return List.of(
                AggregationBuilders.terms(STEP_1_AGGREGATE_BY_ACTUAL_CLASS)
                    .field(actualField)
                    .order(List.of(BucketOrder.count(false), BucketOrder.key(true)))
                    .size(size));
        }
        if (result == null) {  // This is step 2
            KeyedFilter[] keyedFiltersActual =
                topActualClassNames.stream()
                    .map(className -> new KeyedFilter(className, QueryBuilders.termQuery(actualField, className)))
                    .toArray(KeyedFilter[]::new);
            KeyedFilter[] keyedFiltersPredicted =
                topActualClassNames.stream()
                    .map(className -> new KeyedFilter(className, QueryBuilders.termQuery(predictedField, className)))
                    .toArray(KeyedFilter[]::new);
            return List.of(
                AggregationBuilders.cardinality(STEP_2_CARDINALITY_OF_ACTUAL_CLASS)
                    .field(actualField),
                AggregationBuilders.filters(STEP_2_AGGREGATE_BY_ACTUAL_CLASS, keyedFiltersActual)
                    .subAggregation(AggregationBuilders.filters(STEP_2_AGGREGATE_BY_PREDICTED_CLASS, keyedFiltersPredicted)
                        .otherBucket(true)
                        .otherBucketKey(OTHER_BUCKET_KEY)));
        }
        return List.of();
    }

    @Override
    public void process(Aggregations aggs) {
        if (topActualClassNames == null && aggs.get(STEP_1_AGGREGATE_BY_ACTUAL_CLASS) != null) {
            Terms termsAgg = aggs.get(STEP_1_AGGREGATE_BY_ACTUAL_CLASS);
            topActualClassNames = termsAgg.getBuckets().stream().map(Terms.Bucket::getKeyAsString).sorted().collect(Collectors.toList());
        }
        if (result == null && aggs.get(STEP_2_AGGREGATE_BY_ACTUAL_CLASS) != null) {
            Cardinality cardinalityAgg = aggs.get(STEP_2_CARDINALITY_OF_ACTUAL_CLASS);
            Filters filtersAgg = aggs.get(STEP_2_AGGREGATE_BY_ACTUAL_CLASS);
            List<ActualClass> actualClasses = new ArrayList<>(filtersAgg.getBuckets().size());
            for (Filters.Bucket bucket : filtersAgg.getBuckets()) {
                String actualClass = bucket.getKeyAsString();
                long actualClassDocCount = bucket.getDocCount();
                Filters subAgg = bucket.getAggregations().get(STEP_2_AGGREGATE_BY_PREDICTED_CLASS);
                List<PredictedClass> predictedClasses = new ArrayList<>();
                long otherPredictedClassDocCount = 0;
                for (Filters.Bucket subBucket : subAgg.getBuckets()) {
                    String predictedClass = subBucket.getKeyAsString();
                    long docCount = subBucket.getDocCount();
                    if (OTHER_BUCKET_KEY.equals(predictedClass)) {
                        otherPredictedClassDocCount = docCount;
                    } else {
                        predictedClasses.add(new PredictedClass(predictedClass, docCount));
                    }
                }
                predictedClasses.sort(comparing(PredictedClass::getPredictedClass));
                actualClasses.add(new ActualClass(actualClass, actualClassDocCount, predictedClasses, otherPredictedClassDocCount));
            }
            result = new Result(actualClasses, Math.max(cardinalityAgg.getValue() - size, 0));
        }
    }

    @Override
    public Optional<EvaluationMetricResult> getResult() {
        return Optional.ofNullable(result);
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeVInt(size);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.field(SIZE.getPreferredName(), size);
        builder.endObject();
        return builder;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        MulticlassConfusionMatrix that = (MulticlassConfusionMatrix) o;
        return Objects.equals(this.size, that.size);
    }

    @Override
    public int hashCode() {
        return Objects.hash(size);
    }

    public static class Result implements EvaluationMetricResult {

        private static final ParseField CONFUSION_MATRIX = new ParseField("confusion_matrix");
        private static final ParseField OTHER_ACTUAL_CLASS_COUNT = new ParseField("other_actual_class_count");

        @SuppressWarnings("unchecked")
        private static final ConstructingObjectParser<Result, Void> PARSER =
            new ConstructingObjectParser<>(
                "multiclass_confusion_matrix_result", true, a -> new Result((List<ActualClass>) a[0], (long) a[1]));

        static {
            PARSER.declareObjectArray(constructorArg(), ActualClass.PARSER, CONFUSION_MATRIX);
            PARSER.declareLong(constructorArg(), OTHER_ACTUAL_CLASS_COUNT);
        }

        public static Result fromXContent(XContentParser parser) {
            return PARSER.apply(parser, null);
        }

        /** List of actual classes. */
        private final List<ActualClass> actualClasses;
        /** Number of actual classes that were not included in the confusion matrix because there were too many of them. */
        private final long otherActualClassCount;

        public Result(List<ActualClass> actualClasses, long otherActualClassCount) {
            this.actualClasses = Collections.unmodifiableList(ExceptionsHelper.requireNonNull(actualClasses, CONFUSION_MATRIX));
            this.otherActualClassCount = requireNonNegative(otherActualClassCount, OTHER_ACTUAL_CLASS_COUNT);
        }

        public Result(StreamInput in) throws IOException {
            this.actualClasses = Collections.unmodifiableList(in.readList(ActualClass::new));
            this.otherActualClassCount = in.readVLong();
        }

        @Override
        public String getWriteableName() {
            return NAME.getPreferredName();
        }

        @Override
        public String getMetricName() {
            return NAME.getPreferredName();
        }

        public List<ActualClass> getConfusionMatrix() {
            return actualClasses;
        }

        public long getOtherActualClassCount() {
            return otherActualClassCount;
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            out.writeList(actualClasses);
            out.writeVLong(otherActualClassCount);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field(CONFUSION_MATRIX.getPreferredName(), actualClasses);
            builder.field(OTHER_ACTUAL_CLASS_COUNT.getPreferredName(), otherActualClassCount);
            builder.endObject();
            return builder;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            Result that = (Result) o;
            return Objects.equals(this.actualClasses, that.actualClasses)
                && this.otherActualClassCount == that.otherActualClassCount;
        }

        @Override
        public int hashCode() {
            return Objects.hash(actualClasses, otherActualClassCount);
        }
    }

    public static class ActualClass implements ToXContentObject, Writeable {

        private static final ParseField ACTUAL_CLASS = new ParseField("actual_class");
        private static final ParseField ACTUAL_CLASS_DOC_COUNT = new ParseField("actual_class_doc_count");
        private static final ParseField PREDICTED_CLASSES = new ParseField("predicted_classes");
        private static final ParseField OTHER_PREDICTED_CLASS_DOC_COUNT = new ParseField("other_predicted_class_doc_count");

        @SuppressWarnings("unchecked")
        private static final ConstructingObjectParser<ActualClass, Void> PARSER =
            new ConstructingObjectParser<>(
                "multiclass_confusion_matrix_actual_class",
                true,
                a -> new ActualClass((String) a[0], (long) a[1], (List<PredictedClass>) a[2], (long) a[3]));

        static {
            PARSER.declareString(constructorArg(), ACTUAL_CLASS);
            PARSER.declareLong(constructorArg(), ACTUAL_CLASS_DOC_COUNT);
            PARSER.declareObjectArray(constructorArg(), PredictedClass.PARSER, PREDICTED_CLASSES);
            PARSER.declareLong(constructorArg(), OTHER_PREDICTED_CLASS_DOC_COUNT);
        }

        /** Name of the actual class. */
        private final String actualClass;
        /** Number of documents (examples) belonging to the {code actualClass} class. */
        private final long actualClassDocCount;
        /** List of predicted classes. */
        private final List<PredictedClass> predictedClasses;
        /** Number of documents that were not predicted as any of the {@code predictedClasses}. */
        private final long otherPredictedClassDocCount;

        public ActualClass(
                String actualClass, long actualClassDocCount, List<PredictedClass> predictedClasses, long otherPredictedClassDocCount) {
            this.actualClass = ExceptionsHelper.requireNonNull(actualClass, ACTUAL_CLASS);
            this.actualClassDocCount = requireNonNegative(actualClassDocCount, ACTUAL_CLASS_DOC_COUNT);
            this.predictedClasses = Collections.unmodifiableList(ExceptionsHelper.requireNonNull(predictedClasses, PREDICTED_CLASSES));
            this.otherPredictedClassDocCount = requireNonNegative(otherPredictedClassDocCount, OTHER_PREDICTED_CLASS_DOC_COUNT);
        }

        public ActualClass(StreamInput in) throws IOException {
            this.actualClass = in.readString();
            this.actualClassDocCount = in.readVLong();
            this.predictedClasses = Collections.unmodifiableList(in.readList(PredictedClass::new));
            this.otherPredictedClassDocCount = in.readVLong();
        }

        public String getActualClass() {
            return actualClass;
        }

        public List<PredictedClass> getPredictedClasses() {
            return predictedClasses;
        }

        public long getOtherPredictedClassDocCount() {
            return otherPredictedClassDocCount;
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            out.writeString(actualClass);
            out.writeVLong(actualClassDocCount);
            out.writeList(predictedClasses);
            out.writeVLong(otherPredictedClassDocCount);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field(ACTUAL_CLASS.getPreferredName(), actualClass);
            builder.field(ACTUAL_CLASS_DOC_COUNT.getPreferredName(), actualClassDocCount);
            builder.field(PREDICTED_CLASSES.getPreferredName(), predictedClasses);
            builder.field(OTHER_PREDICTED_CLASS_DOC_COUNT.getPreferredName(), otherPredictedClassDocCount);
            builder.endObject();
            return builder;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            ActualClass that = (ActualClass) o;
            return Objects.equals(this.actualClass, that.actualClass)
                && this.actualClassDocCount == that.actualClassDocCount
                && Objects.equals(this.predictedClasses, that.predictedClasses)
                && this.otherPredictedClassDocCount == that.otherPredictedClassDocCount;
        }

        @Override
        public int hashCode() {
            return Objects.hash(actualClass, actualClassDocCount, predictedClasses, otherPredictedClassDocCount);
        }
    }

    public static class PredictedClass implements ToXContentObject, Writeable {

        private static final ParseField PREDICTED_CLASS = new ParseField("predicted_class");
        private static final ParseField COUNT = new ParseField("count");

        @SuppressWarnings("unchecked")
        private static final ConstructingObjectParser<PredictedClass, Void> PARSER =
            new ConstructingObjectParser<>(
                "multiclass_confusion_matrix_predicted_class", true, a -> new PredictedClass((String) a[0], (long) a[1]));

        static {
            PARSER.declareString(constructorArg(), PREDICTED_CLASS);
            PARSER.declareLong(constructorArg(), COUNT);
        }

        private final String predictedClass;
        private final long count;

        public PredictedClass(String predictedClass, long count) {
            this.predictedClass = ExceptionsHelper.requireNonNull(predictedClass, PREDICTED_CLASS);
            this.count = requireNonNegative(count, COUNT);
        }

        public PredictedClass(StreamInput in) throws IOException {
            this.predictedClass = in.readString();
            this.count = in.readVLong();
        }

        public String getPredictedClass() {
            return predictedClass;
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            out.writeString(predictedClass);
            out.writeVLong(count);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field(PREDICTED_CLASS.getPreferredName(), predictedClass);
            builder.field(COUNT.getPreferredName(), count);
            builder.endObject();
            return builder;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            PredictedClass that = (PredictedClass) o;
            return Objects.equals(this.predictedClass, that.predictedClass)
                && this.count == that.count;
        }

        @Override
        public int hashCode() {
            return Objects.hash(predictedClass, count);
        }
    }

    private static long requireNonNegative(long value, ParseField field) {
        if (value < 0) {
            throw ExceptionsHelper.serverError("[" + field.getPreferredName() + "] must be >= 0, was: " + value);
        }
        return value;
    }
}
