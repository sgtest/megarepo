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

package org.elasticsearch.action.support;

import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Setting.Property;
import org.elasticsearch.common.settings.Settings;

import java.util.Arrays;

/**
 * Helper for dealing with destructive operations and wildcard usage.
 */
public final class DestructiveOperations {

    /**
     * Setting which controls whether wildcard usage (*, prefix*, _all) is allowed.
     */
    public static final Setting<Boolean> REQUIRES_NAME_SETTING =
        Setting.boolSetting("action.destructive_requires_name", false, Property.Dynamic, Property.NodeScope);
    /**
     * The "match none" pattern, "*,-*", will never actually be destructive
     * because it operates on no indices. If plugins or other components add
     * their own index resolution layers, they can pass this pattern to the
     * core code in order to indicate that an operation won't run on any
     * indices, relying on the core code to handle this situation.
     */
    private static final String[] MATCH_NONE_PATTERN = {"*", "-*"};

    private volatile boolean destructiveRequiresName;

    public DestructiveOperations(Settings settings, ClusterSettings clusterSettings) {
        destructiveRequiresName = REQUIRES_NAME_SETTING.get(settings);
        clusterSettings.addSettingsUpdateConsumer(REQUIRES_NAME_SETTING, this::setDestructiveRequiresName);
    }

    private void setDestructiveRequiresName(boolean destructiveRequiresName) {
        this.destructiveRequiresName = destructiveRequiresName;
    }

    /**
     * Fail if there is wildcard usage in indices and the named is required for destructive operations.
     */
    public void failDestructive(String[] aliasesOrIndices) {
        if (destructiveRequiresName == false) {
            return;
        }

        if (aliasesOrIndices == null || aliasesOrIndices.length == 0) {
            throw new IllegalArgumentException("Wildcard expressions or all indices are not allowed");
        } else if (aliasesOrIndices.length == 1) {
            if (hasWildcardUsage(aliasesOrIndices[0])) {
                throw new IllegalArgumentException("Wildcard expressions or all indices are not allowed");
            }
        } else if (Arrays.equals(aliasesOrIndices, MATCH_NONE_PATTERN) == false) {
            for (String aliasesOrIndex : aliasesOrIndices) {
                if (hasWildcardUsage(aliasesOrIndex)) {
                    throw new IllegalArgumentException("Wildcard expressions or all indices are not allowed");
                }
            }
        }
    }

    private static boolean hasWildcardUsage(String aliasOrIndex) {
        return "_all".equals(aliasOrIndex) || aliasOrIndex.indexOf('*') != -1;
    }

}
