/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.telemetry.apm.internal;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.SecureSetting;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.core.SuppressForbidden;
import org.elasticsearch.telemetry.apm.internal.metrics.APMMeter;
import org.elasticsearch.telemetry.apm.internal.tracing.APMTracer;

import java.security.AccessController;
import java.security.PrivilegedAction;
import java.util.List;
import java.util.Map;
import java.util.Objects;

import static org.elasticsearch.common.settings.Setting.Property.NodeScope;
import static org.elasticsearch.common.settings.Setting.Property.OperatorDynamic;

/**
 * This class is responsible for APM settings, both for Elasticsearch and the APM Java agent.
 * The methods could all be static, however they are not in order to make unit testing easier.
 */
public class APMAgentSettings {

    private static final Logger LOGGER = LogManager.getLogger(APMAgentSettings.class);

    /**
     * Sensible defaults that Elasticsearch configures. This cannot be done via the APM agent
     * config file, as then their values could not be overridden dynamically via system properties.
     */
    static Map<String, String> APM_AGENT_DEFAULT_SETTINGS = Map.of(
        "transaction_sample_rate",
        "0.2",
        "enable_experimental_instrumentations",
        "true"
    );

    public void addClusterSettingsListeners(ClusterService clusterService, APMTelemetryProvider apmTelemetryProvider) {
        final ClusterSettings clusterSettings = clusterService.getClusterSettings();
        final APMTracer apmTracer = apmTelemetryProvider.getTracer();
        final APMMeter apmMeter = apmTelemetryProvider.getMeter();

        clusterSettings.addSettingsUpdateConsumer(APM_ENABLED_SETTING, enabled -> {
            apmTracer.setEnabled(enabled);
            this.setAgentSetting("instrument", Boolean.toString(enabled));
        });
        clusterSettings.addSettingsUpdateConsumer(TELEMETRY_METRICS_ENABLED_SETTING, enabled -> {
            apmMeter.setEnabled(enabled);
            // The agent records data other than spans, e.g. JVM metrics, so we toggle this setting in order to
            // minimise its impact to a running Elasticsearch.
            this.setAgentSetting("recording", Boolean.toString(enabled));
        });
        clusterSettings.addSettingsUpdateConsumer(APM_TRACING_NAMES_INCLUDE_SETTING, apmTracer::setIncludeNames);
        clusterSettings.addSettingsUpdateConsumer(APM_TRACING_NAMES_EXCLUDE_SETTING, apmTracer::setExcludeNames);
        clusterSettings.addSettingsUpdateConsumer(APM_TRACING_SANITIZE_FIELD_NAMES, apmTracer::setLabelFilters);
        clusterSettings.addAffixMapUpdateConsumer(APM_AGENT_SETTINGS, map -> map.forEach(this::setAgentSetting), (x, y) -> {});
    }

    /**
     * Copies APM settings from the provided settings object into the corresponding system properties.
     * @param settings the settings to apply
     */
    public void syncAgentSystemProperties(Settings settings) {
        this.setAgentSetting("recording", Boolean.toString(APM_ENABLED_SETTING.get(settings)));

        // Apply default values for some system properties. Although we configure
        // the settings in APM_AGENT_DEFAULT_SETTINGS to defer to the default values, they won't
        // do anything if those settings are never configured.
        APM_AGENT_DEFAULT_SETTINGS.keySet()
            .forEach(
                key -> this.setAgentSetting(key, APM_AGENT_SETTINGS.getConcreteSetting(APM_AGENT_SETTINGS.getKey() + key).get(settings))
            );

        // Then apply values from the settings in the cluster state
        APM_AGENT_SETTINGS.getAsMap(settings).forEach(this::setAgentSetting);
    }

    /**
     * Copies a setting to the APM agent's system properties under <code>elastic.apm</code>, either
     * by setting the property if {@code value} has a value, or by deleting the property if it doesn't.
     * @param key the config key to set, without any prefix
     * @param value the value to set, or <code>null</code>
     */
    @SuppressForbidden(reason = "Need to be able to manipulate APM agent-related properties to set them dynamically")
    public void setAgentSetting(String key, String value) {
        final String completeKey = "elastic.apm." + Objects.requireNonNull(key);
        AccessController.doPrivileged((PrivilegedAction<Void>) () -> {
            if (value == null || value.isEmpty()) {
                LOGGER.trace("Clearing system property [{}]", completeKey);
                System.clearProperty(completeKey);
            } else {
                LOGGER.trace("Setting setting property [{}] to [{}]", completeKey, value);
                System.setProperty(completeKey, value);
            }
            return null;
        });
    }

    private static final String APM_SETTING_PREFIX = "tracing.apm.";

    /**
     * A list of APM agent config keys that should never be configured by the user.
     */
    private static final List<String> PROHIBITED_AGENT_KEYS = List.of(
        // ES generates a config file and sets this value
        "config_file",
        // ES controls this via `telemetry.metrics.enabled`
        "recording",
        // ES controls this via `apm.enabled`
        "instrument"
    );

    public static final Setting.AffixSetting<String> APM_AGENT_SETTINGS = Setting.prefixKeySetting(
        APM_SETTING_PREFIX + "agent.",
        (qualifiedKey) -> {
            final String[] parts = qualifiedKey.split("\\.");
            final String key = parts[parts.length - 1];
            final String defaultValue = APM_AGENT_DEFAULT_SETTINGS.getOrDefault(key, "");
            return new Setting<>(qualifiedKey, defaultValue, (value) -> {
                if (PROHIBITED_AGENT_KEYS.contains(key)) {
                    throw new IllegalArgumentException("Explicitly configuring [" + qualifiedKey + "] is prohibited");
                }
                return value;
            }, Setting.Property.NodeScope, Setting.Property.OperatorDynamic);
        }
    );

    public static final Setting<List<String>> APM_TRACING_NAMES_INCLUDE_SETTING = Setting.stringListSetting(
        APM_SETTING_PREFIX + "names.include",
        OperatorDynamic,
        NodeScope
    );

    public static final Setting<List<String>> APM_TRACING_NAMES_EXCLUDE_SETTING = Setting.stringListSetting(
        APM_SETTING_PREFIX + "names.exclude",
        OperatorDynamic,
        NodeScope
    );

    public static final Setting<List<String>> APM_TRACING_SANITIZE_FIELD_NAMES = Setting.stringListSetting(
        APM_SETTING_PREFIX + "sanitize_field_names",
        List.of(
            "password",
            "passwd",
            "pwd",
            "secret",
            "*key",
            "*token*",
            "*session*",
            "*credit*",
            "*card*",
            "*auth*",
            "*principal*",
            "set-cookie"
        ),
        OperatorDynamic,
        NodeScope
    );

    public static final Setting<Boolean> APM_ENABLED_SETTING = Setting.boolSetting(
        APM_SETTING_PREFIX + "enabled",
        false,
        OperatorDynamic,
        NodeScope
    );

    public static final Setting<Boolean> TELEMETRY_METRICS_ENABLED_SETTING = Setting.boolSetting(
        "telemetry.metrics.enabled",
        false,
        OperatorDynamic,
        NodeScope
    );

    public static final Setting<SecureString> APM_SECRET_TOKEN_SETTING = SecureSetting.secureString(
        APM_SETTING_PREFIX + "secret_token",
        null
    );

    public static final Setting<SecureString> APM_API_KEY_SETTING = SecureSetting.secureString(APM_SETTING_PREFIX + "api_key", null);
}
