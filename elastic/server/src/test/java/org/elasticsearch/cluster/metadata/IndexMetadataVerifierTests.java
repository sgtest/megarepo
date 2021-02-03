/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.cluster.metadata;

import org.elasticsearch.Version;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.settings.IndexScopedSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.indices.mapper.MapperRegistry;
import org.elasticsearch.plugins.MapperPlugin;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.VersionUtils;

import java.util.Collections;

import static org.elasticsearch.test.VersionUtils.randomIndexCompatibleVersion;
import static org.hamcrest.Matchers.equalTo;

public class IndexMetadataVerifierTests extends ESTestCase {

    public void testArchiveBrokenIndexSettings() {
        IndexMetadataVerifier service = getIndexMetadataVerifier();
        IndexMetadata src = newIndexMeta("foo", Settings.EMPTY);
        IndexMetadata indexMetadata = service.archiveBrokenIndexSettings(src);
        assertSame(indexMetadata, src);

        src = newIndexMeta("foo", Settings.builder().put("index.refresh_interval", "-200").build());
        indexMetadata = service.archiveBrokenIndexSettings(src);
        assertNotSame(indexMetadata, src);
        assertEquals("-200", indexMetadata.getSettings().get("archived.index.refresh_interval"));

        src = newIndexMeta("foo", Settings.builder().put("index.codec", "best_compression1").build());
        indexMetadata = service.archiveBrokenIndexSettings(src);
        assertNotSame(indexMetadata, src);
        assertEquals("best_compression1", indexMetadata.getSettings().get("archived.index.codec"));

        src = newIndexMeta("foo", Settings.builder().put("index.refresh.interval", "-1").build());
        indexMetadata = service.archiveBrokenIndexSettings(src);
        assertNotSame(indexMetadata, src);
        assertEquals("-1", indexMetadata.getSettings().get("archived.index.refresh.interval"));

        src = newIndexMeta("foo", indexMetadata.getSettings()); // double archive?
        indexMetadata = service.archiveBrokenIndexSettings(src);
        assertSame(indexMetadata, src);
    }

    public void testCustomSimilarity() {
        IndexMetadataVerifier service = getIndexMetadataVerifier();
        IndexMetadata src = newIndexMeta("foo",
            Settings.builder()
                .put("index.similarity.my_similarity.type", "DFR")
                .put("index.similarity.my_similarity.after_effect", "l")
                .build());
        service.verifyIndexMetadata(src, Version.CURRENT.minimumIndexCompatibilityVersion());
    }

    public void testIncompatibleVersion() {
        IndexMetadataVerifier service = getIndexMetadataVerifier();
        Version minCompat = Version.CURRENT.minimumIndexCompatibilityVersion();
        Version indexCreated = Version.fromString((minCompat.major - 1) + "." + randomInt(5) + "." + randomInt(5));
        final IndexMetadata metadata = newIndexMeta("foo", Settings.builder()
            .put(IndexMetadata.SETTING_VERSION_CREATED, indexCreated)
            .build());
        String message = expectThrows(IllegalStateException.class, () -> service.verifyIndexMetadata(metadata,
            Version.CURRENT.minimumIndexCompatibilityVersion())).getMessage();
        assertThat(message, equalTo("The index [foo/" + metadata.getIndexUUID() + "] was created with version [" + indexCreated + "] " +
             "but the minimum compatible version is [" + minCompat + "]." +
            " It should be re-indexed in Elasticsearch " + minCompat.major + ".x before upgrading to " + Version.CURRENT.toString() + "."));

        indexCreated = VersionUtils.randomVersionBetween(random(), minCompat, Version.CURRENT);
        IndexMetadata goodMeta = newIndexMeta("foo", Settings.builder()
            .put(IndexMetadata.SETTING_VERSION_CREATED, indexCreated)
            .build());
        service.verifyIndexMetadata(goodMeta, Version.CURRENT.minimumIndexCompatibilityVersion());
    }

    private IndexMetadataVerifier getIndexMetadataVerifier() {
        return new IndexMetadataVerifier(
            Settings.EMPTY,
            xContentRegistry(),
            new MapperRegistry(Collections.emptyMap(), Collections.emptyMap(), null, Collections.emptyMap(),
                MapperPlugin.NOOP_FIELD_FILTER), IndexScopedSettings.DEFAULT_SCOPED_SETTINGS,
            null
        );
    }

    public static IndexMetadata newIndexMeta(String name, Settings indexSettings) {
        final Settings settings = Settings.builder()
            .put(IndexMetadata.SETTING_VERSION_CREATED, randomIndexCompatibleVersion(random()))
            .put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, between(0, 5))
            .put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, between(1, 5))
            .put(IndexMetadata.SETTING_CREATION_DATE, randomNonNegativeLong())
            .put(IndexMetadata.SETTING_INDEX_UUID, UUIDs.randomBase64UUID(random()))
            .put(indexSettings)
            .build();
        final IndexMetadata.Builder indexMetadataBuilder = IndexMetadata.builder(name).settings(settings);
        if (randomBoolean()) {
            indexMetadataBuilder.state(IndexMetadata.State.CLOSE);
        }
        return indexMetadataBuilder.build();
    }
}
