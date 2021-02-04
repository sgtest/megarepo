/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.security.operator;

import org.elasticsearch.action.admin.cluster.configuration.AddVotingConfigExclusionsAction;
import org.elasticsearch.action.admin.cluster.configuration.ClearVotingConfigExclusionsAction;
import org.elasticsearch.action.admin.cluster.settings.ClusterUpdateSettingsAction;
import org.elasticsearch.action.admin.cluster.settings.ClusterUpdateSettingsRequest;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.license.DeleteLicenseAction;
import org.elasticsearch.license.PutLicenseAction;
import org.elasticsearch.transport.TransportRequest;

import java.util.List;
import java.util.Set;
import java.util.stream.Collectors;
import java.util.stream.Stream;

public class OperatorOnlyRegistry {

    public static final Set<String> SIMPLE_ACTIONS = Set.of(AddVotingConfigExclusionsAction.NAME,
        ClearVotingConfigExclusionsAction.NAME,
        PutLicenseAction.NAME,
        DeleteLicenseAction.NAME,
        // Autoscaling does not publish its actions to core, literal strings are needed.
        "cluster:admin/autoscaling/put_autoscaling_policy",
        "cluster:admin/autoscaling/delete_autoscaling_policy",
        "cluster:admin/autoscaling/get_autoscaling_policy",
        "cluster:admin/autoscaling/get_autoscaling_capacity");

    private final ClusterSettings clusterSettings;

    public OperatorOnlyRegistry(ClusterSettings clusterSettings) {
        this.clusterSettings = clusterSettings;
    }

    /**
     * Check whether the given action and request qualify as operator-only. The method returns
     * null if the action+request is NOT operator-only. Other it returns a violation object
     * that contains the message for details.
     */
    public OperatorPrivilegesViolation check(String action, TransportRequest request) {
        if (SIMPLE_ACTIONS.contains(action)) {
            return () -> "action [" + action + "]";
        } else if (ClusterUpdateSettingsAction.NAME.equals(action)) {
            assert request instanceof ClusterUpdateSettingsRequest;
            return checkClusterUpdateSettings((ClusterUpdateSettingsRequest) request);
        } else {
            return null;
        }
    }

    private OperatorPrivilegesViolation checkClusterUpdateSettings(ClusterUpdateSettingsRequest request) {
        List<String> operatorOnlySettingKeys = Stream.concat(
            request.transientSettings().keySet().stream(), request.persistentSettings().keySet().stream()
        ).filter(k -> {
            final Setting<?> setting = clusterSettings.get(k);
            return setting != null && setting.isOperatorOnly();
        }).collect(Collectors.toList());
        if (false == operatorOnlySettingKeys.isEmpty()) {
            return () -> (operatorOnlySettingKeys.size() == 1 ? "setting" : "settings")
                + " [" + Strings.collectionToDelimitedString(operatorOnlySettingKeys, ",") + "]";
        } else {
            return null;
        }
    }

    @FunctionalInterface
    public interface OperatorPrivilegesViolation {
        String message();
    }
}
