/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.dataframe;

import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.test.AbstractSerializingTestCase;

import java.io.IOException;
import java.util.Objects;

import static org.elasticsearch.xpack.core.ml.dataframe.DataFrameAnalyticsConfigTests.randomValidId;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.not;

public class DataFrameAnalyticsConfigUpdateTests extends AbstractSerializingTestCase<DataFrameAnalyticsConfigUpdate> {

    @Override
    protected DataFrameAnalyticsConfigUpdate doParseInstance(XContentParser parser) throws IOException {
        return DataFrameAnalyticsConfigUpdate.PARSER.apply(parser, null).build();
    }

    @Override
    protected DataFrameAnalyticsConfigUpdate createTestInstance() {
        return randomUpdate(randomValidId());
    }

    @Override
    protected Writeable.Reader<DataFrameAnalyticsConfigUpdate> instanceReader() {
        return DataFrameAnalyticsConfigUpdate::new;
    }

    public static DataFrameAnalyticsConfigUpdate randomUpdate(String id) {
        DataFrameAnalyticsConfigUpdate.Builder builder = new DataFrameAnalyticsConfigUpdate.Builder(id);
        if (randomBoolean()) {
            builder.setDescription(randomAlphaOfLength(20));
        }
        if (randomBoolean()) {
            builder.setModelMemoryLimit(new ByteSizeValue(randomNonNegativeLong()));
        }
        if (randomBoolean()) {
            builder.setAllowLazyStart(randomBoolean());
        }
        return builder.build();
    }

    public void testMergeWithConfig_UpdatedDescription() {
        String id = randomValidId();
        DataFrameAnalyticsConfig config =
            DataFrameAnalyticsConfigTests.createRandomBuilder(id).setDescription("old description").build();
        DataFrameAnalyticsConfigUpdate update =
            new DataFrameAnalyticsConfigUpdate.Builder(id).setDescription("new description").build();
        assertThat(
            update.mergeWithConfig(config).build(),
            is(equalTo(new DataFrameAnalyticsConfig.Builder(config).setDescription("new description").build())));
    }

    public void testMergeWithConfig_UpdatedModelMemoryLimit() {
        String id = randomValidId();
        DataFrameAnalyticsConfig config =
            DataFrameAnalyticsConfigTests.createRandomBuilder(id).setModelMemoryLimit(new ByteSizeValue(1024)).build();
        DataFrameAnalyticsConfigUpdate update =
            new DataFrameAnalyticsConfigUpdate.Builder(id).setModelMemoryLimit(new ByteSizeValue(2048)).build();
        assertThat(
            update.mergeWithConfig(config).build(),
            is(equalTo(new DataFrameAnalyticsConfig.Builder(config).setModelMemoryLimit(new ByteSizeValue(2048)).build())));
    }

    public void testMergeWithConfig_UpdatedAllowLazyStart() {
        String id = randomValidId();
        DataFrameAnalyticsConfig config = DataFrameAnalyticsConfigTests.createRandomBuilder(id).setAllowLazyStart(false).build();
        DataFrameAnalyticsConfigUpdate update = new DataFrameAnalyticsConfigUpdate.Builder(id).setAllowLazyStart(true).build();
        assertThat(
            update.mergeWithConfig(config).build(),
            is(equalTo(new DataFrameAnalyticsConfig.Builder(config).setAllowLazyStart(true).build())));
    }

    public void testMergeWithConfig_UpdatedAllUpdatableProperties() {
        String id = randomValidId();
        DataFrameAnalyticsConfig config =
            DataFrameAnalyticsConfigTests.createRandomBuilder(id)
                .setDescription("old description")
                .setModelMemoryLimit(new ByteSizeValue(1024))
                .setAllowLazyStart(false)
                .build();
        DataFrameAnalyticsConfigUpdate update =
            new DataFrameAnalyticsConfigUpdate.Builder(id)
                .setDescription("new description")
                .setModelMemoryLimit(new ByteSizeValue(2048))
                .setAllowLazyStart(true)
                .build();
        assertThat(
            update.mergeWithConfig(config).build(),
            is(equalTo(
                new DataFrameAnalyticsConfig.Builder(config)
                    .setDescription("new description")
                    .setModelMemoryLimit(new ByteSizeValue(2048))
                    .setAllowLazyStart(true)
                    .build())));
    }

    public void testMergeWithConfig_NoopUpdate() {
        String id = randomValidId();

        DataFrameAnalyticsConfig config = DataFrameAnalyticsConfigTests.createRandom(id);
        DataFrameAnalyticsConfigUpdate update = new DataFrameAnalyticsConfigUpdate.Builder(id).build();
        assertThat(update.mergeWithConfig(config).build(), is(equalTo(config)));
    }

    public void testMergeWithConfig_GivenRandomUpdates_AssertImmutability() {
        String id = randomValidId();

        for (int i = 0; i < 100; ++i) {
            DataFrameAnalyticsConfig config = DataFrameAnalyticsConfigTests.createRandom(id);
            DataFrameAnalyticsConfigUpdate update;
            do {
                update = randomUpdate(id);
            } while (isNoop(config, update));

            assertThat(update.mergeWithConfig(config).build(), is(not(equalTo(config))));
        }
    }

    public void testMergeWithConfig_failBecauseTargetConfigHasDifferentId() {
        String id = randomValidId();

        DataFrameAnalyticsConfig config = DataFrameAnalyticsConfigTests.createRandom(id);
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> randomUpdate(id + "_2").mergeWithConfig(config));
        assertThat(e.getMessage(), containsString("different id"));
    }

    public void testRequiresRestart_DescriptionUpdateDoesNotRequireRestart() {
        String id = randomValidId();
        DataFrameAnalyticsConfig config =
            DataFrameAnalyticsConfigTests.createRandomBuilder(id).setDescription("old description").build();
        DataFrameAnalyticsConfigUpdate update =
            new DataFrameAnalyticsConfigUpdate.Builder(id).setDescription("new description").build();

        assertThat(update.requiresRestart(config), is(false));
    }

    public void testRequiresRestart_ModelMemoryLimitUpdateRequiresRestart() {
        String id = randomValidId();
        DataFrameAnalyticsConfig config =
            DataFrameAnalyticsConfigTests.createRandomBuilder(id).setModelMemoryLimit(new ByteSizeValue(1024)).build();
        DataFrameAnalyticsConfigUpdate update =
            new DataFrameAnalyticsConfigUpdate.Builder(id).setModelMemoryLimit(new ByteSizeValue(2048)).build();

        assertThat(update.requiresRestart(config), is(true));
    }

    private boolean isNoop(DataFrameAnalyticsConfig config, DataFrameAnalyticsConfigUpdate update) {
        return (update.getDescription() == null || Objects.equals(config.getDescription(), update.getDescription()))
            && (update.getModelMemoryLimit() == null || Objects.equals(config.getModelMemoryLimit(), update.getModelMemoryLimit()))
            && (update.isAllowLazyStart() == null || Objects.equals(config.isAllowLazyStart(), update.isAllowLazyStart()));
    }
}
