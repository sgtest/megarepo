/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.job.config;

import org.elasticsearch.Version;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.xpack.core.ml.job.messages.Messages;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.core.ml.utils.time.TimeUtils;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Objects;
import java.util.Set;
import java.util.SortedSet;
import java.util.TreeSet;
import java.util.concurrent.TimeUnit;
import java.util.function.Function;
import java.util.regex.Pattern;
import java.util.regex.PatternSyntaxException;
import java.util.stream.Collectors;

/**
 * Autodetect analysis configuration options describes which fields are
 * analysed and the functions to use.
 * <p>
 * The configuration can contain multiple detectors, a new anomaly detector will
 * be created for each detector configuration. The fields
 * <code>bucketSpan, summaryCountFieldName and categorizationFieldName</code>
 * apply to all detectors.
 * <p>
 * If a value has not been set it will be <code>null</code>
 * Object wrappers are used around integral types &amp; booleans so they can take
 * <code>null</code> values.
 */
public class AnalysisConfig implements ToXContentObject, Writeable {
    /**
     * Serialisation names
     */
    public static final ParseField ANALYSIS_CONFIG = new ParseField("analysis_config");
    private static final ParseField BUCKET_SPAN = new ParseField("bucket_span");
    private static final ParseField CATEGORIZATION_FIELD_NAME = new ParseField("categorization_field_name");
    static final ParseField CATEGORIZATION_FILTERS = new ParseField("categorization_filters");
    private static final ParseField CATEGORIZATION_ANALYZER = CategorizationAnalyzerConfig.CATEGORIZATION_ANALYZER;
    private static final ParseField LATENCY = new ParseField("latency");
    private static final ParseField SUMMARY_COUNT_FIELD_NAME = new ParseField("summary_count_field_name");
    private static final ParseField DETECTORS = new ParseField("detectors");
    private static final ParseField INFLUENCERS = new ParseField("influencers");
    private static final ParseField OVERLAPPING_BUCKETS = new ParseField("overlapping_buckets");
    private static final ParseField RESULT_FINALIZATION_WINDOW = new ParseField("result_finalization_window");
    private static final ParseField MULTIVARIATE_BY_FIELDS = new ParseField("multivariate_by_fields");
    private static final ParseField MULTIPLE_BUCKET_SPANS = new ParseField("multiple_bucket_spans");
    private static final ParseField USER_PER_PARTITION_NORMALIZATION = new ParseField("use_per_partition_normalization");

    public static final String ML_CATEGORY_FIELD = "mlcategory";
    public static final Set<String> AUTO_CREATED_FIELDS = new HashSet<>(Collections.singletonList(ML_CATEGORY_FIELD));

    public static final long DEFAULT_RESULT_FINALIZATION_WINDOW = 2L;

    // These parsers follow the pattern that metadata is parsed leniently (to allow for enhancements), whilst config is parsed strictly
    public static final ConstructingObjectParser<AnalysisConfig.Builder, Void> LENIENT_PARSER = createParser(true);
    public static final ConstructingObjectParser<AnalysisConfig.Builder, Void> STRICT_PARSER = createParser(false);

    @SuppressWarnings("unchecked")
    private static ConstructingObjectParser<AnalysisConfig.Builder, Void> createParser(boolean ignoreUnknownFields) {
        ConstructingObjectParser<AnalysisConfig.Builder, Void> parser = new ConstructingObjectParser<>(ANALYSIS_CONFIG.getPreferredName(),
            ignoreUnknownFields, a -> new AnalysisConfig.Builder((List<Detector>) a[0]));

        parser.declareObjectArray(ConstructingObjectParser.constructorArg(),
            (p, c) -> (ignoreUnknownFields ? Detector.LENIENT_PARSER : Detector.STRICT_PARSER).apply(p, c).build(), DETECTORS);
        parser.declareString((builder, val) ->
            builder.setBucketSpan(TimeValue.parseTimeValue(val, BUCKET_SPAN.getPreferredName())), BUCKET_SPAN);
        parser.declareString(Builder::setCategorizationFieldName, CATEGORIZATION_FIELD_NAME);
        parser.declareStringArray(Builder::setCategorizationFilters, CATEGORIZATION_FILTERS);
        // This one is nasty - the syntax for analyzers takes either names or objects at many levels, hence it's not
        // possible to simply declare whether the field is a string or object and a completely custom parser is required
        parser.declareField(Builder::setCategorizationAnalyzerConfig,
            (p, c) -> CategorizationAnalyzerConfig.buildFromXContentFragment(p, ignoreUnknownFields),
            CATEGORIZATION_ANALYZER, ObjectParser.ValueType.OBJECT_OR_STRING);
        parser.declareString((builder, val) ->
            builder.setLatency(TimeValue.parseTimeValue(val, LATENCY.getPreferredName())), LATENCY);
        parser.declareString(Builder::setSummaryCountFieldName, SUMMARY_COUNT_FIELD_NAME);
        parser.declareStringArray(Builder::setInfluencers, INFLUENCERS);
        parser.declareBoolean(Builder::setOverlappingBuckets, OVERLAPPING_BUCKETS);
        parser.declareLong(Builder::setResultFinalizationWindow, RESULT_FINALIZATION_WINDOW);
        parser.declareBoolean(Builder::setMultivariateByFields, MULTIVARIATE_BY_FIELDS);
        parser.declareStringArray((builder, values) -> builder.setMultipleBucketSpans(
            values.stream().map(v -> TimeValue.parseTimeValue(v, MULTIPLE_BUCKET_SPANS.getPreferredName()))
                .collect(Collectors.toList())), MULTIPLE_BUCKET_SPANS);
        parser.declareBoolean(Builder::setUsePerPartitionNormalization, USER_PER_PARTITION_NORMALIZATION);

        return parser;
    }

    /**
     * These values apply to all detectors
     */
    private final TimeValue bucketSpan;
    private final String categorizationFieldName;
    private final List<String> categorizationFilters;
    private final CategorizationAnalyzerConfig categorizationAnalyzerConfig;
    private final TimeValue latency;
    private final String summaryCountFieldName;
    private final List<Detector> detectors;
    private final List<String> influencers;
    private final Boolean overlappingBuckets;
    private final Long resultFinalizationWindow;
    private final Boolean multivariateByFields;
    private final List<TimeValue> multipleBucketSpans;
    private final boolean usePerPartitionNormalization;

    private AnalysisConfig(TimeValue bucketSpan, String categorizationFieldName, List<String> categorizationFilters,
                           CategorizationAnalyzerConfig categorizationAnalyzerConfig, TimeValue latency, String summaryCountFieldName,
                           List<Detector> detectors, List<String> influencers, Boolean overlappingBuckets, Long resultFinalizationWindow,
                           Boolean multivariateByFields, List<TimeValue> multipleBucketSpans, boolean usePerPartitionNormalization) {
        this.detectors = detectors;
        this.bucketSpan = bucketSpan;
        this.latency = latency;
        this.categorizationFieldName = categorizationFieldName;
        this.categorizationAnalyzerConfig = categorizationAnalyzerConfig;
        this.categorizationFilters = categorizationFilters == null ? null : Collections.unmodifiableList(categorizationFilters);
        this.summaryCountFieldName = summaryCountFieldName;
        this.influencers = Collections.unmodifiableList(influencers);
        this.overlappingBuckets = overlappingBuckets;
        this.resultFinalizationWindow = resultFinalizationWindow;
        this.multivariateByFields = multivariateByFields;
        this.multipleBucketSpans = multipleBucketSpans == null ? null : Collections.unmodifiableList(multipleBucketSpans);
        this.usePerPartitionNormalization = usePerPartitionNormalization;
    }

    public AnalysisConfig(StreamInput in) throws IOException {
        bucketSpan = in.readTimeValue();
        categorizationFieldName = in.readOptionalString();
        categorizationFilters = in.readBoolean() ? Collections.unmodifiableList(in.readList(StreamInput::readString)) : null;
        if (in.getVersion().onOrAfter(Version.V_6_2_0)) {
            categorizationAnalyzerConfig = in.readOptionalWriteable(CategorizationAnalyzerConfig::new);
        } else {
            categorizationAnalyzerConfig = null;
        }
        latency = in.readOptionalTimeValue();
        summaryCountFieldName = in.readOptionalString();
        detectors = Collections.unmodifiableList(in.readList(Detector::new));
        influencers = Collections.unmodifiableList(in.readList(StreamInput::readString));
        overlappingBuckets = in.readOptionalBoolean();
        resultFinalizationWindow = in.readOptionalLong();
        multivariateByFields = in.readOptionalBoolean();
        if (in.readBoolean()) {
            final int arraySize = in.readVInt();
            final List<TimeValue> spans = new ArrayList<>(arraySize);
            for (int i = 0; i < arraySize; i++) {
                spans.add(in.readTimeValue());
            }
            multipleBucketSpans = Collections.unmodifiableList(spans);
        } else {
            multipleBucketSpans = null;
        }
        usePerPartitionNormalization = in.readBoolean();
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeTimeValue(bucketSpan);
        out.writeOptionalString(categorizationFieldName);
        if (categorizationFilters != null) {
            out.writeBoolean(true);
            out.writeStringList(categorizationFilters);
        } else {
            out.writeBoolean(false);
        }
        if (out.getVersion().onOrAfter(Version.V_6_2_0)) {
            out.writeOptionalWriteable(categorizationAnalyzerConfig);
        }
        out.writeOptionalTimeValue(latency);
        out.writeOptionalString(summaryCountFieldName);
        out.writeList(detectors);
        out.writeStringList(influencers);
        out.writeOptionalBoolean(overlappingBuckets);
        out.writeOptionalLong(resultFinalizationWindow);
        out.writeOptionalBoolean(multivariateByFields);
        if (multipleBucketSpans != null) {
            out.writeBoolean(true);
            out.writeVInt(multipleBucketSpans.size());
            for (TimeValue span : multipleBucketSpans) {
                out.writeTimeValue(span);
            }
        } else {
            out.writeBoolean(false);
        }
        out.writeBoolean(usePerPartitionNormalization);
    }

    /**
     * The analysis bucket span
     *
     * @return The bucketspan or <code>null</code> if not set
     */
    public TimeValue getBucketSpan() {
        return bucketSpan;
    }

    public String getCategorizationFieldName() {
        return categorizationFieldName;
    }

    public List<String> getCategorizationFilters() {
        return categorizationFilters;
    }

    public CategorizationAnalyzerConfig getCategorizationAnalyzerConfig() {
        return categorizationAnalyzerConfig;
    }

    /**
     * The latency interval during which out-of-order records should be handled.
     *
     * @return The latency interval or <code>null</code> if not set
     */
    public TimeValue getLatency() {
        return latency;
    }

    /**
     * The name of the field that contains counts for pre-summarised input
     *
     * @return The field name or <code>null</code> if not set
     */
    public String getSummaryCountFieldName() {
        return summaryCountFieldName;
    }

    /**
     * The list of analysis detectors. In a valid configuration the list should
     * contain at least 1 {@link Detector}
     *
     * @return The Detectors used in this job
     */
    public List<Detector> getDetectors() {
        return detectors;
    }

    /**
     * The list of influence field names
     */
    public List<String> getInfluencers() {
        return influencers;
    }

    /**
     * Return the list of term fields.
     * These are the influencer fields, partition field,
     * by field and over field of each detector.
     * <code>null</code> and empty strings are filtered from the
     * config.
     *
     * @return Set of term fields - never <code>null</code>
     */
    public Set<String> termFields() {
        return termFields(getDetectors(), getInfluencers());
    }

    static SortedSet<String> termFields(List<Detector> detectors, List<String> influencers) {
        SortedSet<String> termFields = new TreeSet<>();

        detectors.forEach(d -> termFields.addAll(d.getByOverPartitionTerms()));

        for (String i : influencers) {
            addIfNotNull(termFields, i);
        }

        // remove empty strings
        termFields.remove("");

        return termFields;
    }

    public Set<String> extractReferencedFilters() {
        return detectors.stream().map(Detector::extractReferencedFilters)
                .flatMap(Set::stream).collect(Collectors.toSet());
    }

    public Boolean getOverlappingBuckets() {
        return overlappingBuckets;
    }

    public Long getResultFinalizationWindow() {
        return resultFinalizationWindow;
    }

    public Boolean getMultivariateByFields() {
        return multivariateByFields;
    }

    public List<TimeValue> getMultipleBucketSpans() {
        return multipleBucketSpans;
    }

    public boolean getUsePerPartitionNormalization() {
        return usePerPartitionNormalization;
    }

    /**
     * Return the set of fields required by the analysis.
     * These are the influencer fields, metric field, partition field,
     * by field and over field of each detector, plus the summary count
     * field and the categorization field name of the job.
     * <code>null</code> and empty strings are filtered from the
     * config.
     *
     * @return Set of required analysis fields - never <code>null</code>
     */
    public Set<String> analysisFields() {
        Set<String> analysisFields = termFields();

        addIfNotNull(analysisFields, categorizationFieldName);
        addIfNotNull(analysisFields, summaryCountFieldName);

        for (Detector d : getDetectors()) {
            addIfNotNull(analysisFields, d.getFieldName());
        }

        // remove empty strings
        analysisFields.remove("");

        return analysisFields;
    }

    private static void addIfNotNull(Set<String> fields, String field) {
        if (field != null) {
            fields.add(field);
        }
    }

    public List<String> fields() {
        return collectNonNullAndNonEmptyDetectorFields(Detector::getFieldName);
    }

    private List<String> collectNonNullAndNonEmptyDetectorFields(
            Function<Detector, String> fieldGetter) {
        Set<String> fields = new HashSet<>();

        for (Detector d : getDetectors()) {
            addIfNotNull(fields, fieldGetter.apply(d));
        }

        // remove empty strings
        fields.remove("");

        return new ArrayList<>(fields);
    }

    public List<String> byFields() {
        return collectNonNullAndNonEmptyDetectorFields(Detector::getByFieldName);
    }

    public List<String> overFields() {
        return collectNonNullAndNonEmptyDetectorFields(Detector::getOverFieldName);
    }


    public List<String> partitionFields() {
        return collectNonNullAndNonEmptyDetectorFields(Detector::getPartitionFieldName);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.field(BUCKET_SPAN.getPreferredName(), bucketSpan.getStringRep());
        if (categorizationFieldName != null) {
            builder.field(CATEGORIZATION_FIELD_NAME.getPreferredName(), categorizationFieldName);
        }
        if (categorizationFilters != null) {
            builder.field(CATEGORIZATION_FILTERS.getPreferredName(), categorizationFilters);
        }
        if (categorizationAnalyzerConfig != null) {
            // This cannot be builder.field(CATEGORIZATION_ANALYZER.getPreferredName(), categorizationAnalyzerConfig, params);
            // because that always writes categorizationAnalyzerConfig as an object, and in the case of a global analyzer it
            // gets written as a single string.
            categorizationAnalyzerConfig.toXContent(builder, params);
        }
        if (latency != null) {
            builder.field(LATENCY.getPreferredName(), latency.getStringRep());
        }
        if (summaryCountFieldName != null) {
            builder.field(SUMMARY_COUNT_FIELD_NAME.getPreferredName(), summaryCountFieldName);
        }
        builder.startArray(DETECTORS.getPreferredName());
        for (Detector detector: detectors) {
            detector.toXContent(builder, params);
        }
        builder.endArray();
        builder.field(INFLUENCERS.getPreferredName(), influencers);
        if (overlappingBuckets != null) {
            builder.field(OVERLAPPING_BUCKETS.getPreferredName(), overlappingBuckets);
        }
        if (resultFinalizationWindow != null) {
            builder.field(RESULT_FINALIZATION_WINDOW.getPreferredName(), resultFinalizationWindow);
        }
        if (multivariateByFields != null) {
            builder.field(MULTIVARIATE_BY_FIELDS.getPreferredName(), multivariateByFields);
        }
        if (multipleBucketSpans != null) {
            builder.field(MULTIPLE_BUCKET_SPANS.getPreferredName(),
                    multipleBucketSpans.stream().map(TimeValue::getStringRep).collect(Collectors.toList()));
        }
        if (usePerPartitionNormalization) {
            builder.field(USER_PER_PARTITION_NORMALIZATION.getPreferredName(), usePerPartitionNormalization);
        }
        builder.endObject();
        return builder;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        AnalysisConfig that = (AnalysisConfig) o;
        return Objects.equals(latency, that.latency) &&
                usePerPartitionNormalization == that.usePerPartitionNormalization &&
                Objects.equals(bucketSpan, that.bucketSpan) &&
                Objects.equals(categorizationFieldName, that.categorizationFieldName) &&
                Objects.equals(categorizationFilters, that.categorizationFilters) &&
                Objects.equals(categorizationAnalyzerConfig, that.categorizationAnalyzerConfig) &&
                Objects.equals(summaryCountFieldName, that.summaryCountFieldName) &&
                Objects.equals(detectors, that.detectors) &&
                Objects.equals(influencers, that.influencers) &&
                Objects.equals(overlappingBuckets, that.overlappingBuckets) &&
                Objects.equals(resultFinalizationWindow, that.resultFinalizationWindow) &&
                Objects.equals(multivariateByFields, that.multivariateByFields) &&
                Objects.equals(multipleBucketSpans, that.multipleBucketSpans);
    }

    @Override
    public int hashCode() {
        return Objects.hash(
                bucketSpan, categorizationFieldName, categorizationFilters, categorizationAnalyzerConfig, latency,
                summaryCountFieldName, detectors, influencers, overlappingBuckets, resultFinalizationWindow,
                multivariateByFields, multipleBucketSpans, usePerPartitionNormalization
        );
    }

    public static class Builder {

        public static final TimeValue DEFAULT_BUCKET_SPAN = TimeValue.timeValueMinutes(5);

        private List<Detector> detectors;
        private TimeValue bucketSpan = DEFAULT_BUCKET_SPAN;
        private TimeValue latency;
        private String categorizationFieldName;
        private List<String> categorizationFilters;
        private CategorizationAnalyzerConfig categorizationAnalyzerConfig;
        private String summaryCountFieldName;
        private List<String> influencers = new ArrayList<>();
        private Boolean overlappingBuckets;
        private Long resultFinalizationWindow;
        private Boolean multivariateByFields;
        private List<TimeValue> multipleBucketSpans;
        private boolean usePerPartitionNormalization = false;

        public Builder(List<Detector> detectors) {
            setDetectors(detectors);
        }

        public Builder(AnalysisConfig analysisConfig) {
            this.detectors = new ArrayList<>(analysisConfig.detectors);
            this.bucketSpan = analysisConfig.bucketSpan;
            this.latency = analysisConfig.latency;
            this.categorizationFieldName = analysisConfig.categorizationFieldName;
            this.categorizationFilters = analysisConfig.categorizationFilters == null ? null
                    : new ArrayList<>(analysisConfig.categorizationFilters);
            this.categorizationAnalyzerConfig = analysisConfig.categorizationAnalyzerConfig;
            this.summaryCountFieldName = analysisConfig.summaryCountFieldName;
            this.influencers = new ArrayList<>(analysisConfig.influencers);
            this.overlappingBuckets = analysisConfig.overlappingBuckets;
            this.resultFinalizationWindow = analysisConfig.resultFinalizationWindow;
            this.multivariateByFields = analysisConfig.multivariateByFields;
            this.multipleBucketSpans = analysisConfig.multipleBucketSpans == null ? null
                    : new ArrayList<>(analysisConfig.multipleBucketSpans);
            this.usePerPartitionNormalization = analysisConfig.usePerPartitionNormalization;
        }

        public void setDetectors(List<Detector> detectors) {
            if (detectors == null) {
                this.detectors = null;
                return;
            }
            // We always assign sequential IDs to the detectors that are correct for this analysis config
            int detectorIndex = 0;
            List<Detector> sequentialIndexDetectors = new ArrayList<>(detectors.size());
            for (Detector origDetector : detectors) {
                Detector.Builder builder = new Detector.Builder(origDetector);
                builder.setDetectorIndex(detectorIndex++);
                sequentialIndexDetectors.add(builder.build());
            }
            this.detectors = sequentialIndexDetectors;
        }

        public void setDetector(int detectorIndex, Detector detector) {
            detectors.set(detectorIndex, detector);
        }

        public void setBucketSpan(TimeValue bucketSpan) {
            this.bucketSpan = bucketSpan;
        }

        public void setLatency(TimeValue latency) {
            this.latency = latency;
        }

        public void setCategorizationFieldName(String categorizationFieldName) {
            this.categorizationFieldName = categorizationFieldName;
        }

        public void setCategorizationFilters(List<String> categorizationFilters) {
            this.categorizationFilters = categorizationFilters;
        }

        public void setCategorizationAnalyzerConfig(CategorizationAnalyzerConfig categorizationAnalyzerConfig) {
            this.categorizationAnalyzerConfig = categorizationAnalyzerConfig;
        }

        public void setSummaryCountFieldName(String summaryCountFieldName) {
            this.summaryCountFieldName = summaryCountFieldName;
        }

        public void setInfluencers(List<String> influencers) {
            this.influencers = ExceptionsHelper.requireNonNull(influencers, INFLUENCERS.getPreferredName());
        }

        public void setOverlappingBuckets(Boolean overlappingBuckets) {
            this.overlappingBuckets = overlappingBuckets;
        }

        public void setResultFinalizationWindow(Long resultFinalizationWindow) {
            this.resultFinalizationWindow = resultFinalizationWindow;
        }

        public void setMultivariateByFields(Boolean multivariateByFields) {
            this.multivariateByFields = multivariateByFields;
        }

        public void setMultipleBucketSpans(List<TimeValue> multipleBucketSpans) {
            this.multipleBucketSpans = multipleBucketSpans;
        }

        public void setUsePerPartitionNormalization(boolean usePerPartitionNormalization) {
            this.usePerPartitionNormalization = usePerPartitionNormalization;
        }

        /**
         * Checks the configuration is valid
         * <ol>
         * <li>Check that if non-null BucketSpan and Latency are &gt;= 0</li>
         * <li>Check that if non-null Latency is &lt;= MAX_LATENCY</li>
         * <li>Check there is at least one detector configured</li>
         * <li>Check all the detectors are configured correctly</li>
         * <li>Check that OVERLAPPING_BUCKETS is set appropriately</li>
         * <li>Check that MULTIPLE_BUCKETSPANS are set appropriately</li>
         * <li>If Per Partition normalization is configured at least one detector
         * must have a partition field and no influences can be used</li>
         * </ol>
         */
        public AnalysisConfig build() {
            TimeUtils.checkPositiveMultiple(bucketSpan, TimeUnit.SECONDS, BUCKET_SPAN);
            if (latency != null) {
                TimeUtils.checkNonNegativeMultiple(latency, TimeUnit.SECONDS, LATENCY);
            }

            verifyDetectorAreDefined();
            Detector.Builder.verifyFieldName(summaryCountFieldName);
            Detector.Builder.verifyFieldName(categorizationFieldName);

            verifyMlCategoryIsUsedWhenCategorizationFieldNameIsSet();
            verifyCategorizationAnalyzer();
            verifyCategorizationFilters();
            checkFieldIsNotNegativeIfSpecified(RESULT_FINALIZATION_WINDOW.getPreferredName(), resultFinalizationWindow);
            verifyMultipleBucketSpans();

            verifyNoMetricFunctionsWhenSummaryCountFieldNameIsSet();

            overlappingBuckets = verifyOverlappingBucketsConfig(overlappingBuckets, detectors);

            if (usePerPartitionNormalization) {
                checkDetectorsHavePartitionFields(detectors);
                checkNoInfluencersAreSet(influencers);
            }

            verifyNoInconsistentNestedFieldNames();

            return new AnalysisConfig(bucketSpan, categorizationFieldName, categorizationFilters, categorizationAnalyzerConfig,
                    latency, summaryCountFieldName, detectors, influencers, overlappingBuckets,
                    resultFinalizationWindow, multivariateByFields, multipleBucketSpans, usePerPartitionNormalization);
        }

        private void verifyNoMetricFunctionsWhenSummaryCountFieldNameIsSet() {
            if (Strings.isNullOrEmpty(summaryCountFieldName) == false &&
                    detectors.stream().anyMatch(d -> DetectorFunction.METRIC.equals(d.getFunction()))) {
                throw ExceptionsHelper.badRequestException(
                        Messages.getMessage(Messages.JOB_CONFIG_FUNCTION_INCOMPATIBLE_PRESUMMARIZED, DetectorFunction.METRIC));
            }
        }

        private static void checkFieldIsNotNegativeIfSpecified(String fieldName, Long value) {
            if (value != null && value < 0) {
                String msg = Messages.getMessage(Messages.JOB_CONFIG_FIELD_VALUE_TOO_LOW, fieldName, 0, value);
                throw ExceptionsHelper.badRequestException(msg);
            }
        }

        private void verifyDetectorAreDefined() {
            if (detectors == null || detectors.isEmpty()) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.JOB_CONFIG_NO_DETECTORS));
            }
        }

        private void verifyNoInconsistentNestedFieldNames() {
            SortedSet<String> termFields = termFields(detectors, influencers);
            // We want to outlaw nested fields where a less nested field clashes with one of the nested levels.
            // For example, this is not allowed:
            // - a
            // - a.b
            // Nor is this:
            // - a.b
            // - a.b.c
            // But this is OK:
            // - a.b
            // - a.c
            // The sorted set makes it relatively easy to detect the situations we want to avoid.
            String prevTermField = null;
            for (String termField : termFields) {
                if (prevTermField != null && termField.startsWith(prevTermField + ".")) {
                    throw ExceptionsHelper.badRequestException("Fields [" + prevTermField + "] and [" + termField +
                            "] cannot both be used in the same analysis_config");
                }
                prevTermField = termField;
            }
        }

        private void verifyMlCategoryIsUsedWhenCategorizationFieldNameIsSet() {
            Set<String> byOverPartitionFields = new TreeSet<>();
            detectors.forEach(d -> byOverPartitionFields.addAll(d.getByOverPartitionTerms()));
            boolean isMlCategoryUsed = byOverPartitionFields.contains(ML_CATEGORY_FIELD);
            if (isMlCategoryUsed && categorizationFieldName == null) {
                throw ExceptionsHelper.badRequestException(CATEGORIZATION_FIELD_NAME.getPreferredName()
                        + " must be set for " + ML_CATEGORY_FIELD + " to be available");
            }
            if (categorizationFieldName != null && isMlCategoryUsed == false) {
                throw ExceptionsHelper.badRequestException(CATEGORIZATION_FIELD_NAME.getPreferredName()
                        + " is set but " + ML_CATEGORY_FIELD + " is not used in any detector by/over/partition field");
            }
        }

        private void verifyCategorizationAnalyzer() {
            if (categorizationAnalyzerConfig == null) {
                return;
            }

            verifyCategorizationFieldNameSetIfAnalyzerIsSet();
        }

        private void verifyCategorizationFieldNameSetIfAnalyzerIsSet() {
            if (categorizationFieldName == null) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(
                        Messages.JOB_CONFIG_CATEGORIZATION_ANALYZER_REQUIRES_CATEGORIZATION_FIELD_NAME));
            }
        }

        private void verifyCategorizationFilters() {
            if (categorizationFilters == null || categorizationFilters.isEmpty()) {
                return;
            }

            verifyCategorizationAnalyzerNotSetIfFiltersAreSet();
            verifyCategorizationFieldNameSetIfFiltersAreSet();
            verifyCategorizationFiltersAreDistinct();
            verifyCategorizationFiltersContainNoneEmpty();
            verifyCategorizationFiltersAreValidRegex();
        }

        private void verifyCategorizationAnalyzerNotSetIfFiltersAreSet() {
            if (categorizationAnalyzerConfig != null) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(
                        Messages.JOB_CONFIG_CATEGORIZATION_FILTERS_INCOMPATIBLE_WITH_CATEGORIZATION_ANALYZER));
            }
        }

        private void verifyCategorizationFieldNameSetIfFiltersAreSet() {
            if (categorizationFieldName == null) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(
                        Messages.JOB_CONFIG_CATEGORIZATION_FILTERS_REQUIRE_CATEGORIZATION_FIELD_NAME));
            }
        }

        private void verifyCategorizationFiltersAreDistinct() {
            if (categorizationFilters.stream().distinct().count() != categorizationFilters.size()) {
                throw ExceptionsHelper.badRequestException(
                        Messages.getMessage(Messages.JOB_CONFIG_CATEGORIZATION_FILTERS_CONTAINS_DUPLICATES));
            }
        }

        private void verifyCategorizationFiltersContainNoneEmpty() {
            if (categorizationFilters.stream().anyMatch(String::isEmpty)) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.JOB_CONFIG_CATEGORIZATION_FILTERS_CONTAINS_EMPTY));
            }
        }

        private void verifyCategorizationFiltersAreValidRegex() {
            for (String filter : categorizationFilters) {
                if (!isValidRegex(filter)) {
                    throw ExceptionsHelper.badRequestException(
                            Messages.getMessage(Messages.JOB_CONFIG_CATEGORIZATION_FILTERS_CONTAINS_INVALID_REGEX, filter));
                }
            }
        }

        private void verifyMultipleBucketSpans() {
            if (multipleBucketSpans == null) {
                return;
            }

            for (TimeValue span : multipleBucketSpans) {
                if ((span.getSeconds() % bucketSpan.getSeconds() != 0L) || (span.compareTo(bucketSpan) <= 0)) {
                    throw ExceptionsHelper.badRequestException(
                            Messages.getMessage(Messages.JOB_CONFIG_MULTIPLE_BUCKETSPANS_MUST_BE_MULTIPLE, span, bucketSpan));
                }
            }
        }

        private static void checkDetectorsHavePartitionFields(List<Detector> detectors) {
            for (Detector detector : detectors) {
                if (!Strings.isNullOrEmpty(detector.getPartitionFieldName())) {
                    return;
                }
            }
            throw ExceptionsHelper.badRequestException(Messages.getMessage(
                    Messages.JOB_CONFIG_PER_PARTITION_NORMALIZATION_REQUIRES_PARTITION_FIELD));
        }

        private static void checkNoInfluencersAreSet(List<String> influencers) {
            if (!influencers.isEmpty()) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(
                        Messages.JOB_CONFIG_PER_PARTITION_NORMALIZATION_CANNOT_USE_INFLUENCERS));
            }
        }

        private static boolean isValidRegex(String exp) {
            try {
                Pattern.compile(exp);
                return true;
            } catch (PatternSyntaxException e) {
                return false;
            }
        }

        private static Boolean verifyOverlappingBucketsConfig(Boolean overlappingBuckets, List<Detector> detectors) {
            // If any detector function is rare/freq_rare, mustn't use overlapping buckets
            boolean mustNotUse = false;

            List<DetectorFunction> illegalFunctions = new ArrayList<>();
            for (Detector d : detectors) {
                if (Detector.NO_OVERLAPPING_BUCKETS_FUNCTIONS.contains(d.getFunction())) {
                    illegalFunctions.add(d.getFunction());
                    mustNotUse = true;
                }
            }

            if (Boolean.TRUE.equals(overlappingBuckets) && mustNotUse) {
                throw ExceptionsHelper.badRequestException(
                        Messages.getMessage(Messages.JOB_CONFIG_OVERLAPPING_BUCKETS_INCOMPATIBLE_FUNCTION, illegalFunctions.toString()));
            }

            return overlappingBuckets;
        }
    }
}
