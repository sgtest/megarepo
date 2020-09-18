/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ilm;

import org.elasticsearch.client.Client;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.xpack.cluster.routing.allocation.DataTierAllocationDecider;
import org.elasticsearch.xpack.core.DataTier;
import org.elasticsearch.xpack.core.ilm.Step.StepKey;

import java.io.IOException;
import java.util.Arrays;
import java.util.List;
import java.util.Objects;

/**
 * A {@link LifecycleAction} which enables or disables the automatic migration of data between
 * {@link org.elasticsearch.xpack.core.DataTier}s.
 */
public class MigrateAction implements LifecycleAction {
    public static final String NAME = "migrate";
    public static final ParseField ENABLED_FIELD = new ParseField("enabled");

    private static final ConstructingObjectParser<MigrateAction, Void> PARSER = new ConstructingObjectParser<>(NAME,
        a -> new MigrateAction(a[0] == null ? true : (boolean) a[0]));

    static {
        PARSER.declareBoolean(ConstructingObjectParser.optionalConstructorArg(), ENABLED_FIELD);
    }

    private final boolean enabled;

    public static MigrateAction parse(XContentParser parser) {
        return PARSER.apply(parser, null);
    }

    public MigrateAction() {
        this(true);
    }

    public MigrateAction(boolean enabled) {
        this.enabled = enabled;
    }

    public MigrateAction(StreamInput in) throws IOException {
        this(in.readBoolean());
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeBoolean(enabled);
    }

    @Override
    public String getWriteableName() {
        return NAME;
    }

    public boolean isEnabled() {
        return enabled;
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.field(ENABLED_FIELD.getPreferredName(), enabled);
        builder.endObject();
        return builder;
    }

    @Override
    public boolean isSafeAction() {
        return true;
    }

    @Override
    public List<Step> toSteps(Client client, String phase, StepKey nextStepKey) {
        if (enabled) {
            StepKey migrationKey = new StepKey(phase, NAME, NAME);
            StepKey migrationRoutedKey = new StepKey(phase, NAME, DataTierMigrationRoutedStep.NAME);

            Settings.Builder migrationSettings = Settings.builder();
            String dataTierName = "data_" + phase;
            assert DataTier.validTierName(dataTierName) : "invalid data tier name:" + dataTierName;
            migrationSettings.put(DataTierAllocationDecider.INDEX_ROUTING_PREFER, dataTierName);
            UpdateSettingsStep updateMigrationSettingStep = new UpdateSettingsStep(migrationKey, migrationRoutedKey, client,
                migrationSettings.build());
            DataTierMigrationRoutedStep migrationRoutedStep = new DataTierMigrationRoutedStep(migrationRoutedKey, nextStepKey);
            return Arrays.asList(updateMigrationSettingStep, migrationRoutedStep);
        } else {
            return List.of();
        }
    }

    @Override
    public int hashCode() {
        return Objects.hash(enabled);
    }

    @Override
    public boolean equals(Object obj) {
        if (obj == null) {
            return false;
        }
        if (obj.getClass() != getClass()) {
            return false;
        }
        MigrateAction other = (MigrateAction) obj;
        return Objects.equals(enabled, other.enabled);
    }

    @Override
    public String toString() {
        return Strings.toString(this);
    }

}
