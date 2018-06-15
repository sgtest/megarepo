/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.datafeed;

import org.elasticsearch.Version;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.persistent.PersistentTaskState;
import org.elasticsearch.xpack.core.ml.action.StartDatafeedAction;

import java.io.IOException;
import java.util.Locale;

import static org.elasticsearch.common.xcontent.ConstructingObjectParser.constructorArg;

public enum DatafeedState implements PersistentTaskState {

    STARTED, STOPPED, STARTING, STOPPING;

    public static final String NAME = StartDatafeedAction.TASK_NAME;

    private static final ConstructingObjectParser<DatafeedState, Void> PARSER =
            new ConstructingObjectParser<>(NAME, args -> fromString((String) args[0]));

    static {
        PARSER.declareString(constructorArg(), new ParseField("state"));
    }

    public static DatafeedState fromString(String name) {
        return valueOf(name.trim().toUpperCase(Locale.ROOT));
    }

    public static DatafeedState fromStream(StreamInput in) throws IOException {
        return in.readEnum(DatafeedState.class);
    }

    @Override
    public String getWriteableName() {
        return NAME;
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        DatafeedState state = this;
        // STARTING & STOPPING states were introduced in v5.5.
        if (out.getVersion().before(Version.V_5_5_0)) {
            if (this == STARTING) {
                state = STOPPED;
            } else if (this == STOPPING) {
                state = STARTED;
            }
        }
        out.writeEnum(state);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.field("state", name().toLowerCase(Locale.ROOT));
        builder.endObject();
        return builder;
    }

    @Override
    public boolean isFragment() {
        return false;
    }

    public static DatafeedState fromXContent(XContentParser parser) throws IOException {
        return PARSER.parse(parser, null);
    }

    @Override
    public String toString() {
        return name().toLowerCase(Locale.ROOT);
    }
}
