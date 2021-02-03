/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.autoscaling.capacity;

import org.elasticsearch.cluster.node.DiscoveryNodeRole;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;

import java.util.List;

/**
 * A service to decide for a specific decider.
 */
public interface AutoscalingDeciderService {

    /**
     * A marker role to use to also match policies having an empty set of roles.
     */
    DiscoveryNodeRole EMPTY_ROLES = new DiscoveryNodeRole("_empty", "_empty") {
        @Override
        public Setting<Boolean> legacySetting() {
            return null;
        }
    };

    /**
     * The name of the autoscaling decider.
     *
     * @return the name
     */
    String name();

    /**
     * Whether or not to scale based on the current state.
     *
     * @param configuration the configuration settings for a specific decider
     * @param context provides access to information about current state
     * @return result from this decider
     */
    AutoscalingDeciderResult scale(Settings configuration, AutoscalingDeciderContext context);

    List<Setting<?>> deciderSettings();

    /**
     * The roles that this decider applies to. The decider will automatically be applied to policies that has any of the roles returned,
     * using the default values for settings if not overridden on the policy.
     *
     * Returning the empty list signals a special case of a decider that require explicit configuration to be enabled for a policy and
     * has no restrictions on the roles it applies to. This is intended only for supplying deciders useful for testing.
     */
    List<DiscoveryNodeRole> roles();

    /**
     * Is the decider default on for policies matching the roles() of this decider service?
     */
    default boolean defaultOn() {
        return true;
    }
}
