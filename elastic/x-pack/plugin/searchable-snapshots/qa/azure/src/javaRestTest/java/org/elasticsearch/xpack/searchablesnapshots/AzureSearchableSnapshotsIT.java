/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.searchablesnapshots;

import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.test.rest.ESRestTestCase;

import static org.hamcrest.Matchers.blankOrNullString;
import static org.hamcrest.Matchers.not;

public class AzureSearchableSnapshotsIT extends AbstractSearchableSnapshotsRestTestCase {

    @Override
    protected final Settings restClientSettings() {
        return Settings.builder()
            .put(super.restClientSettings())
            // increase the timeout here to 90 seconds
            // this ensures that we don't retry the test requests when azure is misbehaving
            // (the azure sdk timeout is 60s) see #87389
            .put(ESRestTestCase.CLIENT_SOCKET_TIMEOUT, "90s")
            .build();
    }

    @Override
    protected String writeRepositoryType() {
        return "azure";
    }

    @Override
    protected Settings writeRepositorySettings() {
        final String container = System.getProperty("test.azure.container");
        assertThat(container, not(blankOrNullString()));

        final String basePath = System.getProperty("test.azure.base_path");
        assertThat(basePath, not(blankOrNullString()));

        return Settings.builder().put("client", "searchable_snapshots").put("container", container).put("base_path", basePath).build();
    }
}
