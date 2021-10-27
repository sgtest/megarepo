/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.client;

import org.elasticsearch.Version;
import org.elasticsearch.client.migration.DeprecationInfoRequest;
import org.elasticsearch.client.migration.DeprecationInfoResponse;
import org.elasticsearch.client.migration.GetFeatureUpgradeStatusRequest;
import org.elasticsearch.client.migration.GetFeatureUpgradeStatusResponse;
import org.elasticsearch.client.migration.PostFeatureUpgradeRequest;
import org.elasticsearch.client.migration.PostFeatureUpgradeResponse;
import org.elasticsearch.common.settings.Settings;

import java.io.IOException;
import java.util.Collections;
import java.util.Optional;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThanOrEqualTo;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.is;

public class MigrationIT extends ESRestHighLevelClientTestCase {

    public void testGetDeprecationInfo() throws IOException {
        createIndex("test", Settings.EMPTY);
        DeprecationInfoRequest request = new DeprecationInfoRequest(Collections.singletonList("test"));
        DeprecationInfoResponse response = highLevelClient().migration().getDeprecationInfo(request, RequestOptions.DEFAULT);
        // a test like this cannot test actual deprecations
        assertThat(response.getClusterSettingsIssues().size(), equalTo(0));
        assertThat(response.getIndexSettingsIssues().size(), equalTo(0));
        assertThat(response.getNodeSettingsIssues().size(), equalTo(0));
        assertThat(response.getMlSettingsIssues().size(), equalTo(0));
    }

    public void testGetFeatureUpgradeStatus() throws IOException {
        GetFeatureUpgradeStatusRequest request = new GetFeatureUpgradeStatusRequest();
        GetFeatureUpgradeStatusResponse response = highLevelClient().migration().getFeatureUpgradeStatus(request, RequestOptions.DEFAULT);
        assertThat(response.getUpgradeStatus(), equalTo("NO_MIGRATION_NEEDED"));
        assertThat(response.getFeatureUpgradeStatuses().size(), greaterThanOrEqualTo(1));
        Optional<GetFeatureUpgradeStatusResponse.FeatureUpgradeStatus> optionalTasksStatus = response.getFeatureUpgradeStatuses()
            .stream()
            .filter(status -> "tasks".equals(status.getFeatureName()))
            .findFirst();

        assertThat(optionalTasksStatus.isPresent(), is(true));

        GetFeatureUpgradeStatusResponse.FeatureUpgradeStatus tasksStatus = optionalTasksStatus.get();

        assertThat(tasksStatus.getUpgradeStatus(), equalTo("NO_MIGRATION_NEEDED"));
        assertThat(tasksStatus.getMinimumIndexVersion(), equalTo(Version.CURRENT.toString()));
        assertThat(tasksStatus.getFeatureName(), equalTo("tasks"));
    }

    public void testPostFeatureUpgradeStatus() throws IOException {
        PostFeatureUpgradeRequest request = new PostFeatureUpgradeRequest();
        PostFeatureUpgradeResponse response = highLevelClient().migration().postFeatureUpgrade(request, RequestOptions.DEFAULT);
        assertThat(response.isAccepted(), equalTo(false));
        assertThat(response.getFeatures(), hasSize(0));
        assertThat(response.getReason(), equalTo("No system indices require migration"));
    }
}
