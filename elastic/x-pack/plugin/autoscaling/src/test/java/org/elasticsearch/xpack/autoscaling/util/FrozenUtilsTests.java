/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.autoscaling.util;

import joptsimple.internal.Strings;
import org.elasticsearch.Version;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.IndexModule;
import org.elasticsearch.xpack.autoscaling.AutoscalingTestCase;
import org.elasticsearch.xpack.cluster.routing.allocation.DataTierAllocationDecider;
import org.elasticsearch.xpack.core.DataTier;
import org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshotsConstants;

import java.util.Objects;

import static org.hamcrest.Matchers.is;

public class FrozenUtilsTests extends AutoscalingTestCase {

    public void testIsFrozenIndex() {
        assertThat(FrozenUtils.isFrozenIndex(indexSettings(DataTier.DATA_FROZEN)), is(true));
        assertThat(FrozenUtils.isFrozenIndex(indexSettings(null)), is(false));
        String notFrozenAlone = randomNonFrozenTierPreference();
        assertThat(FrozenUtils.isFrozenIndex(indexSettings(notFrozenAlone)), is(false));
    }

    public static String randomNonFrozenTierPreference() {
        return randomValueOtherThanMany(
            tiers -> tiers.contains(DataTier.DATA_FROZEN),
            () -> Strings.join(randomSubsetOf(DataTier.ALL_DATA_TIERS), ",")
        );
    }

    public static Settings indexSettings(String tierPreference) {
        Settings.Builder settings = Settings.builder()
            .put(randomAlphaOfLength(10), randomLong())
            .put(DataTierAllocationDecider.INDEX_ROUTING_PREFER, tierPreference)
            .put(IndexMetadata.SETTING_VERSION_CREATED, Version.CURRENT);
        // pass setting validator.
        if (Objects.equals(tierPreference, DataTier.DATA_FROZEN)) {
            settings.put(SearchableSnapshotsConstants.SNAPSHOT_PARTIAL_SETTING.getKey(), true)
                .put(IndexModule.INDEX_STORE_TYPE_SETTING.getKey(), SearchableSnapshotsConstants.SNAPSHOT_DIRECTORY_FACTORY_KEY);
        }
        return settings.build();
    }
}
