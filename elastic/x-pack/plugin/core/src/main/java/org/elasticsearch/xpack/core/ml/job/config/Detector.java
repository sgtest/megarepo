/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.job.config;

import org.elasticsearch.ElasticsearchParseException;
import org.elasticsearch.Version;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.xpack.core.ml.MlParserType;
import org.elasticsearch.xpack.core.ml.job.messages.Messages;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.writer.RecordWriter;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.core.ml.utils.ToXContentParams;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.EnumMap;
import java.util.EnumSet;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.stream.Collectors;


/**
 * Defines the fields to be used in the analysis.
 * <code>fieldname</code> must be set and only one of <code>byFieldName</code>
 * and <code>overFieldName</code> should be set.
 */
public class Detector implements ToXContentObject, Writeable {

    public enum ExcludeFrequent implements Writeable {
        ALL,
        NONE,
        BY,
        OVER;

        /**
         * Case-insensitive from string method.
         * Works with either JSON, json, etc.
         *
         * @param value String representation
         * @return The data format
         */
        public static ExcludeFrequent forString(String value) {
            return valueOf(value.toUpperCase(Locale.ROOT));
        }

        public static ExcludeFrequent readFromStream(StreamInput in) throws IOException {
            return in.readEnum(ExcludeFrequent.class);
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            out.writeEnum(this);
        }

        @Override
        public String toString() {
            return name().toLowerCase(Locale.ROOT);
        }
    }

    public static final ParseField DETECTOR_DESCRIPTION_FIELD = new ParseField("detector_description");
    public static final ParseField FUNCTION_FIELD = new ParseField("function");
    public static final ParseField FIELD_NAME_FIELD = new ParseField("field_name");
    public static final ParseField BY_FIELD_NAME_FIELD = new ParseField("by_field_name");
    public static final ParseField OVER_FIELD_NAME_FIELD = new ParseField("over_field_name");
    public static final ParseField PARTITION_FIELD_NAME_FIELD = new ParseField("partition_field_name");
    public static final ParseField USE_NULL_FIELD = new ParseField("use_null");
    public static final ParseField EXCLUDE_FREQUENT_FIELD = new ParseField("exclude_frequent");
    // TODO: Remove the deprecated detector_rules setting in 7.0
    public static final ParseField RULES_FIELD = new ParseField("rules", "detector_rules");
    public static final ParseField DETECTOR_INDEX = new ParseField("detector_index");

    // These parsers follow the pattern that metadata is parsed leniently (to allow for enhancements), whilst config is parsed strictly
    public static final ObjectParser<Builder, Void> METADATA_PARSER = new ObjectParser<>("detector", true, Builder::new);
    public static final ObjectParser<Builder, Void> CONFIG_PARSER = new ObjectParser<>("detector", false, Builder::new);
    public static final Map<MlParserType, ObjectParser<Builder, Void>> PARSERS = new EnumMap<>(MlParserType.class);

    static {
        PARSERS.put(MlParserType.METADATA, METADATA_PARSER);
        PARSERS.put(MlParserType.CONFIG, CONFIG_PARSER);
        for (MlParserType parserType : MlParserType.values()) {
            ObjectParser<Builder, Void> parser = PARSERS.get(parserType);
            assert parser != null;
            parser.declareString(Builder::setDetectorDescription, DETECTOR_DESCRIPTION_FIELD);
            parser.declareString(Builder::setFunction, FUNCTION_FIELD);
            parser.declareString(Builder::setFieldName, FIELD_NAME_FIELD);
            parser.declareString(Builder::setByFieldName, BY_FIELD_NAME_FIELD);
            parser.declareString(Builder::setOverFieldName, OVER_FIELD_NAME_FIELD);
            parser.declareString(Builder::setPartitionFieldName, PARTITION_FIELD_NAME_FIELD);
            parser.declareBoolean(Builder::setUseNull, USE_NULL_FIELD);
            parser.declareField(Builder::setExcludeFrequent, p -> {
                if (p.currentToken() == XContentParser.Token.VALUE_STRING) {
                    return ExcludeFrequent.forString(p.text());
                }
                throw new IllegalArgumentException("Unsupported token [" + p.currentToken() + "]");
            }, EXCLUDE_FREQUENT_FIELD, ObjectParser.ValueType.STRING);
            parser.declareObjectArray(Builder::setRules, (p, c) ->
                    DetectionRule.PARSERS.get(parserType).apply(p, c).build(), RULES_FIELD);
            parser.declareInt(Builder::setDetectorIndex, DETECTOR_INDEX);
        }
    }

    public static final String BY = "by";
    public static final String OVER = "over";

    /**
     * The set of functions that do not require a field, by field or over field
     */
    public static final EnumSet<DetectorFunction> COUNT_WITHOUT_FIELD_FUNCTIONS = EnumSet.of(
            DetectorFunction.COUNT,
            DetectorFunction.HIGH_COUNT,
            DetectorFunction.LOW_COUNT,
            DetectorFunction.NON_ZERO_COUNT,
            DetectorFunction.LOW_NON_ZERO_COUNT,
            DetectorFunction.HIGH_NON_ZERO_COUNT,
            DetectorFunction.TIME_OF_DAY,
            DetectorFunction.TIME_OF_WEEK
    );

    /**
     * The set of functions that require a fieldname
     */
    public static final EnumSet<DetectorFunction> FIELD_NAME_FUNCTIONS = EnumSet.of(
            DetectorFunction.DISTINCT_COUNT,
            DetectorFunction.LOW_DISTINCT_COUNT,
            DetectorFunction.HIGH_DISTINCT_COUNT,
            DetectorFunction.INFO_CONTENT,
            DetectorFunction.LOW_INFO_CONTENT,
            DetectorFunction.HIGH_INFO_CONTENT,
            DetectorFunction.METRIC,
            DetectorFunction.MEAN, DetectorFunction.AVG,
            DetectorFunction.HIGH_MEAN, DetectorFunction.HIGH_AVG,
            DetectorFunction.LOW_MEAN, DetectorFunction.LOW_AVG,
            DetectorFunction.MEDIAN,
            DetectorFunction.LOW_MEDIAN,
            DetectorFunction.HIGH_MEDIAN,
            DetectorFunction.MIN,
            DetectorFunction.MAX,
            DetectorFunction.SUM,
            DetectorFunction.LOW_SUM,
            DetectorFunction.HIGH_SUM,
            DetectorFunction.NON_NULL_SUM,
            DetectorFunction.LOW_NON_NULL_SUM,
            DetectorFunction.HIGH_NON_NULL_SUM,
            DetectorFunction.VARP,
            DetectorFunction.LOW_VARP,
            DetectorFunction.HIGH_VARP,
            DetectorFunction.LAT_LONG
    );

    /**
     * The set of functions that require a by fieldname
     */
    public static final EnumSet<DetectorFunction> BY_FIELD_NAME_FUNCTIONS = EnumSet.of(
            DetectorFunction.RARE,
            DetectorFunction.FREQ_RARE
    );

    /**
     * The set of functions that require a over fieldname
     */
    public static final EnumSet<DetectorFunction> OVER_FIELD_NAME_FUNCTIONS = EnumSet.of(
            DetectorFunction.FREQ_RARE
    );

    /**
     * The set of functions that cannot have an over fieldname
     */
    public static final EnumSet<DetectorFunction> NO_OVER_FIELD_NAME_FUNCTIONS = EnumSet.of(
            DetectorFunction.NON_ZERO_COUNT,
            DetectorFunction.LOW_NON_ZERO_COUNT,
            DetectorFunction.HIGH_NON_ZERO_COUNT
    );

    /**
     * The set of functions that must not be used with overlapping buckets
     */
    public static final EnumSet<DetectorFunction> NO_OVERLAPPING_BUCKETS_FUNCTIONS = EnumSet.of(
            DetectorFunction.RARE,
            DetectorFunction.FREQ_RARE
    );

    /**
     * The set of functions that should not be used with overlapping buckets
     * as they gain no benefit but have overhead
     */
    public static final EnumSet<DetectorFunction> OVERLAPPING_BUCKETS_FUNCTIONS_NOT_NEEDED = EnumSet.of(
            DetectorFunction.MIN,
            DetectorFunction.MAX,
            DetectorFunction.TIME_OF_DAY,
            DetectorFunction.TIME_OF_WEEK
    );

    /**
     * field names cannot contain any of these characters
     * ", \
     */
    public static final Character[] PROHIBITED_FIELDNAME_CHARACTERS = {'"', '\\'};
    public static final String PROHIBITED = String.join(",",
            Arrays.stream(PROHIBITED_FIELDNAME_CHARACTERS).map(
                    c -> Character.toString(c)).collect(Collectors.toList()));


    private final String detectorDescription;
    private final DetectorFunction function;
    private final String fieldName;
    private final String byFieldName;
    private final String overFieldName;
    private final String partitionFieldName;
    private final boolean useNull;
    private final ExcludeFrequent excludeFrequent;
    private final List<DetectionRule> rules;
    private final int detectorIndex;

    public Detector(StreamInput in) throws IOException {
        detectorDescription = in.readString();
        function = DetectorFunction.fromString(in.readString());
        fieldName = in.readOptionalString();
        byFieldName = in.readOptionalString();
        overFieldName = in.readOptionalString();
        partitionFieldName = in.readOptionalString();
        useNull = in.readBoolean();
        excludeFrequent = in.readBoolean() ? ExcludeFrequent.readFromStream(in) : null;
        rules = in.readList(DetectionRule::new);
        if (in.getVersion().onOrAfter(Version.V_5_5_0)) {
            detectorIndex = in.readInt();
        } else {
            // negative means unknown, and is expected for 5.4 jobs
            detectorIndex = -1;
        }
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeString(detectorDescription);
        out.writeString(function.getFullName());
        out.writeOptionalString(fieldName);
        out.writeOptionalString(byFieldName);
        out.writeOptionalString(overFieldName);
        out.writeOptionalString(partitionFieldName);
        out.writeBoolean(useNull);
        if (excludeFrequent != null) {
            out.writeBoolean(true);
            excludeFrequent.writeTo(out);
        } else {
            out.writeBoolean(false);
        }
        out.writeList(rules);
        if (out.getVersion().onOrAfter(Version.V_5_5_0)) {
            out.writeInt(detectorIndex);
        }
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.field(DETECTOR_DESCRIPTION_FIELD.getPreferredName(), detectorDescription);
        builder.field(FUNCTION_FIELD.getPreferredName(), function);
        if (fieldName != null) {
            builder.field(FIELD_NAME_FIELD.getPreferredName(), fieldName);
        }
        if (byFieldName != null) {
            builder.field(BY_FIELD_NAME_FIELD.getPreferredName(), byFieldName);
        }
        if (overFieldName != null) {
            builder.field(OVER_FIELD_NAME_FIELD.getPreferredName(), overFieldName);
        }
        if (partitionFieldName != null) {
            builder.field(PARTITION_FIELD_NAME_FIELD.getPreferredName(), partitionFieldName);
        }
        if (useNull) {
            builder.field(USE_NULL_FIELD.getPreferredName(), useNull);
        }
        if (excludeFrequent != null) {
            builder.field(EXCLUDE_FREQUENT_FIELD.getPreferredName(), excludeFrequent);
        }
        if (rules.isEmpty() == false) {
            builder.field(RULES_FIELD.getPreferredName(), rules);
        }
        // negative means "unknown", which should only happen for a 5.4 job
        if (detectorIndex >= 0
                // no point writing this to cluster state, as the indexes will get reassigned on reload anyway
                && params.paramAsBoolean(ToXContentParams.FOR_CLUSTER_STATE, false) == false) {
            builder.field(DETECTOR_INDEX.getPreferredName(), detectorIndex);
        }
        builder.endObject();
        return builder;
    }

    private Detector(String detectorDescription, DetectorFunction function, String fieldName, String byFieldName, String overFieldName,
                     String partitionFieldName, boolean useNull, ExcludeFrequent excludeFrequent, List<DetectionRule> rules,
                     int detectorIndex) {
        this.function = function;
        this.fieldName = fieldName;
        this.byFieldName = byFieldName;
        this.overFieldName = overFieldName;
        this.partitionFieldName = partitionFieldName;
        this.useNull = useNull;
        this.excludeFrequent = excludeFrequent;
        this.rules = Collections.unmodifiableList(rules);
        this.detectorDescription = detectorDescription != null ? detectorDescription : DefaultDetectorDescription.of(this);
        this.detectorIndex = detectorIndex;
    }

    public String getDetectorDescription() {
        return detectorDescription;
    }

    /**
     * The analysis function used e.g. count, rare, min etc.
     *
     * @return The function or <code>null</code> if not set
     */
    public DetectorFunction getFunction() {
        return function;
    }

    /**
     * The Analysis field
     *
     * @return The field to analyse
     */
    public String getFieldName() {
        return fieldName;
    }

    /**
     * The 'by' field or <code>null</code> if not set.
     *
     * @return The 'by' field
     */
    public String getByFieldName() {
        return byFieldName;
    }

    /**
     * The 'over' field or <code>null</code> if not set.
     *
     * @return The 'over' field
     */
    public String getOverFieldName() {
        return overFieldName;
    }

    /**
     * Segments the analysis along another field to have completely
     * independent baselines for each instance of partitionfield
     *
     * @return The Partition Field
     */
    public String getPartitionFieldName() {
        return partitionFieldName;
    }

    /**
     * Where there isn't a value for the 'by' or 'over' field should a new
     * series be used as the 'null' series.
     *
     * @return true if the 'null' series should be created
     */
    public boolean isUseNull() {
        return useNull;
    }

    /**
     * Excludes frequently-occuring metrics from the analysis;
     * can apply to 'by' field, 'over' field, or both
     *
     * @return the value that the user set
     */
    public ExcludeFrequent getExcludeFrequent() {
        return excludeFrequent;
    }

    public List<DetectionRule> getRules() {
        return rules;
    }

    /**
     * @return the detector index or a negative number if unknown
     */
    public int getDetectorIndex() {
        return detectorIndex;
    }

    /**
     * Returns a list with the byFieldName, overFieldName and partitionFieldName that are not null
     *
     * @return a list with the byFieldName, overFieldName and partitionFieldName that are not null
     */
    public List<String> extractAnalysisFields() {
        List<String> analysisFields = Arrays.asList(getByFieldName(),
                getOverFieldName(), getPartitionFieldName());
        return analysisFields.stream().filter(item -> item != null).collect(Collectors.toList());
    }

    public Set<String> extractReferencedFilters() {
        return rules == null ? Collections.emptySet()
                : rules.stream().map(DetectionRule::extractReferencedFilters)
                .flatMap(Set::stream).collect(Collectors.toSet());
    }

    /**
     * Returns the set of by/over/partition terms
     */
    public Set<String> getByOverPartitionTerms() {
        Set<String> terms = new HashSet<>();
        if (byFieldName != null) {
            terms.add(byFieldName);
        }
        if (overFieldName != null) {
            terms.add(overFieldName);
        }
        if (partitionFieldName != null) {
            terms.add(partitionFieldName);
        }
        return terms;
    }

    @Override
    public boolean equals(Object other) {
        if (this == other) {
            return true;
        }

        if (other instanceof Detector == false) {
            return false;
        }

        Detector that = (Detector) other;

        return Objects.equals(this.detectorDescription, that.detectorDescription) &&
                Objects.equals(this.function, that.function) &&
                Objects.equals(this.fieldName, that.fieldName) &&
                Objects.equals(this.byFieldName, that.byFieldName) &&
                Objects.equals(this.overFieldName, that.overFieldName) &&
                Objects.equals(this.partitionFieldName, that.partitionFieldName) &&
                Objects.equals(this.useNull, that.useNull) &&
                Objects.equals(this.excludeFrequent, that.excludeFrequent) &&
                Objects.equals(this.rules, that.rules) &&
                this.detectorIndex == that.detectorIndex;
    }

    @Override
    public int hashCode() {
        return Objects.hash(detectorDescription, function, fieldName, byFieldName, overFieldName, partitionFieldName, useNull,
                excludeFrequent, rules, detectorIndex);
    }

    public static class Builder {

        /**
         * Functions that do not support rules:
         * <ul>
         * <li>lat_long - because it is a multivariate feature
         * <li>metric - because having the same conditions on min,max,mean is
         * error-prone
         * </ul>
         */
        static final EnumSet<DetectorFunction> FUNCTIONS_WITHOUT_RULE_SUPPORT = EnumSet.of(
                DetectorFunction.LAT_LONG, DetectorFunction.METRIC);

        private String detectorDescription;
        private DetectorFunction function;
        private String fieldName;
        private String byFieldName;
        private String overFieldName;
        private String partitionFieldName;
        private boolean useNull = false;
        private ExcludeFrequent excludeFrequent;
        private List<DetectionRule> rules = Collections.emptyList();
        // negative means unknown, and is expected for v5.4 jobs
        private int detectorIndex = -1;

        public Builder() {
        }

        public Builder(Detector detector) {
            detectorDescription = detector.detectorDescription;
            function = detector.function;
            fieldName = detector.fieldName;
            byFieldName = detector.byFieldName;
            overFieldName = detector.overFieldName;
            partitionFieldName = detector.partitionFieldName;
            useNull = detector.useNull;
            excludeFrequent = detector.excludeFrequent;
            rules = new ArrayList<>(detector.getRules());
            detectorIndex = detector.detectorIndex;
        }

        public Builder(String function, String fieldName) {
            this(DetectorFunction.fromString(function), fieldName);
        }

        public Builder(DetectorFunction function, String fieldName) {
            this.function = function;
            this.fieldName = fieldName;
        }

        public void setDetectorDescription(String detectorDescription) {
            this.detectorDescription = detectorDescription;
        }

        public void setFunction(String function) {
            this.function = DetectorFunction.fromString(function);
        }

        public void setFieldName(String fieldName) {
            this.fieldName = fieldName;
        }

        public void setByFieldName(String byFieldName) {
            this.byFieldName = byFieldName;
        }

        public void setOverFieldName(String overFieldName) {
            this.overFieldName = overFieldName;
        }

        public void setPartitionFieldName(String partitionFieldName) {
            this.partitionFieldName = partitionFieldName;
        }

        public void setUseNull(boolean useNull) {
            this.useNull = useNull;
        }

        public void setExcludeFrequent(ExcludeFrequent excludeFrequent) {
            this.excludeFrequent = excludeFrequent;
        }

        public void setRules(List<DetectionRule> rules) {
            this.rules = rules;
        }

        public void setDetectorIndex(int detectorIndex) {
            this.detectorIndex = detectorIndex;
        }

        public Detector build() {
            boolean emptyField = Strings.isEmpty(fieldName);
            boolean emptyByField = Strings.isEmpty(byFieldName);
            boolean emptyOverField = Strings.isEmpty(overFieldName);
            boolean emptyPartitionField = Strings.isEmpty(partitionFieldName);

            if (emptyField && emptyByField && emptyOverField) {
                if (!Detector.COUNT_WITHOUT_FIELD_FUNCTIONS.contains(function)) {
                    throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.JOB_CONFIG_ANALYSIS_FIELD_MUST_BE_SET));
                }
            }

            // check functions have required fields

            if (emptyField && Detector.FIELD_NAME_FUNCTIONS.contains(function)) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.JOB_CONFIG_FUNCTION_REQUIRES_FIELDNAME, function));
            }

            if (!emptyField && (Detector.FIELD_NAME_FUNCTIONS.contains(function) == false)) {
                throw ExceptionsHelper.badRequestException(
                        Messages.getMessage(Messages.JOB_CONFIG_FIELDNAME_INCOMPATIBLE_FUNCTION, function));
            }

            if (emptyByField && Detector.BY_FIELD_NAME_FUNCTIONS.contains(function)) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.JOB_CONFIG_FUNCTION_REQUIRES_BYFIELD, function));
            }

            if (emptyOverField && Detector.OVER_FIELD_NAME_FUNCTIONS.contains(function)) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.JOB_CONFIG_FUNCTION_REQUIRES_OVERFIELD, function));
            }

            if (!emptyOverField && Detector.NO_OVER_FIELD_NAME_FUNCTIONS.contains(function)) {
                throw ExceptionsHelper.badRequestException(
                        Messages.getMessage(Messages.JOB_CONFIG_OVERFIELD_INCOMPATIBLE_FUNCTION, function));
            }

            // field names cannot contain certain characters
            String[] fields = { fieldName, byFieldName, overFieldName, partitionFieldName };
            for (String field : fields) {
                verifyFieldName(field);
            }

            DetectorFunction function = this.function == null ? DetectorFunction.METRIC : this.function;
            if (rules.isEmpty() == false) {
                if (FUNCTIONS_WITHOUT_RULE_SUPPORT.contains(function)) {
                    String msg = Messages.getMessage(Messages.JOB_CONFIG_DETECTION_RULE_NOT_SUPPORTED_BY_FUNCTION, function);
                    throw ExceptionsHelper.badRequestException(msg);
                }
                for (DetectionRule rule : rules) {
                    checkScoping(rule);
                }
            }

            // partition, by and over field names cannot be duplicates
            if (!emptyPartitionField) {
                if (partitionFieldName.equals(byFieldName)) {
                    throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.JOB_CONFIG_DETECTOR_DUPLICATE_FIELD_NAME,
                            PARTITION_FIELD_NAME_FIELD.getPreferredName(), BY_FIELD_NAME_FIELD.getPreferredName(),
                            partitionFieldName));
                }
                if (partitionFieldName.equals(overFieldName)) {
                    throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.JOB_CONFIG_DETECTOR_DUPLICATE_FIELD_NAME,
                            PARTITION_FIELD_NAME_FIELD.getPreferredName(), OVER_FIELD_NAME_FIELD.getPreferredName(),
                            partitionFieldName));
                }
            }
            if (!emptyByField && byFieldName.equals(overFieldName)) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.JOB_CONFIG_DETECTOR_DUPLICATE_FIELD_NAME,
                        BY_FIELD_NAME_FIELD.getPreferredName(), OVER_FIELD_NAME_FIELD.getPreferredName(),
                        byFieldName));
            }

            // by/over field names cannot be "count", "over', "by" - this requirement dates back to the early
            // days of the ML code and could be removed now BUT ONLY IF THE C++ CODE IS CHANGED
            // FIRST - see https://github.com/elastic/x-pack-elasticsearch/issues/858
            if (DetectorFunction.COUNT.getFullName().equals(byFieldName)) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.JOB_CONFIG_DETECTOR_COUNT_DISALLOWED,
                        BY_FIELD_NAME_FIELD.getPreferredName()));
            }
            if (DetectorFunction.COUNT.getFullName().equals(overFieldName)) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.JOB_CONFIG_DETECTOR_COUNT_DISALLOWED,
                        OVER_FIELD_NAME_FIELD.getPreferredName()));
            }

            if (BY.equals(byFieldName)) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.JOB_CONFIG_DETECTOR_BY_DISALLOWED,
                        BY_FIELD_NAME_FIELD.getPreferredName()));
            }
            if (BY.equals(overFieldName)) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.JOB_CONFIG_DETECTOR_BY_DISALLOWED,
                        OVER_FIELD_NAME_FIELD.getPreferredName()));
            }

            if (OVER.equals(byFieldName)) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.JOB_CONFIG_DETECTOR_OVER_DISALLOWED,
                        BY_FIELD_NAME_FIELD.getPreferredName()));
            }
            if (OVER.equals(overFieldName)) {
                throw ExceptionsHelper.badRequestException(Messages.getMessage(Messages.JOB_CONFIG_DETECTOR_OVER_DISALLOWED,
                        OVER_FIELD_NAME_FIELD.getPreferredName()));
            }

            return new Detector(detectorDescription, function, fieldName, byFieldName, overFieldName, partitionFieldName,
                    useNull, excludeFrequent, rules, detectorIndex);
        }

        public List<String> extractAnalysisFields() {
            List<String> analysisFields = Arrays.asList(byFieldName, overFieldName, partitionFieldName);
            return analysisFields.stream().filter(item -> item != null).collect(Collectors.toList());
        }

        /**
         * Check that the characters used in a field name will not cause problems.
         *
         * @param field The field name to be validated
         */
        public static void verifyFieldName(String field) throws ElasticsearchParseException {
            if (field != null && containsInvalidChar(field)) {
                throw ExceptionsHelper.badRequestException(
                        Messages.getMessage(Messages.JOB_CONFIG_INVALID_FIELDNAME_CHARS, field, Detector.PROHIBITED));
            }
            if (RecordWriter.CONTROL_FIELD_NAME.equals(field)) {
                throw ExceptionsHelper.badRequestException(
                        Messages.getMessage(Messages.JOB_CONFIG_INVALID_FIELDNAME, field, RecordWriter.CONTROL_FIELD_NAME));
            }
        }

        private static boolean containsInvalidChar(String field) {
            for (Character ch : Detector.PROHIBITED_FIELDNAME_CHARACTERS) {
                if (field.indexOf(ch) >= 0) {
                    return true;
                }
            }
            return field.chars().anyMatch(Character::isISOControl);
        }

        private void checkScoping(DetectionRule rule) throws ElasticsearchParseException {
            String targetFieldName = rule.getTargetFieldName();
            checkTargetFieldNameIsValid(extractAnalysisFields(), targetFieldName);
            for (RuleCondition condition : rule.getConditions()) {
                List<String> validOptions = Collections.emptyList();
                switch (condition.getType()) {
                    case CATEGORICAL:
                    case CATEGORICAL_COMPLEMENT:
                        validOptions = extractAnalysisFields();
                        break;
                    case NUMERICAL_ACTUAL:
                    case NUMERICAL_TYPICAL:
                    case NUMERICAL_DIFF_ABS:
                        validOptions = getValidFieldNameOptionsForNumeric(rule);
                        break;
                    case TIME:
                    default:
                        break;
                }
                if (!validOptions.contains(condition.getFieldName())) {
                    String msg = Messages.getMessage(Messages.JOB_CONFIG_DETECTION_RULE_CONDITION_INVALID_FIELD_NAME, validOptions,
                            condition.getFieldName());
                    throw ExceptionsHelper.badRequestException(msg);
                }
            }
        }

        private void checkTargetFieldNameIsValid(List<String> analysisFields, String targetFieldName)
                throws ElasticsearchParseException {
            if (targetFieldName != null && !analysisFields.contains(targetFieldName)) {
                String msg =
                        Messages.getMessage(Messages.JOB_CONFIG_DETECTION_RULE_INVALID_TARGET_FIELD_NAME, analysisFields, targetFieldName);
                throw ExceptionsHelper.badRequestException(msg);
            }
        }

        private List<String> getValidFieldNameOptionsForNumeric(DetectionRule rule) {
            List<String> result = new ArrayList<>();
            if (overFieldName != null) {
                result.add(byFieldName == null ? overFieldName : byFieldName);
            } else if (byFieldName != null) {
                result.add(byFieldName);
            }

            if (rule.getTargetFieldName() != null) {
                ScopingLevel targetLevel = ScopingLevel.from(this, rule.getTargetFieldName());
                result = result.stream().filter(field -> targetLevel.isHigherThan(ScopingLevel.from(this, field)))
                        .collect(Collectors.toList());
            }

            if (isEmptyFieldNameAllowed(rule)) {
                result.add(null);
            }
            return result;
        }

        private boolean isEmptyFieldNameAllowed(DetectionRule rule) {
            List<String> analysisFields = extractAnalysisFields();
            return analysisFields.isEmpty() || (rule.getTargetFieldName() != null && analysisFields.size() == 1);
        }

        enum ScopingLevel {
            PARTITION(3),
            OVER(2),
            BY(1);

            int level;

            ScopingLevel(int level) {
                this.level = level;
            }

            boolean isHigherThan(ScopingLevel other) {
                return level > other.level;
            }

            static ScopingLevel from(Detector.Builder detector, String fieldName) {
                if (fieldName.equals(detector.partitionFieldName)) {
                    return ScopingLevel.PARTITION;
                }
                if (fieldName.equals(detector.overFieldName)) {
                    return ScopingLevel.OVER;
                }
                if (fieldName.equals(detector.byFieldName)) {
                    return ScopingLevel.BY;
                }
                throw ExceptionsHelper.badRequestException(
                        "fieldName '" + fieldName + "' does not match an analysis field");
            }
        }

    }
}
