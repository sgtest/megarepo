/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.logstructurefinder;

import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;
import java.util.SortedMap;
import java.util.TreeMap;

/**
 * Stores the log file format determined by a {@link LogStructureFinder}.
 */
public class LogStructure implements ToXContentObject {

    public enum Format {

        JSON, XML, DELIMITED, SEMI_STRUCTURED_TEXT;

        public boolean supportsNesting() {
            switch (this) {
                case JSON:
                case XML:
                    return true;
                case DELIMITED:
                case SEMI_STRUCTURED_TEXT:
                    return false;
                default:
                    throw new IllegalStateException("enum value [" + this + "] missing from switch.");
            }
        }

        public boolean isStructured() {
            switch (this) {
                case JSON:
                case XML:
                case DELIMITED:
                    return true;
                case SEMI_STRUCTURED_TEXT:
                    return false;
                default:
                    throw new IllegalStateException("enum value [" + this + "] missing from switch.");
            }
        }

        public boolean isSemiStructured() {
            switch (this) {
                case JSON:
                case XML:
                case DELIMITED:
                    return false;
                case SEMI_STRUCTURED_TEXT:
                    return true;
                default:
                    throw new IllegalStateException("enum value [" + this + "] missing from switch.");
            }
        }

        public static Format fromString(String name) {
            return valueOf(name.trim().toUpperCase(Locale.ROOT));
        }

        @Override
        public String toString() {
            return name().toLowerCase(Locale.ROOT);
        }
    }

    static final ParseField NUM_LINES_ANALYZED = new ParseField("num_lines_analyzed");
    static final ParseField NUM_MESSAGES_ANALYZED = new ParseField("num_messages_analyzed");
    static final ParseField SAMPLE_START = new ParseField("sample_start");
    static final ParseField CHARSET = new ParseField("charset");
    static final ParseField HAS_BYTE_ORDER_MARKER = new ParseField("has_byte_order_marker");
    static final ParseField STRUCTURE = new ParseField("format");
    static final ParseField MULTILINE_START_PATTERN = new ParseField("multiline_start_pattern");
    static final ParseField EXCLUDE_LINES_PATTERN = new ParseField("exclude_lines_pattern");
    static final ParseField INPUT_FIELDS = new ParseField("input_fields");
    static final ParseField HAS_HEADER_ROW = new ParseField("has_header_row");
    static final ParseField DELIMITER = new ParseField("delimiter");
    static final ParseField SHOULD_TRIM_FIELDS = new ParseField("should_trim_fields");
    static final ParseField GROK_PATTERN = new ParseField("grok_pattern");
    static final ParseField TIMESTAMP_FIELD = new ParseField("timestamp_field");
    static final ParseField TIMESTAMP_FORMATS = new ParseField("timestamp_formats");
    static final ParseField NEED_CLIENT_TIMEZONE = new ParseField("need_client_timezone");
    static final ParseField MAPPINGS = new ParseField("mappings");
    static final ParseField EXPLANATION = new ParseField("explanation");

    public static final ObjectParser<Builder, Void> PARSER = new ObjectParser<>("log_file_structure", false, Builder::new);

    static {
        PARSER.declareInt(Builder::setNumLinesAnalyzed, NUM_LINES_ANALYZED);
        PARSER.declareInt(Builder::setNumMessagesAnalyzed, NUM_MESSAGES_ANALYZED);
        PARSER.declareString(Builder::setSampleStart, SAMPLE_START);
        PARSER.declareString(Builder::setCharset, CHARSET);
        PARSER.declareBoolean(Builder::setHasByteOrderMarker, HAS_BYTE_ORDER_MARKER);
        PARSER.declareString((p, c) -> p.setFormat(Format.fromString(c)), STRUCTURE);
        PARSER.declareString(Builder::setMultilineStartPattern, MULTILINE_START_PATTERN);
        PARSER.declareString(Builder::setExcludeLinesPattern, EXCLUDE_LINES_PATTERN);
        PARSER.declareStringArray(Builder::setInputFields, INPUT_FIELDS);
        PARSER.declareBoolean(Builder::setHasHeaderRow, HAS_HEADER_ROW);
        PARSER.declareString((p, c) -> p.setDelimiter(c.charAt(0)), DELIMITER);
        PARSER.declareBoolean(Builder::setShouldTrimFields, SHOULD_TRIM_FIELDS);
        PARSER.declareString(Builder::setGrokPattern, GROK_PATTERN);
        PARSER.declareString(Builder::setTimestampField, TIMESTAMP_FIELD);
        PARSER.declareStringArray(Builder::setTimestampFormats, TIMESTAMP_FORMATS);
        PARSER.declareBoolean(Builder::setNeedClientTimezone, NEED_CLIENT_TIMEZONE);
        PARSER.declareObject(Builder::setMappings, (p, c) -> new TreeMap<>(p.map()), MAPPINGS);
        PARSER.declareStringArray(Builder::setExplanation, EXPLANATION);
    }

    private final int numLinesAnalyzed;
    private final int numMessagesAnalyzed;
    private final String sampleStart;
    private final String charset;
    private final Boolean hasByteOrderMarker;
    private final Format format;
    private final String multilineStartPattern;
    private final String excludeLinesPattern;
    private final List<String> inputFields;
    private final Boolean hasHeaderRow;
    private final Character delimiter;
    private final Boolean shouldTrimFields;
    private final String grokPattern;
    private final List<String> timestampFormats;
    private final String timestampField;
    private final boolean needClientTimezone;
    private final SortedMap<String, Object> mappings;
    private final List<String> explanation;

    public LogStructure(int numLinesAnalyzed, int numMessagesAnalyzed, String sampleStart, String charset, Boolean hasByteOrderMarker,
                        Format format, String multilineStartPattern, String excludeLinesPattern, List<String> inputFields,
                        Boolean hasHeaderRow, Character delimiter, Boolean shouldTrimFields, String grokPattern, String timestampField,
                        List<String> timestampFormats, boolean needClientTimezone, Map<String, Object> mappings,
                        List<String> explanation) {

        this.numLinesAnalyzed = numLinesAnalyzed;
        this.numMessagesAnalyzed = numMessagesAnalyzed;
        this.sampleStart = Objects.requireNonNull(sampleStart);
        this.charset = Objects.requireNonNull(charset);
        this.hasByteOrderMarker = hasByteOrderMarker;
        this.format = Objects.requireNonNull(format);
        this.multilineStartPattern = multilineStartPattern;
        this.excludeLinesPattern = excludeLinesPattern;
        this.inputFields = (inputFields == null) ? null : Collections.unmodifiableList(new ArrayList<>(inputFields));
        this.hasHeaderRow = hasHeaderRow;
        this.delimiter = delimiter;
        this.shouldTrimFields = shouldTrimFields;
        this.grokPattern = grokPattern;
        this.timestampField = timestampField;
        this.timestampFormats = (timestampFormats == null) ? null : Collections.unmodifiableList(new ArrayList<>(timestampFormats));
        this.needClientTimezone = needClientTimezone;
        this.mappings = Collections.unmodifiableSortedMap(new TreeMap<>(mappings));
        this.explanation = Collections.unmodifiableList(new ArrayList<>(explanation));
    }

    public int getNumLinesAnalyzed() {
        return numLinesAnalyzed;
    }

    public int getNumMessagesAnalyzed() {
        return numMessagesAnalyzed;
    }

    public String getSampleStart() {
        return sampleStart;
    }

    public String getCharset() {
        return charset;
    }

    public Boolean getHasByteOrderMarker() {
        return hasByteOrderMarker;
    }

    public Format getFormat() {
        return format;
    }

    public String getMultilineStartPattern() {
        return multilineStartPattern;
    }

    public String getExcludeLinesPattern() {
        return excludeLinesPattern;
    }

    public List<String> getInputFields() {
        return inputFields;
    }

    public Boolean getHasHeaderRow() {
        return hasHeaderRow;
    }

    public Character getDelimiter() {
        return delimiter;
    }

    public Boolean getShouldTrimFields() {
        return shouldTrimFields;
    }

    public String getGrokPattern() {
        return grokPattern;
    }

    public String getTimestampField() {
        return timestampField;
    }

    public List<String> getTimestampFormats() {
        return timestampFormats;
    }

    public boolean needClientTimezone() {
        return needClientTimezone;
    }

    public SortedMap<String, Object> getMappings() {
        return mappings;
    }

    public List<String> getExplanation() {
        return explanation;
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {

        builder.startObject();
        builder.field(NUM_LINES_ANALYZED.getPreferredName(), numLinesAnalyzed);
        builder.field(NUM_MESSAGES_ANALYZED.getPreferredName(), numMessagesAnalyzed);
        builder.field(SAMPLE_START.getPreferredName(), sampleStart);
        builder.field(CHARSET.getPreferredName(), charset);
        if (hasByteOrderMarker != null) {
            builder.field(HAS_BYTE_ORDER_MARKER.getPreferredName(), hasByteOrderMarker.booleanValue());
        }
        builder.field(STRUCTURE.getPreferredName(), format);
        if (multilineStartPattern != null && multilineStartPattern.isEmpty() == false) {
            builder.field(MULTILINE_START_PATTERN.getPreferredName(), multilineStartPattern);
        }
        if (excludeLinesPattern != null && excludeLinesPattern.isEmpty() == false) {
            builder.field(EXCLUDE_LINES_PATTERN.getPreferredName(), excludeLinesPattern);
        }
        if (inputFields != null && inputFields.isEmpty() == false) {
            builder.field(INPUT_FIELDS.getPreferredName(), inputFields);
        }
        if (hasHeaderRow != null) {
            builder.field(HAS_HEADER_ROW.getPreferredName(), hasHeaderRow.booleanValue());
        }
        if (delimiter != null) {
            builder.field(DELIMITER.getPreferredName(), String.valueOf(delimiter));
        }
        if (shouldTrimFields != null) {
            builder.field(SHOULD_TRIM_FIELDS.getPreferredName(), shouldTrimFields.booleanValue());
        }
        if (grokPattern != null && grokPattern.isEmpty() == false) {
            builder.field(GROK_PATTERN.getPreferredName(), grokPattern);
        }
        if (timestampField != null && timestampField.isEmpty() == false) {
            builder.field(TIMESTAMP_FIELD.getPreferredName(), timestampField);
        }
        if (timestampFormats != null && timestampFormats.isEmpty() == false) {
            builder.field(TIMESTAMP_FORMATS.getPreferredName(), timestampFormats);
        }
        builder.field(NEED_CLIENT_TIMEZONE.getPreferredName(), needClientTimezone);
        builder.field(MAPPINGS.getPreferredName(), mappings);
        builder.field(EXPLANATION.getPreferredName(), explanation);
        builder.endObject();

        return builder;
    }

    @Override
    public int hashCode() {

        return Objects.hash(numLinesAnalyzed, numMessagesAnalyzed, sampleStart, charset, hasByteOrderMarker, format,
            multilineStartPattern, excludeLinesPattern, inputFields, hasHeaderRow, delimiter, shouldTrimFields, grokPattern, timestampField,
            timestampFormats, needClientTimezone, mappings, explanation);
    }

    @Override
    public boolean equals(Object other) {

        if (this == other) {
            return true;
        }

        if (other == null || getClass() != other.getClass()) {
            return false;
        }

        LogStructure that = (LogStructure) other;
        return this.numLinesAnalyzed == that.numLinesAnalyzed &&
            this.numMessagesAnalyzed == that.numMessagesAnalyzed &&
            this.needClientTimezone == that.needClientTimezone &&
            Objects.equals(this.sampleStart, that.sampleStart) &&
            Objects.equals(this.charset, that.charset) &&
            Objects.equals(this.hasByteOrderMarker, that.hasByteOrderMarker) &&
            Objects.equals(this.format, that.format) &&
            Objects.equals(this.multilineStartPattern, that.multilineStartPattern) &&
            Objects.equals(this.excludeLinesPattern, that.excludeLinesPattern) &&
            Objects.equals(this.inputFields, that.inputFields) &&
            Objects.equals(this.hasHeaderRow, that.hasHeaderRow) &&
            Objects.equals(this.delimiter, that.delimiter) &&
            Objects.equals(this.shouldTrimFields, that.shouldTrimFields) &&
            Objects.equals(this.grokPattern, that.grokPattern) &&
            Objects.equals(this.timestampField, that.timestampField) &&
            Objects.equals(this.timestampFormats, that.timestampFormats) &&
            Objects.equals(this.mappings, that.mappings) &&
            Objects.equals(this.explanation, that.explanation);
    }

    public static class Builder {

        private int numLinesAnalyzed;
        private int numMessagesAnalyzed;
        private String sampleStart;
        private String charset;
        private Boolean hasByteOrderMarker;
        private Format format;
        private String multilineStartPattern;
        private String excludeLinesPattern;
        private List<String> inputFields;
        private Boolean hasHeaderRow;
        private Character delimiter;
        private Boolean shouldTrimFields;
        private String grokPattern;
        private String timestampField;
        private List<String> timestampFormats;
        private boolean needClientTimezone;
        private Map<String, Object> mappings;
        private List<String> explanation;

        public Builder() {
            this(Format.SEMI_STRUCTURED_TEXT);
        }

        public Builder(Format format) {
            setFormat(format);
        }

        public Builder setNumLinesAnalyzed(int numLinesAnalyzed) {
            this.numLinesAnalyzed = numLinesAnalyzed;
            return this;
        }

        public Builder setNumMessagesAnalyzed(int numMessagesAnalyzed) {
            this.numMessagesAnalyzed = numMessagesAnalyzed;
            return this;
        }

        public Builder setSampleStart(String sampleStart) {
            this.sampleStart = Objects.requireNonNull(sampleStart);
            return this;
        }

        public Builder setCharset(String charset) {
            this.charset = Objects.requireNonNull(charset);
            return this;
        }

        public Builder setHasByteOrderMarker(Boolean hasByteOrderMarker) {
            this.hasByteOrderMarker = hasByteOrderMarker;
            return this;
        }

        public Builder setFormat(Format format) {
            this.format = Objects.requireNonNull(format);
            return this;
        }

        public Builder setMultilineStartPattern(String multilineStartPattern) {
            this.multilineStartPattern = multilineStartPattern;
            return this;
        }

        public Builder setExcludeLinesPattern(String excludeLinesPattern) {
            this.excludeLinesPattern = excludeLinesPattern;
            return this;
        }

        public Builder setInputFields(List<String> inputFields) {
            this.inputFields = inputFields;
            return this;
        }

        public Builder setHasHeaderRow(Boolean hasHeaderRow) {
            this.hasHeaderRow = hasHeaderRow;
            return this;
        }

        public Builder setDelimiter(Character delimiter) {
            this.delimiter = delimiter;
            return this;
        }

        public Builder setShouldTrimFields(Boolean shouldTrimFields) {
            this.shouldTrimFields = shouldTrimFields;
            return this;
        }

        public Builder setGrokPattern(String grokPattern) {
            this.grokPattern = grokPattern;
            return this;
        }

        public Builder setTimestampField(String timestampField) {
            this.timestampField = timestampField;
            return this;
        }

        public Builder setTimestampFormats(List<String> timestampFormats) {
            this.timestampFormats = timestampFormats;
            return this;
        }

        public Builder setNeedClientTimezone(boolean needClientTimezone) {
            this.needClientTimezone = needClientTimezone;
            return this;
        }

        public Builder setMappings(Map<String, Object> mappings) {
            this.mappings = Objects.requireNonNull(mappings);
            return this;
        }

        public Builder setExplanation(List<String> explanation) {
            this.explanation = Objects.requireNonNull(explanation);
            return this;
        }

        @SuppressWarnings("fallthrough")
        public LogStructure build() {

            if (numLinesAnalyzed <= 0) {
                throw new IllegalArgumentException("Number of lines analyzed must be positive.");
            }

            if (numMessagesAnalyzed <= 0) {
                throw new IllegalArgumentException("Number of messages analyzed must be positive.");
            }

            if (numMessagesAnalyzed > numLinesAnalyzed) {
                throw new IllegalArgumentException("Number of messages analyzed cannot be greater than number of lines analyzed.");
            }

            if (sampleStart == null || sampleStart.isEmpty()) {
                throw new IllegalArgumentException("Sample start must be specified.");
            }

            if (charset == null || charset.isEmpty()) {
                throw new IllegalArgumentException("A character set must be specified.");
            }

            if (charset.toUpperCase(Locale.ROOT).startsWith("UTF") == false && hasByteOrderMarker != null) {
                throw new IllegalArgumentException("A byte order marker is only possible for UTF character sets.");
            }

            switch (format) {
                case JSON:
                    if (shouldTrimFields != null) {
                        throw new IllegalArgumentException("Should trim fields may not be specified for [" + format + "] structures.");
                    }
                    // $FALL-THROUGH$
                case XML:
                    if (hasHeaderRow != null) {
                        throw new IllegalArgumentException("Has header row may not be specified for [" + format + "] structures.");
                    }
                    if (delimiter != null) {
                        throw new IllegalArgumentException("Delimiter may not be specified for [" + format + "] structures.");
                    }
                    if (grokPattern != null) {
                        throw new IllegalArgumentException("Grok pattern may not be specified for [" + format + "] structures.");
                    }
                    break;
                case DELIMITED:
                    if (inputFields == null || inputFields.isEmpty()) {
                        throw new IllegalArgumentException("Input fields must be specified for [" + format + "] structures.");
                    }
                    if (hasHeaderRow == null) {
                        throw new IllegalArgumentException("Has header row must be specified for [" + format + "] structures.");
                    }
                    if (delimiter == null) {
                        throw new IllegalArgumentException("Delimiter must be specified for [" + format + "] structures.");
                    }
                    if (grokPattern != null) {
                        throw new IllegalArgumentException("Grok pattern may not be specified for [" + format + "] structures.");
                    }
                    break;
                case SEMI_STRUCTURED_TEXT:
                    if (inputFields != null) {
                        throw new IllegalArgumentException("Input fields may not be specified for [" + format + "] structures.");
                    }
                    if (hasHeaderRow != null) {
                        throw new IllegalArgumentException("Has header row may not be specified for [" + format + "] structures.");
                    }
                    if (delimiter != null) {
                        throw new IllegalArgumentException("Delimiter may not be specified for [" + format + "] structures.");
                    }
                    if (shouldTrimFields != null) {
                        throw new IllegalArgumentException("Should trim fields may not be specified for [" + format + "] structures.");
                    }
                    if (grokPattern == null || grokPattern.isEmpty()) {
                        throw new IllegalArgumentException("Grok pattern must be specified for [" + format + "] structures.");
                    }
                    break;
                default:
                    throw new IllegalStateException("enum value [" + format + "] missing from switch.");
            }

            if ((timestampField == null) != (timestampFormats == null || timestampFormats.isEmpty())) {
                throw new IllegalArgumentException("Timestamp field and timestamp formats must both be specified or neither be specified.");
            }

            if (needClientTimezone && timestampField == null) {
                throw new IllegalArgumentException("Client timezone cannot be needed if there is no timestamp field.");
            }

            if (mappings == null || mappings.isEmpty()) {
                throw new IllegalArgumentException("Mappings must be specified.");
            }

            if (explanation == null || explanation.isEmpty()) {
                throw new IllegalArgumentException("Explanation must be specified.");
            }

            return new LogStructure(numLinesAnalyzed, numMessagesAnalyzed, sampleStart, charset, hasByteOrderMarker, format,
                multilineStartPattern, excludeLinesPattern, inputFields, hasHeaderRow, delimiter, shouldTrimFields, grokPattern,
                timestampField, timestampFormats, needClientTimezone, mappings, explanation);
        }
    }
}
