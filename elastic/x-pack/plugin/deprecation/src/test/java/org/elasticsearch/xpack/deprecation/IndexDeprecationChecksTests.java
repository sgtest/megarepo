/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.deprecation;

import org.elasticsearch.Version;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.IndexModule;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.test.ESTestCase;

import java.util.List;

import static java.util.Collections.singletonList;
import static org.elasticsearch.xpack.deprecation.DeprecationChecks.INDEX_SETTINGS_CHECKS;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.collection.IsIterableContainingInOrder.contains;

public class IndexDeprecationChecksTests extends ESTestCase {
    public void testOldIndicesCheck() {
        Version createdWith = Version.fromString("1.0.0");
        IndexMetadata indexMetadata = IndexMetadata.builder("test")
            .settings(settings(createdWith))
            .numberOfShards(1)
            .numberOfReplicas(0)
            .build();
        DeprecationIssue expected = new DeprecationIssue(DeprecationIssue.Level.CRITICAL,
            "Index created before 7.0",
            "https://www.elastic.co/guide/en/elasticsearch/reference/master/" +
                "breaking-changes-8.0.html",
            "This index was created using version: " + createdWith, null);
        List<DeprecationIssue> issues = DeprecationChecks.filterChecks(INDEX_SETTINGS_CHECKS, c -> c.apply(indexMetadata));
        assertEquals(singletonList(expected), issues);
    }

    public void testTranslogRetentionSettings() {
        Settings.Builder settings = settings(Version.CURRENT);
        settings.put(IndexSettings.INDEX_TRANSLOG_RETENTION_AGE_SETTING.getKey(), randomPositiveTimeValue());
        settings.put(IndexSettings.INDEX_TRANSLOG_RETENTION_SIZE_SETTING.getKey(), between(1, 1024) + "b");
        IndexMetadata indexMetadata = IndexMetadata.builder("test").settings(settings).numberOfShards(1).numberOfReplicas(0).build();
        List<DeprecationIssue> issues = DeprecationChecks.filterChecks(INDEX_SETTINGS_CHECKS, c -> c.apply(indexMetadata));
        assertThat(issues, contains(
            new DeprecationIssue(DeprecationIssue.Level.WARNING,
                "translog retention settings are ignored",
                "https://www.elastic.co/guide/en/elasticsearch/reference/current/index-modules-translog.html",
                "translog retention settings [index.translog.retention.size] and [index.translog.retention.age] are ignored " +
                    "because translog is no longer used in peer recoveries with soft-deletes enabled (default in 7.0 or later)", null)
        ));
    }

    public void testDefaultTranslogRetentionSettings() {
        Settings.Builder settings = settings(Version.CURRENT);
        if (randomBoolean()) {
            settings.put(IndexSettings.INDEX_TRANSLOG_RETENTION_AGE_SETTING.getKey(), randomPositiveTimeValue());
            settings.put(IndexSettings.INDEX_TRANSLOG_RETENTION_SIZE_SETTING.getKey(), between(1, 1024) + "b");
            settings.put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), false);
        }
        IndexMetadata indexMetadata = IndexMetadata.builder("test").settings(settings).numberOfShards(1).numberOfReplicas(0).build();
        List<DeprecationIssue> issues = DeprecationChecks.filterChecks(INDEX_SETTINGS_CHECKS, c -> c.apply(indexMetadata));
        assertThat(issues, empty());
    }

    public void testIndexDataPathSetting() {
        Settings.Builder settings = settings(Version.CURRENT);
        settings.put(IndexMetadata.INDEX_DATA_PATH_SETTING.getKey(), createTempDir());
        IndexMetadata indexMetadata = IndexMetadata.builder("test").settings(settings).numberOfShards(1).numberOfReplicas(0).build();
        List<DeprecationIssue> issues = DeprecationChecks.filterChecks(INDEX_SETTINGS_CHECKS, c -> c.apply(indexMetadata));
        final String expectedUrl =
            "https://www.elastic.co/guide/en/elasticsearch/reference/7.13/breaking-changes-7.13.html#deprecate-shared-data-path-setting";
        assertThat(issues, contains(
            new DeprecationIssue(DeprecationIssue.Level.CRITICAL,
                "setting [index.data_path] is deprecated and will be removed in a future version",
                expectedUrl,
                "Found index data path configured. Discontinue use of this setting.",
                null)));
    }

    public void testSimpleFSSetting() {
        Settings.Builder settings = settings(Version.CURRENT);
        settings.put(IndexModule.INDEX_STORE_TYPE_SETTING.getKey(), "simplefs");
        IndexMetadata indexMetadata = IndexMetadata.builder("test").settings(settings).numberOfShards(1).numberOfReplicas(0).build();
        List<DeprecationIssue> issues = DeprecationChecks.filterChecks(INDEX_SETTINGS_CHECKS, c -> c.apply(indexMetadata));
        assertThat(issues, contains(
            new DeprecationIssue(DeprecationIssue.Level.WARNING,
                "[simplefs] is deprecated and will be removed in future versions",
                "https://www.elastic.co/guide/en/elasticsearch/reference/current/index-modules-store.html",
                "[simplefs] is deprecated and will be removed in 8.0. Use [niofs] or other file systems instead. " +
                    "Elasticsearch 7.15 or later uses [niofs] for the [simplefs] store type " +
                    "as it offers superior or equivalent performance to [simplefs].", null)
        ));
    }
}
