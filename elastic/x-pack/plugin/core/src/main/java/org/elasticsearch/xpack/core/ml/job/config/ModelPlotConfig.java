/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.job.config;

import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.xpack.core.ml.MlParserType;

import java.io.IOException;
import java.util.EnumMap;
import java.util.Map;
import java.util.Objects;

public class ModelPlotConfig implements ToXContentObject, Writeable {

    private static final ParseField TYPE_FIELD = new ParseField("model_plot_config");
    private static final ParseField ENABLED_FIELD = new ParseField("enabled");
    public static final ParseField TERMS_FIELD = new ParseField("terms");

    // These parsers follow the pattern that metadata is parsed leniently (to allow for enhancements), whilst config is parsed strictly
    public static final ConstructingObjectParser<ModelPlotConfig, Void> METADATA_PARSER =
            new ConstructingObjectParser<>(TYPE_FIELD.getPreferredName(), true,
                    a -> new ModelPlotConfig((boolean) a[0], (String) a[1]));
    public static final ConstructingObjectParser<ModelPlotConfig, Void> CONFIG_PARSER =
            new ConstructingObjectParser<>(TYPE_FIELD.getPreferredName(), false,
                    a -> new ModelPlotConfig((boolean) a[0], (String) a[1]));
    public static final Map<MlParserType, ConstructingObjectParser<ModelPlotConfig, Void>> PARSERS =
            new EnumMap<>(MlParserType.class);

    static {
        PARSERS.put(MlParserType.METADATA, METADATA_PARSER);
        PARSERS.put(MlParserType.CONFIG, CONFIG_PARSER);
        for (MlParserType parserType : MlParserType.values()) {
            ConstructingObjectParser<ModelPlotConfig, Void> parser = PARSERS.get(parserType);
            assert parser != null;
            parser.declareBoolean(ConstructingObjectParser.constructorArg(), ENABLED_FIELD);
            parser.declareString(ConstructingObjectParser.optionalConstructorArg(), TERMS_FIELD);
        }
    }

    private final boolean enabled;
    private final String terms;

    public ModelPlotConfig() {
        this(true, null);
    }

    public ModelPlotConfig(boolean enabled, String terms) {
        this.enabled = enabled;
        this.terms = terms;
    }

    public ModelPlotConfig(StreamInput in) throws IOException {
        enabled = in.readBoolean();
        terms = in.readOptionalString();
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeBoolean(enabled);
        out.writeOptionalString(terms);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.field(ENABLED_FIELD.getPreferredName(), enabled);
        if (terms != null) {
            builder.field(TERMS_FIELD.getPreferredName(), terms);
        }
        builder.endObject();
        return builder;
    }

    public boolean isEnabled() {
        return enabled;
    }

    public String getTerms() {
        return this.terms;
    }

    @Override
    public boolean equals(Object other) {
        if (this == other) {
            return true;
        }

        if (other instanceof ModelPlotConfig == false) {
            return false;
        }

        ModelPlotConfig that = (ModelPlotConfig) other;
        return this.enabled == that.enabled && Objects.equals(this.terms, that.terms);
    }

    @Override
    public int hashCode() {
        return Objects.hash(enabled, terms);
    }
}
