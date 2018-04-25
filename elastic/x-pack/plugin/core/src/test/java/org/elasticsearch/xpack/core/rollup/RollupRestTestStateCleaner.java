/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.rollup;

import org.apache.http.HttpStatus;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.admin.cluster.node.tasks.list.ListTasksAction;
import org.elasticsearch.client.Response;
import org.elasticsearch.client.RestClient;
import org.elasticsearch.common.xcontent.support.XContentMapValues;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.rest.ESRestTestCase;
import org.elasticsearch.xpack.core.rollup.job.RollupJob;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

import static org.junit.Assert.assertEquals;

public class RollupRestTestStateCleaner {

    private final Logger logger;
    private final RestClient adminClient;
    private final ESRestTestCase testCase;

    public RollupRestTestStateCleaner(Logger logger, RestClient adminClient, ESRestTestCase testCase) {
        this.logger = logger;
        this.adminClient = adminClient;
        this.testCase = testCase;
    }

    public void clearRollupMetadata() throws Exception {
        deleteAllJobs();
        waitForPendingTasks();
        // indices will be deleted by the ESIntegTestCase class
    }

    private void waitForPendingTasks() throws Exception {
        ESTestCase.assertBusy(() -> {
            try {
                Response response = adminClient.performRequest("GET", "/_cat/tasks",
                        Collections.singletonMap("detailed", "true"));
                if (response.getStatusLine().getStatusCode() == HttpStatus.SC_OK) {
                    try (BufferedReader responseReader = new BufferedReader(
                            new InputStreamReader(response.getEntity().getContent(), StandardCharsets.UTF_8))) {
                        int activeTasks = 0;
                        String line;
                        StringBuilder tasksListString = new StringBuilder();
                        while ((line = responseReader.readLine()) != null) {

                            // We only care about Rollup jobs, otherwise this fails too easily due to unrelated tasks
                            if (line.startsWith(RollupJob.NAME) == true) {
                                activeTasks++;
                                tasksListString.append(line);
                                tasksListString.append('\n');
                            }
                        }
                        assertEquals(activeTasks + " active tasks found:\n" + tasksListString, 0, activeTasks);
                    }
                }
            } catch (IOException e) {
                throw new AssertionError("Error getting active tasks list", e);
            }
        });
    }

    @SuppressWarnings("unchecked")
    private void deleteAllJobs() throws Exception {
        Response response = adminClient.performRequest("GET", "/_xpack/rollup/job/_all");
        Map<String, Object> jobs = testCase.entityAsMap(response);
        @SuppressWarnings("unchecked")
        List<Map<String, Object>> jobConfigs =
                (List<Map<String, Object>>) XContentMapValues.extractValue("jobs", jobs);

        if (jobConfigs == null) {
            return;
        }

        for (Map<String, Object> jobConfig : jobConfigs) {
            logger.debug(jobConfig);
            String jobId = (String) ((Map<String, Object>) jobConfig.get("config")).get("id");
            logger.debug("Deleting job " + jobId);
            try {
                response = adminClient.performRequest("DELETE", "/_xpack/rollup/job/" + jobId);
            } catch (Exception e) {
                // ok
            }
        }
    }

    private static String responseEntityToString(Response response) throws Exception {
        try (BufferedReader reader = new BufferedReader(new InputStreamReader(response.getEntity().getContent(), StandardCharsets.UTF_8))) {
            return reader.lines().collect(Collectors.joining("\n"));
        }
    }
}
