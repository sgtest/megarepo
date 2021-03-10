/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.transform.action;

import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.logging.HeaderWarning;
import org.elasticsearch.common.regex.Regex;
import org.elasticsearch.persistent.PersistentTasksCustomMetadata;
import org.elasticsearch.persistent.PersistentTasksCustomMetadata.Assignment;
import org.elasticsearch.persistent.PersistentTasksCustomMetadata.PersistentTask;
import org.elasticsearch.xpack.core.transform.TransformField;
import org.elasticsearch.xpack.core.transform.TransformMessages;
import org.elasticsearch.xpack.core.transform.transforms.TransformTaskParams;
import org.elasticsearch.xpack.transform.Transform;

import java.util.Collection;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.function.Predicate;
import java.util.stream.Collectors;
import java.util.stream.StreamSupport;

public final class TransformNodes {

    private TransformNodes() {}

    /**
     * Get node assignments for a given list of transforms.
     *
     * @param transformIds The transforms.
     * @param clusterState State
     * @return The {@link TransformNodeAssignments} for the given transforms.
     */
    public static TransformNodeAssignments transformTaskNodes(List<String> transformIds, ClusterState clusterState) {
        Set<String> executorNodes = new HashSet<>();
        Set<String> assigned = new HashSet<>();
        Set<String> waitingForAssignment = new HashSet<>();

        PersistentTasksCustomMetadata tasksMetadata = PersistentTasksCustomMetadata.getPersistentTasksCustomMetadata(clusterState);

        if (tasksMetadata != null) {
            Set<String> transformIdsSet = new HashSet<>(transformIds);

            Collection<PersistentTasksCustomMetadata.PersistentTask<?>> tasks = tasksMetadata.findTasks(
                TransformField.TASK_NAME,
                t -> transformIdsSet.contains(t.getId())
            );

            for (PersistentTasksCustomMetadata.PersistentTask<?> task : tasks) {
                if (task.isAssigned()) {
                    executorNodes.add(task.getExecutorNode());
                    assigned.add(task.getId());
                } else {
                    waitingForAssignment.add(task.getId());
                }
            }
        }

        Set<String> stopped = transformIds.stream()
            .filter(id -> (assigned.contains(id) || waitingForAssignment.contains(id)) == false)
            .collect(Collectors.toSet());

        return new TransformNodeAssignments(executorNodes, assigned, waitingForAssignment, stopped);
    }

    /**
     * Get node assignments for a given transform pattern.
     *
     * Note: This only returns p-task assignments, stopped transforms are not reported. P-Tasks can be running or waiting for a node.
     *
     * @param transformId The transform or a wildcard pattern, including '_all' to match against transform tasks.
     * @param clusterState State
     * @return The {@link TransformNodeAssignments} for the given pattern.
     */
    public static TransformNodeAssignments findPersistentTasks(String transformId, ClusterState clusterState) {
        Set<String> executorNodes = new HashSet<>();
        Set<String> assigned = new HashSet<>();
        Set<String> waitingForAssignment = new HashSet<>();

        PersistentTasksCustomMetadata tasksMetadata = PersistentTasksCustomMetadata.getPersistentTasksCustomMetadata(clusterState);

        if (tasksMetadata != null) {
            Predicate<PersistentTask<?>> taskMatcher = Strings.isAllOrWildcard(new String[] { transformId }) ? t -> true : t -> {
                TransformTaskParams transformParams = (TransformTaskParams) t.getParams();
                return Regex.simpleMatch(transformId, transformParams.getId());
            };

            for (PersistentTasksCustomMetadata.PersistentTask<?> task : tasksMetadata.findTasks(TransformField.TASK_NAME, taskMatcher)) {
                if (task.isAssigned()) {
                    executorNodes.add(task.getExecutorNode());
                    assigned.add(task.getId());
                } else {
                    waitingForAssignment.add(task.getId());
                }
            }
        }
        return new TransformNodeAssignments(executorNodes, assigned, waitingForAssignment, Collections.emptySet());
    }

    /**
     * Get the assignment of a specific transform.
     *
     * @param transformId the transform id
     * @param clusterState state
     * @return {@link Assignment} of task
     */
    public static Assignment getAssignment(String transformId, ClusterState clusterState) {
        PersistentTasksCustomMetadata tasksMetadata = PersistentTasksCustomMetadata.getPersistentTasksCustomMetadata(clusterState);
        PersistentTask<?> task = tasksMetadata.getTask(transformId);

        if (task != null) {
            return task.getAssignment();
        }

        return PersistentTasksCustomMetadata.INITIAL_ASSIGNMENT;
    }

    /**
     * Get the number of transform nodes in the cluster
     *
     * @param clusterState state
     * @return number of transform nodes
     */
    public static long getNumberOfTransformNodes(ClusterState clusterState) {
        return StreamSupport.stream(clusterState.getNodes().spliterator(), false)
            .filter(node -> node.getRoles().contains(Transform.TRANSFORM_ROLE))
            .count();
    }

    /**
     * Check if cluster has at least 1 transform nodes and add a header warning if not.
     * To be used by transport actions only.
     *
     * @param clusterState state
     */
    public static void warnIfNoTransformNodes(ClusterState clusterState) {
        long transformNodes = getNumberOfTransformNodes(clusterState);
        if (transformNodes == 0) {
            HeaderWarning.addWarning(TransformMessages.REST_WARN_NO_TRANSFORM_NODES);
        }
    }
}
