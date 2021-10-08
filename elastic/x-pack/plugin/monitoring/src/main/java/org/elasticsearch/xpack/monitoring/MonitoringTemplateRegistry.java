/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.monitoring;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.Version;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.xcontent.NamedXContentRegistry;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.ClientHelper;
import org.elasticsearch.xpack.core.monitoring.MonitoredSystem;
import org.elasticsearch.xpack.core.template.IndexTemplateConfig;
import org.elasticsearch.xpack.core.template.IndexTemplateRegistry;

import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.stream.Collectors;

public class MonitoringTemplateRegistry extends IndexTemplateRegistry {
    private static final Logger logger = LogManager.getLogger(MonitoringTemplateRegistry.class);

    /**
     * The monitoring template registry version. This version number is normally incremented each change starting at "1", but
     * the legacy monitoring templates used release version numbers within their version fields instead. Because of this, we
     * continue to use the release version number in this registry, even though this is not standard practice for template
     * registries.
     */
    public static final int REGISTRY_VERSION = Version.V_7_14_0.id;
    private static final String REGISTRY_VERSION_VARIABLE = "xpack.monitoring.template.release.version";

    /**
     * Current version of templates used in their name to differentiate from breaking changes (separate from product version).
     * This would have been used for {@link MonitoringTemplateRegistry#REGISTRY_VERSION}, but the legacy monitoring
     * template installation process used the release version of the last template change in the template version
     * field instead. We keep it around to substitute into the template names.
     */
    private static final String TEMPLATE_VERSION = "7";
    private static final String TEMPLATE_VERSION_VARIABLE = "xpack.monitoring.template.version";
    private static final Map<String, String> ADDITIONAL_TEMPLATE_VARIABLES = Map.of(TEMPLATE_VERSION_VARIABLE, TEMPLATE_VERSION);

    public static final Setting<Boolean> MONITORING_TEMPLATES_ENABLED = Setting.boolSetting(
        "xpack.monitoring.templates.enabled",
        true,
        Setting.Property.NodeScope,
        Setting.Property.Dynamic
    );

    private final ClusterService clusterService;
    private volatile boolean monitoringTemplatesEnabled;

    //////////////////////////////////////////////////////////
    // Alerts template (for matching the ".monitoring-alerts-${version}" index)
    //////////////////////////////////////////////////////////
    public static final String ALERTS_INDEX_TEMPLATE_NAME = ".monitoring-alerts-7";
    public static final IndexTemplateConfig ALERTS_INDEX_TEMPLATE = new IndexTemplateConfig(
        ALERTS_INDEX_TEMPLATE_NAME,
        "/monitoring-alerts-7.json",
        REGISTRY_VERSION,
        REGISTRY_VERSION_VARIABLE,
        ADDITIONAL_TEMPLATE_VARIABLES
    );

    //////////////////////////////////////////////////////////
    // Beats template (for matching ".monitoring-beats-${version}-*" indices)
    //////////////////////////////////////////////////////////
    public static final String BEATS_INDEX_TEMPLATE_NAME = ".monitoring-beats";
    public static final IndexTemplateConfig BEATS_INDEX_TEMPLATE = new IndexTemplateConfig(
        BEATS_INDEX_TEMPLATE_NAME,
        "/monitoring-beats.json",
        REGISTRY_VERSION,
        REGISTRY_VERSION_VARIABLE,
        ADDITIONAL_TEMPLATE_VARIABLES
    );

    //////////////////////////////////////////////////////////
    // ES template (for matching ".monitoring-es-${version}-*" indices)
    //////////////////////////////////////////////////////////
    public static final String ES_INDEX_TEMPLATE_NAME = ".monitoring-es";
    public static final IndexTemplateConfig ES_INDEX_TEMPLATE = new IndexTemplateConfig(
        ES_INDEX_TEMPLATE_NAME,
        "/monitoring-es.json",
        REGISTRY_VERSION,
        REGISTRY_VERSION_VARIABLE,
        ADDITIONAL_TEMPLATE_VARIABLES
    );

    //////////////////////////////////////////////////////////
    // Kibana template (for matching ".monitoring-kibana-${version}-*" indices)
    //////////////////////////////////////////////////////////
    public static final String KIBANA_INDEX_TEMPLATE_NAME = ".monitoring-kibana";
    public static final IndexTemplateConfig KIBANA_INDEX_TEMPLATE = new IndexTemplateConfig(
        KIBANA_INDEX_TEMPLATE_NAME,
        "/monitoring-kibana.json",
        REGISTRY_VERSION,
        REGISTRY_VERSION_VARIABLE,
        ADDITIONAL_TEMPLATE_VARIABLES
    );

    //////////////////////////////////////////////////////////
    // Logstash template (for matching ".monitoring-logstash-${version}-*" indices)
    //////////////////////////////////////////////////////////
    public static final String LOGSTASH_INDEX_TEMPLATE_NAME = ".monitoring-logstash";
    public static final IndexTemplateConfig LOGSTASH_INDEX_TEMPLATE = new IndexTemplateConfig(
        LOGSTASH_INDEX_TEMPLATE_NAME,
        "/monitoring-logstash.json",
        REGISTRY_VERSION,
        REGISTRY_VERSION_VARIABLE,
        ADDITIONAL_TEMPLATE_VARIABLES
    );

    public static final String[] TEMPLATE_NAMES = new String[]{
        ALERTS_INDEX_TEMPLATE_NAME,
        BEATS_INDEX_TEMPLATE_NAME,
        ES_INDEX_TEMPLATE_NAME,
        KIBANA_INDEX_TEMPLATE_NAME,
        LOGSTASH_INDEX_TEMPLATE_NAME
    };


    private static final Map<String, IndexTemplateConfig> MONITORED_SYSTEM_CONFIG_LOOKUP = new HashMap<>();
    static {
        MONITORED_SYSTEM_CONFIG_LOOKUP.put(MonitoredSystem.BEATS.getSystem(), BEATS_INDEX_TEMPLATE);
        MONITORED_SYSTEM_CONFIG_LOOKUP.put(MonitoredSystem.ES.getSystem(), ES_INDEX_TEMPLATE);
        MONITORED_SYSTEM_CONFIG_LOOKUP.put(MonitoredSystem.KIBANA.getSystem(), KIBANA_INDEX_TEMPLATE);
        MONITORED_SYSTEM_CONFIG_LOOKUP.put(MonitoredSystem.LOGSTASH.getSystem(), LOGSTASH_INDEX_TEMPLATE);
    }

    public static IndexTemplateConfig getTemplateConfigForMonitoredSystem(MonitoredSystem system) {
        return Optional.ofNullable(MONITORED_SYSTEM_CONFIG_LOOKUP.get(system.getSystem()))
            .orElseThrow(() -> new IllegalArgumentException("Invalid system [" + system + "]"));
    }

    public MonitoringTemplateRegistry(Settings nodeSettings, ClusterService clusterService, ThreadPool threadPool, Client client,
                                      NamedXContentRegistry xContentRegistry) {
        super(nodeSettings, clusterService, threadPool, client, xContentRegistry);
        this.clusterService = clusterService;
        this.monitoringTemplatesEnabled = MONITORING_TEMPLATES_ENABLED.get(nodeSettings);
    }

    @Override
    public void initialize() {
        super.initialize();
        clusterService.getClusterSettings().addSettingsUpdateConsumer(MONITORING_TEMPLATES_ENABLED, this::updateEnabledSetting);
    }

    private void updateEnabledSetting(boolean newValue) {
        if (newValue) {
            monitoringTemplatesEnabled = true;
        } else {
            logger.info(
                "monitoring templates [{}] will not be installed or reinstalled",
                getLegacyTemplateConfigs().stream().map(IndexTemplateConfig::getTemplateName).collect(Collectors.joining(","))
            );
            monitoringTemplatesEnabled = false;
        }
    }

    @Override
    protected List<IndexTemplateConfig> getLegacyTemplateConfigs() {
        if (monitoringTemplatesEnabled) {
            return Arrays.asList(
                ALERTS_INDEX_TEMPLATE,
                BEATS_INDEX_TEMPLATE,
                ES_INDEX_TEMPLATE,
                KIBANA_INDEX_TEMPLATE,
                LOGSTASH_INDEX_TEMPLATE
            );
        } else {
            return Collections.emptyList();
        }
    }

    @Override
    protected String getOrigin() {
        return ClientHelper.MONITORING_ORIGIN;
    }

    @Override
    protected boolean requiresMasterNode() {
        // Monitoring templates have historically been installed from the master node of the cluster only.
        // Other nodes use the existence of templates as a coordination barrier in some parts of the code.
        // Templates should only be installed from the master node while we await the deprecation and
        // removal of those features so as to avoid ordering issues with exporters.
        return true;
    }
}
