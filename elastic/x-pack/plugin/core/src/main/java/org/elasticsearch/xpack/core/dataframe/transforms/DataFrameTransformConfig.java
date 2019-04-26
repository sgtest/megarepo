/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.core.dataframe.transforms;

import org.elasticsearch.cluster.AbstractDiffable;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.xpack.core.dataframe.DataFrameField;
import org.elasticsearch.xpack.core.dataframe.DataFrameMessages;
import org.elasticsearch.xpack.core.dataframe.transforms.pivot.PivotConfig;
import org.elasticsearch.xpack.core.dataframe.utils.ExceptionsHelper;

import java.io.IOException;
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
    private static final ConstructingObjectParser<DataFrameTransformConfig, String> STRICT_PARSER = createParser(false);
    private static final ConstructingObjectParser<DataFrameTransformConfig, String> LENIENT_PARSER = createParser(true);
    private static final int MAX_DESCRIPTION_LENGTH = 1_000;

    private final String id;
    private final SourceConfig source;
    private final DestConfig dest;
    private final String description;
    // headers store the user context from the creating user, which allows us to run the transform as this user
    // the header only contains name, groups and other context but no authorization keys
    private Map<String, String> headers;

    private final PivotConfig pivotConfig;

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

                    // ignored, only for internal storage: String docType = (String) args[3];

                    // on strict parsing do not allow injection of headers
                    if (lenient == false && args[4] != null) {
                        throw new IllegalArgumentException("Found [headers], not allowed for strict parsing");
                    }

                    @SuppressWarnings("unchecked")
                    Map<String, String> headers = (Map<String, String>) args[4];

                    PivotConfig pivotConfig = (PivotConfig) args[5];
                    String description = (String)args[6];
                    return new DataFrameTransformConfig(id, source, dest, headers, pivotConfig, description);
                });

        parser.declareString(optionalConstructorArg(), DataFrameField.ID);
        parser.declareObject(constructorArg(), (p, c) -> SourceConfig.fromXContent(p, lenient), DataFrameField.SOURCE);
        parser.declareObject(constructorArg(), (p, c) -> DestConfig.fromXContent(p, lenient), DataFrameField.DESTINATION);

        parser.declareString(optionalConstructorArg(), DataFrameField.INDEX_DOC_TYPE);
        parser.declareObject(optionalConstructorArg(), (p, c) -> p.mapStrings(), HEADERS);
        parser.declareObject(optionalConstructorArg(), (p, c) -> PivotConfig.fromXContent(p, lenient), PIVOT_TRANSFORM);
        parser.declareString(optionalConstructorArg(), DESCRIPTION);

        return parser;
    }

    public static String documentId(String transformId) {
        return NAME + "-" + transformId;
    }

    public DataFrameTransformConfig(final String id,
                                    final SourceConfig source,
                                    final DestConfig dest,
                                    final Map<String, String> headers,
                                    final PivotConfig pivotConfig,
                                    final String description) {
        this.id = ExceptionsHelper.requireNonNull(id, DataFrameField.ID.getPreferredName());
        this.source = ExceptionsHelper.requireNonNull(source, DataFrameField.SOURCE.getPreferredName());
        this.dest = ExceptionsHelper.requireNonNull(dest, DataFrameField.DESTINATION.getPreferredName());
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
    }

    public DataFrameTransformConfig(final StreamInput in) throws IOException {
        id = in.readString();
        source = new SourceConfig(in);
        dest = new DestConfig(in);
        setHeaders(in.readMap(StreamInput::readString, StreamInput::readString));
        pivotConfig = in.readOptionalWriteable(PivotConfig::new);
        description = in.readOptionalString();
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

    public Map<String, String> getHeaders() {
        return headers;
    }

    public void setHeaders(Map<String, String> headers) {
        this.headers = headers;
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

        return source.isValid() && dest.isValid();
    }

    @Override
    public void writeTo(final StreamOutput out) throws IOException {
        out.writeString(id);
        source.writeTo(out);
        dest.writeTo(out);
        out.writeMap(headers, StreamOutput::writeString, StreamOutput::writeString);
        out.writeOptionalWriteable(pivotConfig);
        out.writeOptionalString(description);
    }

    @Override
    public XContentBuilder toXContent(final XContentBuilder builder, final Params params) throws IOException {
        builder.startObject();
        builder.field(DataFrameField.ID.getPreferredName(), id);
        builder.field(DataFrameField.SOURCE.getPreferredName(), source);
        builder.field(DataFrameField.DESTINATION.getPreferredName(), dest);
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
                && Objects.equals(this.headers, that.headers)
                && Objects.equals(this.pivotConfig, that.pivotConfig)
                && Objects.equals(this.description, that.description);
    }

    @Override
    public int hashCode(){
        return Objects.hash(id, source, dest, headers, pivotConfig, description);
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
