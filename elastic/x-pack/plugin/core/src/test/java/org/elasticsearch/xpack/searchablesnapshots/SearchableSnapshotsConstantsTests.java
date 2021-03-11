/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.searchablesnapshots;

import org.elasticsearch.index.IndexModule;
import org.elasticsearch.test.ESTestCase;

import java.util.Map;

import static org.hamcrest.Matchers.is;

public class SearchableSnapshotsConstantsTests extends ESTestCase {

    public void testIsPartialSearchableSnapshotIndex() {
        assertThat(SearchableSnapshotsConstants.isPartialSearchableSnapshotIndex(
            Map.of(IndexModule.INDEX_STORE_TYPE_SETTING, SearchableSnapshotsConstants.SNAPSHOT_DIRECTORY_FACTORY_KEY,
                SearchableSnapshotsConstants.SNAPSHOT_PARTIAL_SETTING, false)),
            is(false));

        assertThat(SearchableSnapshotsConstants.isPartialSearchableSnapshotIndex(
            Map.of(IndexModule.INDEX_STORE_TYPE_SETTING, "abc",
                SearchableSnapshotsConstants.SNAPSHOT_PARTIAL_SETTING, randomBoolean())),
            is(false));

        assertThat(SearchableSnapshotsConstants.isPartialSearchableSnapshotIndex(
            Map.of(IndexModule.INDEX_STORE_TYPE_SETTING, SearchableSnapshotsConstants.SNAPSHOT_DIRECTORY_FACTORY_KEY,
                SearchableSnapshotsConstants.SNAPSHOT_PARTIAL_SETTING, true)),
            is(true));
    }
}
