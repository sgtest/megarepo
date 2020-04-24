/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.searchablesnapshots;

import org.elasticsearch.Build;
import org.elasticsearch.common.settings.Settings;

import static org.elasticsearch.index.IndexModule.INDEX_STORE_TYPE_SETTING;

public class SearchableSnapshotsConstants {
    public static final boolean SEARCHABLE_SNAPSHOTS_FEATURE_ENABLED;

    static {
        final String property = System.getProperty("es.searchable_snapshots_feature_enabled");
        if ("true".equals(property)) {
            SEARCHABLE_SNAPSHOTS_FEATURE_ENABLED = true;
        } else if ("false".equals(property)) {
            SEARCHABLE_SNAPSHOTS_FEATURE_ENABLED = false;
        } else if (property == null) {
            SEARCHABLE_SNAPSHOTS_FEATURE_ENABLED = Build.CURRENT.isSnapshot();
        } else {
            throw new IllegalArgumentException(
                "expected es.searchable_snapshots_feature_enabled to be unset or [true|false] but was [" + property + "]"
            );
        }
    }

    public static final String SNAPSHOT_DIRECTORY_FACTORY_KEY = "snapshot";

    public static boolean isSearchableSnapshotStore(Settings indexSettings) {
        return SEARCHABLE_SNAPSHOTS_FEATURE_ENABLED
            && SNAPSHOT_DIRECTORY_FACTORY_KEY.equals(INDEX_STORE_TYPE_SETTING.get(indexSettings));
    }

    public static final String SEARCHABLE_SNAPSHOTS_THREAD_POOL_NAME = "searchable_snapshots";
}
