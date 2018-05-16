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

package org.elasticsearch.client.documentation;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.FailedNodeException;
import org.elasticsearch.action.LatchedActionListener;
import org.elasticsearch.action.TaskOperationFailure;
import org.elasticsearch.action.admin.cluster.node.tasks.list.ListTasksRequest;
import org.elasticsearch.action.admin.cluster.node.tasks.list.ListTasksResponse;
import org.elasticsearch.action.admin.cluster.node.tasks.list.TaskGroup;
import org.elasticsearch.action.admin.cluster.settings.ClusterUpdateSettingsRequest;
import org.elasticsearch.action.admin.cluster.settings.ClusterUpdateSettingsResponse;
import org.elasticsearch.client.ESRestHighLevelClientTestCase;
import org.elasticsearch.client.RestHighLevelClient;
import org.elasticsearch.cluster.routing.allocation.decider.EnableAllocationDecider;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.indices.recovery.RecoverySettings;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.tasks.TaskInfo;

import java.io.IOException;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

import static java.util.Collections.emptyList;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThanOrEqualTo;
import static org.hamcrest.Matchers.notNullValue;

/**
 * This class is used to generate the Java Cluster API documentation.
 * You need to wrap your code between two tags like:
 * // tag::example
 * // end::example
 *
 * Where example is your tag name.
 *
 * Then in the documentation, you can extract what is between tag and end tags with
 * ["source","java",subs="attributes,callouts,macros"]
 * --------------------------------------------------
 * include-tagged::{doc-tests}/ClusterClientDocumentationIT.java[example]
 * --------------------------------------------------
 *
 * The column width of the code block is 84. If the code contains a line longer
 * than 84, the line will be cut and a horizontal scroll bar will be displayed.
 * (the code indentation of the tag is not included in the width)
 */
public class ClusterClientDocumentationIT extends ESRestHighLevelClientTestCase {

    public void testClusterPutSettings() throws IOException {
        RestHighLevelClient client = highLevelClient();

        // tag::put-settings-request
        ClusterUpdateSettingsRequest request = new ClusterUpdateSettingsRequest();
        // end::put-settings-request

        // tag::put-settings-create-settings
        String transientSettingKey = 
                RecoverySettings.INDICES_RECOVERY_MAX_BYTES_PER_SEC_SETTING.getKey();
        int transientSettingValue = 10;
        Settings transientSettings = 
                Settings.builder()
                .put(transientSettingKey, transientSettingValue, ByteSizeUnit.BYTES)
                .build(); // <1>

        String persistentSettingKey = 
                EnableAllocationDecider.CLUSTER_ROUTING_ALLOCATION_ENABLE_SETTING.getKey();
        String persistentSettingValue = 
                EnableAllocationDecider.Allocation.NONE.name();
        Settings persistentSettings = 
                Settings.builder()
                .put(persistentSettingKey, persistentSettingValue)
                .build(); // <2>
        // end::put-settings-create-settings

        // tag::put-settings-request-cluster-settings
        request.transientSettings(transientSettings); // <1>
        request.persistentSettings(persistentSettings); // <2>
        // end::put-settings-request-cluster-settings

        {
            // tag::put-settings-settings-builder
            Settings.Builder transientSettingsBuilder = 
                    Settings.builder()
                    .put(transientSettingKey, transientSettingValue, ByteSizeUnit.BYTES); 
            request.transientSettings(transientSettingsBuilder); // <1>
            // end::put-settings-settings-builder
        }
        {
            // tag::put-settings-settings-map
            Map<String, Object> map = new HashMap<>();
            map.put(transientSettingKey
                    , transientSettingValue + ByteSizeUnit.BYTES.getSuffix());
            request.transientSettings(map); // <1>
            // end::put-settings-settings-map
        }
        {
            // tag::put-settings-settings-source
            request.transientSettings(
                    "{\"indices.recovery.max_bytes_per_sec\": \"10b\"}"
                    , XContentType.JSON); // <1>
            // end::put-settings-settings-source
        }

        // tag::put-settings-request-timeout
        request.timeout(TimeValue.timeValueMinutes(2)); // <1>
        request.timeout("2m"); // <2>
        // end::put-settings-request-timeout
        // tag::put-settings-request-masterTimeout
        request.masterNodeTimeout(TimeValue.timeValueMinutes(1)); // <1>
        request.masterNodeTimeout("1m"); // <2>
        // end::put-settings-request-masterTimeout

        // tag::put-settings-execute
        ClusterUpdateSettingsResponse response = client.cluster().putSettings(request);
        // end::put-settings-execute

        // tag::put-settings-response
        boolean acknowledged = response.isAcknowledged(); // <1>
        Settings transientSettingsResponse = response.getTransientSettings(); // <2>
        Settings persistentSettingsResponse = response.getPersistentSettings(); // <3>
        // end::put-settings-response
        assertTrue(acknowledged);
        assertThat(transientSettingsResponse.get(transientSettingKey), equalTo(transientSettingValue + ByteSizeUnit.BYTES.getSuffix()));
        assertThat(persistentSettingsResponse.get(persistentSettingKey), equalTo(persistentSettingValue));

        // tag::put-settings-request-reset-transient
        request.transientSettings(Settings.builder().putNull(transientSettingKey).build()); // <1>
        // tag::put-settings-request-reset-transient
        request.persistentSettings(Settings.builder().putNull(persistentSettingKey));
        ClusterUpdateSettingsResponse resetResponse = client.cluster().putSettings(request);

        assertTrue(resetResponse.isAcknowledged());
    }

    public void testClusterUpdateSettingsAsync() throws Exception {
        RestHighLevelClient client = highLevelClient();
        {
            ClusterUpdateSettingsRequest request = new ClusterUpdateSettingsRequest();

            // tag::put-settings-execute-listener
            ActionListener<ClusterUpdateSettingsResponse> listener = 
                    new ActionListener<ClusterUpdateSettingsResponse>() {
                @Override
                public void onResponse(ClusterUpdateSettingsResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::put-settings-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::put-settings-execute-async
            client.cluster().putSettingsAsync(request, listener); // <1>
            // end::put-settings-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testListTasks() throws IOException {
        RestHighLevelClient client = highLevelClient();
        {
            // tag::list-tasks-request
            ListTasksRequest request = new ListTasksRequest();
            // end::list-tasks-request

            // tag::list-tasks-request-filter
            request.setActions("cluster:*"); // <1>
            request.setNodes("nodeId1", "nodeId2"); // <2>
            request.setParentTaskId(new TaskId("parentTaskId", 42)); // <3>
            // end::list-tasks-request-filter

            // tag::list-tasks-request-detailed
            request.setDetailed(true); // <1>
            // end::list-tasks-request-detailed

            // tag::list-tasks-request-wait-completion
            request.setWaitForCompletion(true); // <1>
            request.setTimeout(TimeValue.timeValueSeconds(50)); // <2>
            request.setTimeout("50s"); // <3>
            // end::list-tasks-request-wait-completion
        }

        ListTasksRequest request = new ListTasksRequest();

        // tag::list-tasks-execute
        ListTasksResponse response = client.cluster().listTasks(request);
        // end::list-tasks-execute

        assertThat(response, notNullValue());

        // tag::list-tasks-response-tasks
        List<TaskInfo> tasks = response.getTasks(); // <1>
        // end::list-tasks-response-tasks

        // tag::list-tasks-response-calc
        Map<String, List<TaskInfo>> perNodeTasks = response.getPerNodeTasks(); // <1>
        List<TaskGroup> groups = response.getTaskGroups(); // <2>
        // end::list-tasks-response-calc

        // tag::list-tasks-response-failures
        List<ElasticsearchException> nodeFailures = response.getNodeFailures(); // <1>
        List<TaskOperationFailure> taskFailures = response.getTaskFailures(); // <2>
        // end::list-tasks-response-failures

        assertThat(response.getNodeFailures(), equalTo(emptyList()));
        assertThat(response.getTaskFailures(), equalTo(emptyList()));
        assertThat(response.getTasks().size(), greaterThanOrEqualTo(2));
    }

    public void testListTasksAsync() throws Exception {
        RestHighLevelClient client = highLevelClient();
        {
            ListTasksRequest request = new ListTasksRequest();

            // tag::list-tasks-execute-listener
            ActionListener<ListTasksResponse> listener =
                    new ActionListener<ListTasksResponse>() {
                        @Override
                        public void onResponse(ListTasksResponse response) {
                            // <1>
                        }

                        @Override
                        public void onFailure(Exception e) {
                            // <2>
                        }
                    };
            // end::list-tasks-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::list-tasks-execute-async
            client.cluster().listTasksAsync(request, listener); // <1>
            // end::list-tasks-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }
}
