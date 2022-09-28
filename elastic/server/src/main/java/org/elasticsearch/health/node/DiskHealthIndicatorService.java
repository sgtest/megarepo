/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.health.node;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodeRole;
import org.elasticsearch.cluster.routing.RoutingNodes;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.util.set.Sets;
import org.elasticsearch.health.Diagnosis;
import org.elasticsearch.health.HealthIndicatorDetails;
import org.elasticsearch.health.HealthIndicatorImpact;
import org.elasticsearch.health.HealthIndicatorResult;
import org.elasticsearch.health.HealthIndicatorService;
import org.elasticsearch.health.HealthStatus;
import org.elasticsearch.health.ImpactArea;
import org.elasticsearch.index.Index;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.stream.Collectors;
import java.util.stream.Stream;

import static org.elasticsearch.health.node.HealthIndicatorDisplayValues.are;
import static org.elasticsearch.health.node.HealthIndicatorDisplayValues.getSortedUniqueValuesString;
import static org.elasticsearch.health.node.HealthIndicatorDisplayValues.getTruncatedIndices;
import static org.elasticsearch.health.node.HealthIndicatorDisplayValues.indices;
import static org.elasticsearch.health.node.HealthIndicatorDisplayValues.regularNoun;
import static org.elasticsearch.health.node.HealthIndicatorDisplayValues.regularVerb;
import static org.elasticsearch.health.node.HealthIndicatorDisplayValues.these;

/**
 * This indicator reports the clusters' disk health aka if the cluster has enough available space to function.
 * Indicator will report YELLOW status when:
 * - a data node's disk usage is above the high watermark and it's not relocating any of its shards.
 * - a non data node's disk usage is above the high watermark.
 * Indicator will report RED status when:
 * - an index has the INDEX_READ_ONLY_ALLOW_DELETE_BLOCK which indicates that an index has been blocked because a node was out of space.
 * - any node's disk usage is above the flood stage watermark.
 */
public class DiskHealthIndicatorService implements HealthIndicatorService {
    public static final String NAME = "disk";

    private static final Logger logger = LogManager.getLogger(DiskHealthIndicatorService.class);

    private static final String IMPACT_INGEST_UNAVAILABLE_ID = "ingest_capability_unavailable";
    private static final String IMPACT_INGEST_AT_RISK_ID = "ingest_capability_at_risk";
    private static final String IMPACT_CLUSTER_STABILITY_AT_RISK_ID = "cluster_stability_at_risk";
    private static final String IMPACT_CLUSTER_FUNCTIONALITY_UNAVAILABLE_ID = "cluster_functionality_unavailable";

    private final ClusterService clusterService;

    public DiskHealthIndicatorService(ClusterService clusterService) {
        this.clusterService = clusterService;
    }

    @Override
    public String name() {
        return NAME;
    }

    @Override
    public HealthIndicatorResult calculate(boolean explain, HealthInfo healthInfo) {
        Map<String, DiskHealthInfo> diskHealthInfoMap = healthInfo.diskInfoByNode();
        if (diskHealthInfoMap == null || diskHealthInfoMap.isEmpty()) {
            /*
             * If there is no disk health info, that either means that a new health node was just elected, or something is seriously
             * wrong with health data collection on the health node. Either way, we immediately return UNKNOWN. If there are at least
             * some health info results then we work with what we have (and log any missing ones at debug level immediately below this).
             */
            return createIndicator(
                HealthStatus.UNKNOWN,
                "No disk usage data.",
                HealthIndicatorDetails.EMPTY,
                Collections.emptyList(),
                Collections.emptyList()
            );
        }
        ClusterState clusterState = clusterService.state();
        logNodesMissingHealthInfo(diskHealthInfoMap, clusterState);

        DiskHealthAnalyzer diskHealthAnalyzer = new DiskHealthAnalyzer(diskHealthInfoMap, clusterState);
        return createIndicator(
            diskHealthAnalyzer.getHealthStatus(),
            diskHealthAnalyzer.getSymptom(),
            diskHealthAnalyzer.getDetails(explain),
            diskHealthAnalyzer.getImpacts(),
            diskHealthAnalyzer.getDiagnoses()
        );
    }

    /**
     * This method logs if any nodes in the cluster state do not have health info results reported. This is logged at debug level and is
     * not ordinary important, but could be useful in tracking down problems where nodes have stopped reporting health node information.
     * @param diskHealthInfoMap A map of nodeId to DiskHealthInfo
     */
    private void logNodesMissingHealthInfo(Map<String, DiskHealthInfo> diskHealthInfoMap, ClusterState clusterState) {
        if (logger.isDebugEnabled()) {
            String nodesMissingHealthInfo = getSortedUniqueValuesString(
                clusterState.getNodes(),
                node -> diskHealthInfoMap.containsKey(node.getId()) == false,
                HealthIndicatorDisplayValues::getNodeName
            );
            if (nodesMissingHealthInfo.isBlank() == false) {
                logger.debug("The following nodes are in the cluster state but not reporting health data: [{}]", nodesMissingHealthInfo);
            }
        }
    }

    /**
     * The disk health analyzer takes into consideration the blocked indices and the health status of the all the nodes and calculates
     * the different aspects of the disk indicator such as the overall status, the symptom, the impacts and the diagnoses.
     */
    static class DiskHealthAnalyzer {

        private final ClusterState clusterState;
        private final Set<String> blockedIndices;
        private final Set<DiscoveryNode> dataNodes = new HashSet<>();
        // In this context a master node, is a master node that cannot contain data.
        private final Map<HealthStatus, Set<DiscoveryNode>> masterNodes = new HashMap<>();
        // In this context "other" nodes are nodes that cannot contain data and are not masters.
        private final Map<HealthStatus, Set<DiscoveryNode>> otherNodes = new HashMap<>();
        private final Set<DiscoveryNodeRole> affectedRoles = new HashSet<>();
        private final Set<String> indicesAtRisk;
        private final HealthStatus healthStatus;
        private final HealthIndicatorDetails details;

        DiskHealthAnalyzer(Map<String, DiskHealthInfo> diskHealthByNode, ClusterState clusterState) {
            this.clusterState = clusterState;
            blockedIndices = clusterState.blocks()
                .indices()
                .entrySet()
                .stream()
                .filter(entry -> entry.getValue().contains(IndexMetadata.INDEX_READ_ONLY_ALLOW_DELETE_BLOCK))
                .map(Map.Entry::getKey)
                .collect(Collectors.toSet());
            HealthStatus mostSevereStatusSoFar = blockedIndices.isEmpty() ? HealthStatus.GREEN : HealthStatus.RED;
            for (String nodeId : diskHealthByNode.keySet()) {
                DiscoveryNode node = clusterState.getNodes().get(nodeId);
                HealthStatus healthStatus = diskHealthByNode.get(nodeId).healthStatus();
                // TODO #90213 update this only after we check that this health status indicates a problem.
                if (mostSevereStatusSoFar.value() < healthStatus.value()) {
                    mostSevereStatusSoFar = healthStatus;
                }
                if (node == null || healthStatus.indicatesHealthProblem() == false) {
                    continue;
                }
                affectedRoles.addAll(node.getRoles());
                if (node.canContainData()) {
                    dataNodes.add(node);
                } else if (node.isMasterNode()) {
                    masterNodes.computeIfAbsent(healthStatus, ignored -> new HashSet<>()).add(node);
                } else {
                    otherNodes.computeIfAbsent(healthStatus, ignored -> new HashSet<>()).add(node);
                }
            }
            indicesAtRisk = getIndicesForNodes(dataNodes, clusterState);
            healthStatus = mostSevereStatusSoFar;
            details = createDetails(diskHealthByNode, blockedIndices);
        }

        public HealthStatus getHealthStatus() {
            return healthStatus;
        }

        String getSymptom() {
            if (healthStatus == HealthStatus.GREEN) {
                return "The cluster has enough available disk space.";
            }
            String symptom;
            if (hasBlockedIndices()) {
                symptom = String.format(
                    Locale.ROOT,
                    "%d %s %s not allowed to be updated because ",
                    blockedIndices.size(),
                    indices(blockedIndices.size()),
                    are(blockedIndices.size())
                );
                if (hasUnhealthyDataNodes()) {
                    symptom += String.format(
                        Locale.ROOT,
                        "%d %s %s out of disk or running low on disk space.",
                        dataNodes.size(),
                        regularNoun("node", dataNodes.size()),
                        are(dataNodes.size())
                    );
                } else {
                    // In this case the disk issue has been resolved but the index block has not been removed yet or the
                    // cluster is still moving shards away from data nodes that are over the high watermark.
                    symptom +=
                        ("the cluster was running out of disk space. The cluster is recovering and ingest capabilities should be restored "
                            + "within a few minutes.");
                }
                if (hasUnhealthyMasterNodes() || hasUnhealthyOtherNodes()) {
                    String roles = Stream.concat(masterNodes.values().stream(), otherNodes.values().stream())
                        .flatMap(Collection::stream)
                        .flatMap(node -> node.getRoles().stream())
                        .map(DiscoveryNodeRole::roleName)
                        .distinct()
                        .sorted()
                        .collect(Collectors.joining(", "));

                    int unhealthyNodesCount = getUnhealthyNodeSize(masterNodes) + getUnhealthyNodeSize(otherNodes);
                    symptom += String.format(
                        Locale.ROOT,
                        " Furthermore %d node%s with roles: [%s] %s out of disk or running low on disk space.",
                        unhealthyNodesCount,
                        unhealthyNodesCount == 1 ? "" : "s",
                        roles,
                        unhealthyNodesCount == 1 ? "is" : "are"
                    );
                }
            } else {
                String roles = getSortedUniqueValuesString(affectedRoles, DiscoveryNodeRole::roleName);
                int unhealthyNodesCount = dataNodes.size() + getUnhealthyNodeSize(masterNodes) + getUnhealthyNodeSize(otherNodes);
                symptom = String.format(
                    Locale.ROOT,
                    "%d %s with roles: [%s] %s out of disk or running low on disk space.",
                    unhealthyNodesCount,
                    regularNoun("node", unhealthyNodesCount),
                    roles,
                    are(unhealthyNodesCount)
                );
            }
            return symptom;
        }

        List<HealthIndicatorImpact> getImpacts() {
            if (healthStatus == HealthStatus.GREEN) {
                return List.of();
            }
            List<HealthIndicatorImpact> impacts = new ArrayList<>();
            if (hasBlockedIndices()) {
                impacts.add(
                    new HealthIndicatorImpact(
                        NAME,
                        IMPACT_INGEST_UNAVAILABLE_ID,
                        1,
                        String.format(
                            Locale.ROOT,
                            "Cannot insert or update documents in the affected indices [%s].",
                            getTruncatedIndices(blockedIndices, clusterState.getMetadata())
                        ),
                        List.of(ImpactArea.INGEST)
                    )
                );
            } else {
                if (indicesAtRisk.isEmpty() == false) {
                    impacts.add(
                        new HealthIndicatorImpact(
                            NAME,
                            IMPACT_INGEST_AT_RISK_ID,
                            1,
                            String.format(
                                Locale.ROOT,
                                "The cluster is at risk of not being able to insert or update documents in the affected indices [%s].",
                                getTruncatedIndices(indicesAtRisk, clusterState.metadata())
                            ),
                            List.of(ImpactArea.INGEST)
                        )
                    );
                }
            }
            if (affectedRoles.contains(DiscoveryNodeRole.MASTER_ROLE)) {
                impacts.add(
                    new HealthIndicatorImpact(
                        NAME,
                        IMPACT_CLUSTER_STABILITY_AT_RISK_ID,
                        1,
                        "Cluster stability might be impaired.",
                        List.of(ImpactArea.DEPLOYMENT_MANAGEMENT)
                    )
                );
            }
            String impactedOtherRoles = getSortedUniqueValuesString(
                affectedRoles,
                role -> role.canContainData() == false && role.equals(DiscoveryNodeRole.MASTER_ROLE) == false,
                DiscoveryNodeRole::roleName
            );
            if (impactedOtherRoles.isBlank() == false) {
                impacts.add(
                    new HealthIndicatorImpact(
                        NAME,
                        IMPACT_CLUSTER_FUNCTIONALITY_UNAVAILABLE_ID,
                        3,
                        String.format(Locale.ROOT, "The [%s] functionality might be impaired.", impactedOtherRoles),
                        List.of(ImpactArea.DEPLOYMENT_MANAGEMENT)
                    )
                );
            }
            return impacts;
        }

        private List<Diagnosis> getDiagnoses() {
            if (healthStatus == HealthStatus.GREEN) {
                return List.of();
            }
            List<Diagnosis> diagnosisList = new ArrayList<>();
            if (hasBlockedIndices() || hasUnhealthyDataNodes()) {
                Set<String> affectedIndices = Sets.union(blockedIndices, indicesAtRisk);
                diagnosisList.add(
                    new Diagnosis(
                        new Diagnosis.Definition(
                            NAME,
                            "add_disk_capacity_data_nodes",
                            String.format(
                                Locale.ROOT,
                                "%d %s %s on nodes that have run or are likely to run out of disk space, "
                                    + "this can temporarily disable writing on %s %s.",
                                affectedIndices.size(),
                                indices(affectedIndices.size()),
                                regularVerb("reside", affectedIndices.size()),
                                these(affectedIndices.size()),
                                indices(affectedIndices.size())
                            ),
                            "Enable autoscaling (if applicable), add disk capacity or free up disk space to resolve "
                                + "this. If you have already taken action please wait for the rebalancing to complete.",
                            "https://ela.st/fix-data-disk"
                        ),
                        dataNodes.stream().map(DiscoveryNode::getId).sorted().toList()
                    )
                );
            }
            if (masterNodes.containsKey(HealthStatus.RED)) {
                diagnosisList.add(createNonDataNodeDiagnosis(HealthStatus.RED, masterNodes.get(HealthStatus.RED), true));
            }
            if (masterNodes.containsKey(HealthStatus.YELLOW)) {
                diagnosisList.add(createNonDataNodeDiagnosis(HealthStatus.YELLOW, masterNodes.get(HealthStatus.YELLOW), true));
            }
            if (otherNodes.containsKey(HealthStatus.RED)) {
                diagnosisList.add(createNonDataNodeDiagnosis(HealthStatus.RED, otherNodes.get(HealthStatus.RED), false));
            }
            if (otherNodes.containsKey(HealthStatus.YELLOW)) {
                diagnosisList.add(createNonDataNodeDiagnosis(HealthStatus.YELLOW, otherNodes.get(HealthStatus.YELLOW), false));
            }
            return diagnosisList;
        }

        HealthIndicatorDetails getDetails(boolean explain) {
            if (explain == false) {
                return HealthIndicatorDetails.EMPTY;
            }
            return details;
        }

        private static HealthIndicatorDetails createDetails(Map<String, DiskHealthInfo> diskHealthInfoMap, Set<String> blockedIndices) {
            Map<HealthStatus, Integer> healthNodesCount = new HashMap<>();
            for (HealthStatus healthStatus : HealthStatus.values()) {
                healthNodesCount.put(healthStatus, 0);
            }
            for (DiskHealthInfo diskHealthInfo : diskHealthInfoMap.values()) {
                healthNodesCount.computeIfPresent(diskHealthInfo.healthStatus(), (key, oldCount) -> oldCount + 1);
            }
            return ((builder, params) -> {
                builder.startObject();
                builder.field("blocked_indices", blockedIndices.size());
                for (HealthStatus healthStatus : HealthStatus.values()) {
                    builder.field(healthStatus.name().toLowerCase(Locale.ROOT) + "_nodes", healthNodesCount.get(healthStatus));
                }
                return builder.endObject();
            });
        }

        private boolean hasUnhealthyDataNodes() {
            return dataNodes.isEmpty() == false;
        }

        private boolean hasUnhealthyMasterNodes() {
            return masterNodes.isEmpty() == false;
        }

        private boolean hasUnhealthyOtherNodes() {
            return otherNodes.isEmpty() == false;
        }

        private boolean hasBlockedIndices() {
            return blockedIndices.isEmpty() == false;
        }

        // Non-private for unit testing
        static Set<String> getIndicesForNodes(Set<DiscoveryNode> nodes, ClusterState clusterState) {
            RoutingNodes routingNodes = clusterState.getRoutingNodes();
            return nodes.stream()
                .map(node -> routingNodes.node(node.getId()))
                .filter(Objects::nonNull)
                .flatMap(routingNode -> Arrays.stream(routingNode.copyIndices()))
                .map(Index::getName)
                .collect(Collectors.toSet());
        }

        private Diagnosis createNonDataNodeDiagnosis(HealthStatus healthStatus, Collection<DiscoveryNode> nodes, boolean isMaster) {
            return new Diagnosis(
                new Diagnosis.Definition(
                    NAME,
                    isMaster ? "add_disk_capacity_master_nodes" : "add_disk_capacity",
                    healthStatus == HealthStatus.RED ? "Disk is full." : "The cluster is running low on disk space.",
                    "Please add capacity to the current nodes, or replace them with ones with higher capacity.",
                    isMaster ? "https://ela.st/fix-master-disk" : "https://ela.st/fix-disk-space"
                ),
                nodes.stream().map(DiscoveryNode::getId).sorted().toList()
            );
        }

        private int getUnhealthyNodeSize(Map<HealthStatus, Set<DiscoveryNode>> nodes) {
            return (nodes.containsKey(HealthStatus.RED) ? nodes.get(HealthStatus.RED).size() : 0) + (nodes.containsKey(HealthStatus.YELLOW)
                ? nodes.get(HealthStatus.YELLOW).size()
                : 0);
        }
    }
}
