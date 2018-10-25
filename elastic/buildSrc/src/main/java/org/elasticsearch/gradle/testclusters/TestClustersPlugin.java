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
package org.elasticsearch.gradle.testclusters;

import groovy.lang.Closure;
import org.elasticsearch.GradleServicesAdapter;
import org.gradle.api.NamedDomainObjectContainer;
import org.gradle.api.Plugin;
import org.gradle.api.Project;
import org.gradle.api.Task;
import org.gradle.api.execution.TaskActionListener;
import org.gradle.api.execution.TaskExecutionListener;
import org.gradle.api.logging.Logger;
import org.gradle.api.logging.Logging;
import org.gradle.api.plugins.ExtraPropertiesExtension;
import org.gradle.api.tasks.TaskState;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

public class TestClustersPlugin implements Plugin<Project> {

    private static final String LIST_TASK_NAME = "listTestClusters";
    private static final String NODE_EXTENSION_NAME = "testClusters";

    private final Logger logger =  Logging.getLogger(TestClustersPlugin.class);

    @Override
    public void apply(Project project) {
        NamedDomainObjectContainer<? extends ElasticsearchNode> container = project.container(
            ElasticsearchNode.class,
            (name) -> new ElasticsearchNode(name, GradleServicesAdapter.getInstance(project))
        );
        project.getExtensions().add(NODE_EXTENSION_NAME, container);

        Task listTask = project.getTasks().create(LIST_TASK_NAME);
        listTask.setGroup("ES cluster formation");
        listTask.setDescription("Lists all ES clusters configured for this project");
        listTask.doLast((Task task) ->
            container.forEach((ElasticsearchNode cluster) ->
                logger.lifecycle("   * {}: {}", cluster.getName(), cluster.getDistribution())
            )
        );

        Map<Task, List<ElasticsearchNode>> taskToCluster = new HashMap<>();

        // register an extension for all current and future tasks, so that any task can declare that it wants to use a
        // specific cluster.
        project.getTasks().all((Task task) ->
            task.getExtensions().findByType(ExtraPropertiesExtension.class)
            .set(
                "useCluster",
                new Closure<Void>(this, this) {
                    public void doCall(ElasticsearchNode conf) {
                        taskToCluster.computeIfAbsent(task, k -> new ArrayList<>()).add(conf);
                    }
                })
        );

        project.getGradle().getTaskGraph().whenReady(taskExecutionGraph ->
            taskExecutionGraph.getAllTasks()
                .forEach(task ->
                    taskToCluster.getOrDefault(task, Collections.emptyList()).forEach(ElasticsearchNode::claim)
                )
        );
        project.getGradle().addListener(
            new TaskActionListener() {
                @Override
                public void beforeActions(Task task) {
                    // we only start the cluster before the actions, so we'll not start it if the task is up-to-date
                    taskToCluster.getOrDefault(task, new ArrayList<>()).forEach(ElasticsearchNode::start);
                }
                @Override
                public void afterActions(Task task) {}
            }
        );
        project.getGradle().addListener(
            new TaskExecutionListener() {
                @Override
                public void afterExecute(Task task, TaskState state) {
                    // always un-claim the cluster, even if _this_ task is up-to-date, as others might not have been and caused the
                    // cluster to start.
                    taskToCluster.getOrDefault(task, new ArrayList<>()).forEach(ElasticsearchNode::unClaimAndStop);
                }
                @Override
                public void beforeExecute(Task task) {}
            }
        );
    }

}
