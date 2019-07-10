/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.core.dataframe.transforms;

import org.elasticsearch.Version;
import org.elasticsearch.cluster.AbstractDiffable;
import org.elasticsearch.common.Nullable;
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
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentParserUtils;
import org.elasticsearch.xpack.core.common.time.TimeUtils;
import org.elasticsearch.xpack.core.dataframe.DataFrameField;
import org.elasticsearch.xpack.core.dataframe.DataFrameMessages;
import org.elasticsearch.xpack.core.dataframe.transforms.pivot.PivotConfig;
import org.elasticsearch.xpack.core.dataframe.utils.ExceptionsHelper;

import java.io.IOException;
import java.time.Instant;
import java.util.Collections;
import java.util.Map;
import java.util.Objects;

import static org.elasticsearch.common.xcontent.ConstructingObjectParser.constructorArg;
import static org.elasticsearch.common.xcontent.ConstructingObjectParser.optionalConstructorArg;

/**
 * This class holds the configuration details of a data frame transform
 */
public class DataFrameTransformConfig extends AbstractDiffable<DataFrameTransformConfig> implements Writeable, ToXContentObject {

    public static final String NAME = "data_frame_transform_config";
    public static final ParseField HEADERS = new ParseField("headers");

    // types of transforms
    public static final ParseField PIVOT_TRANSFORM = new ParseField("pivot");

    public static final ParseField DESCRIPTION = new ParseField("description");
    public static final ParseField VERSION = new ParseField("version");
    public static final ParseField CREATE_TIME = new ParseField("create_time");
    private static final ConstructingObjectParser<DataFrameTransformConfig, String> STRICT_PARSER = createParser(false);
    private static final ConstructingObjectParser<DataFrameTransformConfig, String> LENIENT_PARSER = createParser(true);
    private static final int MAX_DESCRIPTION_LENGTH = 1_000;

    private final String id;
    private final SourceConfig source;
    private final DestConfig dest;
    private final TimeValue frequency;
    private final SyncConfig syncConfig;
    private final String description;
    // headers store the user context from the creating user, which allows us to run the transform as this user
    // the header only contains name, groups and other context but no authorization keys
    private Map<String, String> headers;
    private Version transformVersion;
    private Instant createTime;

    private final PivotConfig pivotConfig;

    private static void validateStrictParsingParams(Object arg, String parameterName) {
        if (arg != null) {
            throw new IllegalArgumentException("Found [" + parameterName + "], not allowed for strict parsing");
        }
    }

    private static ConstructingObjectParser<DataFrameTransformConfig, String> createParser(boolean lenient) {
        ConstructingObjectParser<DataFrameTransformConfig, String> parser = new ConstructingObjectParser<>(NAME, lenient,
                (args, optionalId) -> {
                    String id = (String) args[0];

                    // if the id has been specified in the body and the path, they must match
                    if (id == null) {
                        id = optionalId;
                    } else if (optionalId != null && id.equals(optionalId) == false) {
                        throw new IllegalArgumentException(
                                DataFrameMessages.getMessage(DataFrameMessages.REST_PUT_DATA_FRAME_INCONSISTENT_ID, id, optionalId));
                    }

                    SourceConfig source = (SourceConfig) args[1];
                    DestConfig dest = (DestConfig) args[2];

                    TimeValue frequency =
                        args[3] == null ? null : TimeValue.parseTimeValue((String) args[3], DataFrameField.FREQUENCY.getPreferredName());

                    SyncConfig syncConfig = (SyncConfig) args[4];
                    // ignored, only for internal storage: String docType = (String) args[5];

                    // on strict parsing do not allow injection of headers, transform version, or create time
                    if (lenient == false) {
                        validateStrictParsingParams(args[6], HEADERS.getPreferredName());
                        validateStrictParsingParams(args[9], CREATE_TIME.getPreferredName());
                        validateStrictParsingParams(args[10], VERSION.getPreferredName());
                    }

                    @SuppressWarnings("unchecked")
                    Map<String, String> headers = (Map<String, String>) args[6];

                    PivotConfig pivotConfig = (PivotConfig) args[7];
                    String description = (String)args[8];
                    return new DataFrameTransformConfig(id,
                        source,
                        dest,
                        frequency,
                        syncConfig,
                        headers,
                        pivotConfig,
                        description,
                        (Instant)args[9],
                        (String)args[10]);
                });

        parser.declareString(optionalConstructorArg(), DataFrameField.ID);
        parser.declareObject(constructorArg(), (p, c) -> SourceConfig.fromXContent(p, lenient), DataFrameField.SOURCE);
        parser.declareObject(constructorArg(), (p, c) -> DestConfig.fromXContent(p, lenient), DataFrameField.DESTINATION);
        parser.declareString(optionalConstructorArg(), DataFrameField.FREQUENCY);

        parser.declareObject(optionalConstructorArg(), (p, c) -> parseSyncConfig(p, lenient), DataFrameField.SYNC);

        parser.declareString(optionalConstructorArg(), DataFrameField.INDEX_DOC_TYPE);

        parser.declareObject(optionalConstructorArg(), (p, c) -> p.mapStrings(), HEADERS);
        parser.declareObject(optionalConstructorArg(), (p, c) -> PivotConfig.fromXContent(p, lenient), PIVOT_TRANSFORM);
        parser.declareString(optionalConstructorArg(), DESCRIPTION);
        parser.declareField(optionalConstructorArg(),
            p -> TimeUtils.parseTimeFieldToInstant(p, CREATE_TIME.getPreferredName()), CREATE_TIME, ObjectParser.ValueType.VALUE);
        parser.declareString(optionalConstructorArg(), VERSION);
        return parser;
    }

    private static SyncConfig parseSyncConfig(XContentParser parser, boolean ignoreUnknownFields) throws IOException {
        XContentParserUtils.ensureExpectedToken(XContentParser.Token.START_OBJECT, parser.currentToken(), parser::getTokenLocation);
        XContentParserUtils.ensureExpectedToken(XContentParser.Token.FIELD_NAME, parser.nextToken(), parser::getTokenLocation);
        SyncConfig syncConfig = parser.namedObject(SyncConfig.class, parser.currentName(), ignoreUnknownFields);
        XContentParserUtils.ensureExpectedToken(XContentParser.Token.END_OBJECT, parser.nextToken(), parser::getTokenLocation);
        return syncConfig;
    }

    public static String documentId(String transformId) {
        return NAME + "-" + transformId;
    }

    DataFrameTransformConfig(final String id,
                             final SourceConfig source,
                             final DestConfig dest,
                             final TimeValue frequency,
                             final SyncConfig syncConfig,
                             final Map<String, String> headers,
                             final PivotConfig pivotConfig,
                             final String description,
                             final Instant createTime,
                             final String version){
        this.id = ExceptionsHelper.requireNonNull(id, DataFrameField.ID.getPreferredName());
        this.source = ExceptionsHelper.requireNonNull(source, DataFrameField.SOURCE.getPreferredName());
        this.dest = ExceptionsHelper.requireNonNull(dest, DataFrameField.DESTINATION.getPreferredName());
        this.frequency = frequency;
        this.syncConfig = syncConfig;
        this.setHeaders(headers == null ? Collections.emptyMap() : headers);
        this.pivotConfig = pivotConfig;
        this.description = description;

        // at least one function must be defined
        if (this.pivotConfig == null) {
            throw new IllegalArgumentException(DataFrameMessages.DATA_FRAME_TRANSFORM_CONFIGURATION_NO_TRANSFORM);
        }
        if (this.description != null && this.description.length() > MAX_DESCRIPTION_LENGTH) {
            throw new IllegalArgumentException("[description] must be less than 1000 characters in length.");
        }
        this.createTime = createTime == null ? null : Instant.ofEpochMilli(createTime.toEpochMilli());
        this.transformVersion = version == null ? null : Version.fromString(version);
    }

    public DataFrameTransformConfig(final String id,
                                    final SourceConfig source,
                                    final DestConfig dest,
                                    final TimeValue frequency,
                                    final SyncConfig syncConfig,
                                    final Map<String, String> headers,
                                    final PivotConfig pivotConfig,
                                    final String description) {
        this(id, source, dest, frequency, syncConfig, headers, pivotConfig, description, null, null);
    }

    public DataFrameTransformConfig(final StreamInput in) throws IOException {
        id = in.readString();
        source = new SourceConfig(in);
        dest = new DestConfig(in);
        if (in.getVersion().onOrAfter(Version.V_7_3_0)) {
            frequency = in.readOptionalTimeValue();
        } else {
            frequency = null;
        }
        setHeaders(in.readMap(StreamInput::readString, StreamInput::readString));
        pivotConfig = in.readOptionalWriteable(PivotConfig::new);
        description = in.readOptionalString();
        if (in.getVersion().onOrAfter(Version.V_7_3_0)) {
            syncConfig = in.readOptionalNamedWriteable(SyncConfig.class);
            createTime = in.readOptionalInstant();
            transformVersion = in.readBoolean() ? Version.readVersion(in) : null;
        } else {
            syncConfig = null;
            createTime = null;
            transformVersion = null;
        }
    }

    public String getId() {
        return id;
    }

    public SourceConfig getSource() {
        return source;
    }

    public DestConfig getDestination() {
        return dest;
    }

    public TimeValue getFrequency() {
        return frequency;
    }

    public SyncConfig getSyncConfig() {
        return syncConfig;
    }

    public Map<String, String> getHeaders() {
        return headers;
    }

    public DataFrameTransformConfig setHeaders(Map<String, String> headers) {
        this.headers = headers;
        return this;
    }

    public Version getVersion() {
        return transformVersion;
    }

    public DataFrameTransformConfig setVersion(Version transformVersion) {
        this.transformVersion = transformVersion;
        return this;
    }

    public Instant getCreateTime() {
        return createTime;
    }

    public DataFrameTransformConfig setCreateTime(Instant createTime) {
        ExceptionsHelper.requireNonNull(createTime, CREATE_TIME.getPreferredName());
        this.createTime = Instant.ofEpochMilli(createTime.toEpochMilli());
        return this;
    }

    public PivotConfig getPivotConfig() {
        return pivotConfig;
    }

    @Nullable
    public String getDescription() {
        return description;
    }

    public boolean isValid() {
        if (pivotConfig != null && pivotConfig.isValid() == false) {
            return false;
        }

        if (syncConfig != null && syncConfig.isValid() == false) {
            return false;
        }

        return source.isValid() && dest.isValid();
    }

    @Override
    public void writeTo(final StreamOutput out) throws IOException {
        out.writeString(id);
        source.writeTo(out);
        dest.writeTo(out);
        if (out.getVersion().onOrAfter(Version.V_7_3_0)) {
            out.writeOptionalTimeValue(frequency);
        }
        out.writeMap(headers, StreamOutput::writeString, StreamOutput::writeString);
        out.writeOptionalWriteable(pivotConfig);
        out.writeOptionalString(description);
        if (out.getVersion().onOrAfter(Version.V_7_3_0)) {
            out.writeOptionalNamedWriteable(syncConfig);
            out.writeOptionalInstant(createTime);
            if (transformVersion != null) {
                out.writeBoolean(true);
                Version.writeVersion(transformVersion, out);
            } else {
                out.writeBoolean(false);
            }
        }
    }

    @Override
    public XContentBuilder toXContent(final XContentBuilder builder, final Params params) throws IOException {
        builder.startObject();
        builder.field(DataFrameField.ID.getPreferredName(), id);
        builder.field(DataFrameField.SOURCE.getPreferredName(), source);
        builder.field(DataFrameField.DESTINATION.getPreferredName(), dest);
        if (frequency != null) {
            builder.field(DataFrameField.FREQUENCY.getPreferredName(), frequency.getStringRep());
        }
        if (syncConfig != null) {
            builder.startObject(DataFrameField.SYNC.getPreferredName());
            builder.field(syncConfig.getWriteableName(), syncConfig);
            builder.endObject();
        }
        if (pivotConfig != null) {
            builder.field(PIVOT_TRANSFORM.getPreferredName(), pivotConfig);
        }
        if (params.paramAsBoolean(DataFrameField.FOR_INTERNAL_STORAGE, false)) {
            builder.field(DataFrameField.INDEX_DOC_TYPE.getPreferredName(), NAME);
        }
        if (headers.isEmpty() == false && params.paramAsBoolean(DataFrameField.FOR_INTERNAL_STORAGE, false) == true) {
            builder.field(HEADERS.getPreferredName(), headers);
        }
        if (description != null) {
            builder.field(DESCRIPTION.getPreferredName(), description);
        }
        if (transformVersion != null) {
            builder.field(VERSION.getPreferredName(), transformVersion);
        }
        if (createTime != null) {
            builder.timeField(CREATE_TIME.getPreferredName(), CREATE_TIME.getPreferredName() + "_string", createTime.toEpochMilli());
        }
        builder.endObject();
        return builder;
    }

    @Override
    public boolean equals(Object other) {
        if (this == other) {
            return true;
        }

        if (other == null || getClass() != other.getClass()) {
            return false;
        }

        final DataFrameTransformConfig that = (DataFrameTransformConfig) other;

        return Objects.equals(this.id, that.id)
                && Objects.equals(this.source, that.source)
                && Objects.equals(this.dest, that.dest)
                && Objects.equals(this.frequency, that.frequency)
                && Objects.equals(this.syncConfig, that.syncConfig)
                && Objects.equals(this.headers, that.headers)
                && Objects.equals(this.pivotConfig, that.pivotConfig)
                && Objects.equals(this.description, that.description)
                && Objects.equals(this.createTime, that.createTime)
                && Objects.equals(this.transformVersion, that.transformVersion);
    }

    @Override
    public int hashCode(){
        return Objects.hash(id, source, dest, frequency, syncConfig, headers, pivotConfig, description, createTime, transformVersion);
    }

    @Override
    public String toString() {
        return Strings.toString(this, true, true);
    }

    public static DataFrameTransformConfig fromXContent(final XContentParser parser, @Nullable final String optionalTransformId,
            boolean lenient) {

        return lenient ? LENIENT_PARSER.apply(parser, optionalTransformId) : STRICT_PARSER.apply(parser, optionalTransformId);
    }
}
