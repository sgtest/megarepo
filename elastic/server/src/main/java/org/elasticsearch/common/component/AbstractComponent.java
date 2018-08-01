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

package org.elasticsearch.common.component;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.logging.DeprecationLogger;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.node.Node;

public abstract class AbstractComponent {

    protected final Logger logger;
    protected final DeprecationLogger deprecationLogger;
    protected final Settings settings;

    public AbstractComponent(Settings settings) {
        this.logger = LogManager.getLogger(getClass());
        this.deprecationLogger = new DeprecationLogger(logger);
        this.settings = settings;
    }

    /**
     * Returns the nodes name from the settings or the empty string if not set.
     */
    public final String nodeName() {
        return Node.NODE_NAME_SETTING.get(settings);
    }

    /**
     * Checks for a deprecated setting and logs the correct alternative
     */
    protected void logDeprecatedSetting(String settingName, String alternativeName) {
        if (!Strings.isNullOrEmpty(settings.get(settingName))) {
            deprecationLogger.deprecated("Setting [{}] is deprecated, use [{}] instead", settingName, alternativeName);
        }
    }

    /**
     * Checks for a removed setting and logs the correct alternative
     */
    protected void logRemovedSetting(String settingName, String alternativeName) {
        if (!Strings.isNullOrEmpty(settings.get(settingName))) {
            deprecationLogger.deprecated("Setting [{}] has been removed, use [{}] instead", settingName, alternativeName);
        }
    }

}
