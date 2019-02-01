/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.client;

import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.client.migration.DeprecationInfoRequest;
import org.elasticsearch.client.migration.DeprecationInfoResponse;
import org.elasticsearch.client.migration.IndexUpgradeInfoRequest;
import org.elasticsearch.client.migration.IndexUpgradeInfoResponse;
import org.elasticsearch.client.migration.IndexUpgradeRequest;
import org.elasticsearch.client.tasks.TaskSubmissionResponse;
import org.elasticsearch.common.settings.Settings;

import java.io.IOException;
import java.util.Collections;
import java.util.function.BooleanSupplier;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;

public class MigrationIT extends ESRestHighLevelClientTestCase {

    public void testGetAssistance() throws IOException {
        {
            IndexUpgradeInfoResponse response = highLevelClient().migration()
                .getAssistance(new IndexUpgradeInfoRequest(), RequestOptions.DEFAULT);
            assertEquals(0, response.getActions().size());
        }
        {
            createIndex("test", Settings.EMPTY);
            IndexUpgradeInfoResponse response = highLevelClient().migration().getAssistance(
                new IndexUpgradeInfoRequest("test"), RequestOptions.DEFAULT);
            assertEquals(0, response.getActions().size());
        }
    }

    public void testUpgradeWhenIndexCannotBeUpgraded() throws IOException {
        createIndex("test", Settings.EMPTY);

        ThrowingRunnable execute = () -> execute(new IndexUpgradeRequest("test"),
            highLevelClient().migration()::upgrade,
            highLevelClient().migration()::upgradeAsync);

        ElasticsearchStatusException responseException = expectThrows(ElasticsearchStatusException.class, execute);

        assertThat(responseException.getDetailedMessage(), containsString("cannot be upgraded"));
    }

    public void testUpgradeWithTaskApi() throws IOException, InterruptedException {
        createIndex("test", Settings.EMPTY);

        IndexUpgradeRequest request = new IndexUpgradeRequest("test");

        TaskSubmissionResponse upgrade = highLevelClient().migration()
            .submitUpgradeTask(request, RequestOptions.DEFAULT);

        assertNotNull(upgrade.getTask());

        BooleanSupplier hasUpgradeCompleted = checkCompletionStatus(upgrade);
        awaitBusy(hasUpgradeCompleted);
    }

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

    /**
     * Using low-level api as high-level-rest-client's getTaskById work is in progress.
     * TODO revisit once that work is finished
     */
    private BooleanSupplier checkCompletionStatus(TaskSubmissionResponse upgrade) {
        return () -> {
            try {
                Response response = client().performRequest(new Request("GET", "/_tasks/" + upgrade.getTask()));
                return (boolean) entityAsMap(response).get("completed");
            } catch (IOException e) {
                fail(e.getMessage());
                return false;
            }
        };
    }
}
