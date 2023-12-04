/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.application;

import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.IndexScopedSettings;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.settings.SettingsFilter;
import org.elasticsearch.indices.SystemIndexDescriptor;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.logging.LogManager;
import org.elasticsearch.logging.Logger;
import org.elasticsearch.plugins.ActionPlugin;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.plugins.SearchPlugin;
import org.elasticsearch.plugins.SystemIndexPlugin;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestHandler;
import org.elasticsearch.xpack.application.analytics.AnalyticsTemplateRegistry;
import org.elasticsearch.xpack.application.analytics.action.DeleteAnalyticsCollectionAction;
import org.elasticsearch.xpack.application.analytics.action.GetAnalyticsCollectionAction;
import org.elasticsearch.xpack.application.analytics.action.PostAnalyticsEventAction;
import org.elasticsearch.xpack.application.analytics.action.PutAnalyticsCollectionAction;
import org.elasticsearch.xpack.application.analytics.action.RestDeleteAnalyticsCollectionAction;
import org.elasticsearch.xpack.application.analytics.action.RestGetAnalyticsCollectionAction;
import org.elasticsearch.xpack.application.analytics.action.RestPostAnalyticsEventAction;
import org.elasticsearch.xpack.application.analytics.action.RestPutAnalyticsCollectionAction;
import org.elasticsearch.xpack.application.analytics.action.TransportDeleteAnalyticsCollectionAction;
import org.elasticsearch.xpack.application.analytics.action.TransportGetAnalyticsCollectionAction;
import org.elasticsearch.xpack.application.analytics.action.TransportPostAnalyticsEventAction;
import org.elasticsearch.xpack.application.analytics.action.TransportPutAnalyticsCollectionAction;
import org.elasticsearch.xpack.application.analytics.ingest.AnalyticsEventIngestConfig;
import org.elasticsearch.xpack.application.connector.ConnectorAPIFeature;
import org.elasticsearch.xpack.application.connector.ConnectorTemplateRegistry;
import org.elasticsearch.xpack.application.connector.action.DeleteConnectorAction;
import org.elasticsearch.xpack.application.connector.action.GetConnectorAction;
import org.elasticsearch.xpack.application.connector.action.ListConnectorAction;
import org.elasticsearch.xpack.application.connector.action.PutConnectorAction;
import org.elasticsearch.xpack.application.connector.action.RestDeleteConnectorAction;
import org.elasticsearch.xpack.application.connector.action.RestGetConnectorAction;
import org.elasticsearch.xpack.application.connector.action.RestListConnectorAction;
import org.elasticsearch.xpack.application.connector.action.RestPutConnectorAction;
import org.elasticsearch.xpack.application.connector.action.RestUpdateConnectorConfigurationAction;
import org.elasticsearch.xpack.application.connector.action.RestUpdateConnectorErrorAction;
import org.elasticsearch.xpack.application.connector.action.RestUpdateConnectorFilteringAction;
import org.elasticsearch.xpack.application.connector.action.RestUpdateConnectorLastSeenAction;
import org.elasticsearch.xpack.application.connector.action.RestUpdateConnectorLastSyncStatsAction;
import org.elasticsearch.xpack.application.connector.action.RestUpdateConnectorPipelineAction;
import org.elasticsearch.xpack.application.connector.action.RestUpdateConnectorSchedulingAction;
import org.elasticsearch.xpack.application.connector.action.TransportDeleteConnectorAction;
import org.elasticsearch.xpack.application.connector.action.TransportGetConnectorAction;
import org.elasticsearch.xpack.application.connector.action.TransportListConnectorAction;
import org.elasticsearch.xpack.application.connector.action.TransportPutConnectorAction;
import org.elasticsearch.xpack.application.connector.action.TransportUpdateConnectorConfigurationAction;
import org.elasticsearch.xpack.application.connector.action.TransportUpdateConnectorErrorAction;
import org.elasticsearch.xpack.application.connector.action.TransportUpdateConnectorFilteringAction;
import org.elasticsearch.xpack.application.connector.action.TransportUpdateConnectorLastSeenAction;
import org.elasticsearch.xpack.application.connector.action.TransportUpdateConnectorLastSyncStatsAction;
import org.elasticsearch.xpack.application.connector.action.TransportUpdateConnectorPipelineAction;
import org.elasticsearch.xpack.application.connector.action.TransportUpdateConnectorSchedulingAction;
import org.elasticsearch.xpack.application.connector.action.UpdateConnectorConfigurationAction;
import org.elasticsearch.xpack.application.connector.action.UpdateConnectorErrorAction;
import org.elasticsearch.xpack.application.connector.action.UpdateConnectorFilteringAction;
import org.elasticsearch.xpack.application.connector.action.UpdateConnectorLastSeenAction;
import org.elasticsearch.xpack.application.connector.action.UpdateConnectorLastSyncStatsAction;
import org.elasticsearch.xpack.application.connector.action.UpdateConnectorPipelineAction;
import org.elasticsearch.xpack.application.connector.action.UpdateConnectorSchedulingAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.CancelConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.CheckInConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.DeleteConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.GetConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.PostConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.RestCancelConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.RestCheckInConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.RestDeleteConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.RestGetConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.RestPostConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.RestUpdateConnectorSyncJobIngestionStatsAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.TransportCancelConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.TransportCheckInConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.TransportDeleteConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.TransportGetConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.TransportPostConnectorSyncJobAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.TransportUpdateConnectorSyncJobIngestionStatsAction;
import org.elasticsearch.xpack.application.connector.syncjob.action.UpdateConnectorSyncJobIngestionStatsAction;
import org.elasticsearch.xpack.application.rules.QueryRulesConfig;
import org.elasticsearch.xpack.application.rules.QueryRulesIndexService;
import org.elasticsearch.xpack.application.rules.RuleQueryBuilder;
import org.elasticsearch.xpack.application.rules.action.DeleteQueryRulesetAction;
import org.elasticsearch.xpack.application.rules.action.GetQueryRulesetAction;
import org.elasticsearch.xpack.application.rules.action.ListQueryRulesetsAction;
import org.elasticsearch.xpack.application.rules.action.PutQueryRulesetAction;
import org.elasticsearch.xpack.application.rules.action.RestDeleteQueryRulesetAction;
import org.elasticsearch.xpack.application.rules.action.RestGetQueryRulesetAction;
import org.elasticsearch.xpack.application.rules.action.RestListQueryRulesetsAction;
import org.elasticsearch.xpack.application.rules.action.RestPutQueryRulesetAction;
import org.elasticsearch.xpack.application.rules.action.TransportDeleteQueryRulesetAction;
import org.elasticsearch.xpack.application.rules.action.TransportGetQueryRulesetAction;
import org.elasticsearch.xpack.application.rules.action.TransportListQueryRulesetsAction;
import org.elasticsearch.xpack.application.rules.action.TransportPutQueryRulesetAction;
import org.elasticsearch.xpack.application.search.SearchApplicationIndexService;
import org.elasticsearch.xpack.application.search.action.DeleteSearchApplicationAction;
import org.elasticsearch.xpack.application.search.action.GetSearchApplicationAction;
import org.elasticsearch.xpack.application.search.action.ListSearchApplicationAction;
import org.elasticsearch.xpack.application.search.action.PutSearchApplicationAction;
import org.elasticsearch.xpack.application.search.action.QuerySearchApplicationAction;
import org.elasticsearch.xpack.application.search.action.RenderSearchApplicationQueryAction;
import org.elasticsearch.xpack.application.search.action.RestDeleteSearchApplicationAction;
import org.elasticsearch.xpack.application.search.action.RestGetSearchApplicationAction;
import org.elasticsearch.xpack.application.search.action.RestListSearchApplicationAction;
import org.elasticsearch.xpack.application.search.action.RestPutSearchApplicationAction;
import org.elasticsearch.xpack.application.search.action.RestQuerySearchApplicationAction;
import org.elasticsearch.xpack.application.search.action.RestRenderSearchApplicationQueryAction;
import org.elasticsearch.xpack.application.search.action.TransportDeleteSearchApplicationAction;
import org.elasticsearch.xpack.application.search.action.TransportGetSearchApplicationAction;
import org.elasticsearch.xpack.application.search.action.TransportListSearchApplicationAction;
import org.elasticsearch.xpack.application.search.action.TransportPutSearchApplicationAction;
import org.elasticsearch.xpack.application.search.action.TransportQuerySearchApplicationAction;
import org.elasticsearch.xpack.application.search.action.TransportRenderSearchApplicationQueryAction;
import org.elasticsearch.xpack.core.XPackPlugin;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.core.action.XPackInfoFeatureAction;
import org.elasticsearch.xpack.core.action.XPackUsageFeatureAction;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.function.Supplier;

import static java.util.Collections.singletonList;

public class EnterpriseSearch extends Plugin implements ActionPlugin, SystemIndexPlugin, SearchPlugin {

    public static final String APPLICATION_API_ENDPOINT = "_application";

    public static final String SEARCH_APPLICATION_API_ENDPOINT = APPLICATION_API_ENDPOINT + "/search_application";

    public static final String BEHAVIORAL_ANALYTICS_API_ENDPOINT = APPLICATION_API_ENDPOINT + "/analytics";

    public static final String QUERY_RULES_API_ENDPOINT = "_query_rules";

    public static final String CONNECTOR_API_ENDPOINT = "_connector";

    public static final String CONNECTOR_SYNC_JOB_API_ENDPOINT = CONNECTOR_API_ENDPOINT + "/_sync_job";

    private static final Logger logger = LogManager.getLogger(EnterpriseSearch.class);

    public static final String FEATURE_NAME = "ent_search";

    private final boolean enabled;

    public EnterpriseSearch(Settings settings) {
        this.enabled = XPackSettings.ENTERPRISE_SEARCH_ENABLED.get(settings);
    }

    protected XPackLicenseState getLicenseState() {
        return XPackPlugin.getSharedLicenseState();
    }

    @Override
    public List<ActionHandler<? extends ActionRequest, ? extends ActionResponse>> getActions() {
        var usageAction = new ActionHandler<>(XPackUsageFeatureAction.ENTERPRISE_SEARCH, EnterpriseSearchUsageTransportAction.class);
        var infoAction = new ActionHandler<>(XPackInfoFeatureAction.ENTERPRISE_SEARCH, EnterpriseSearchInfoTransportAction.class);
        if (enabled == false) {
            return List.of(usageAction, infoAction);
        }

        List<ActionHandler<? extends ActionRequest, ? extends ActionResponse>> actionHandlers = new ArrayList<>(
            List.of(
                // Behavioral Analytics
                new ActionHandler<>(PutAnalyticsCollectionAction.INSTANCE, TransportPutAnalyticsCollectionAction.class),
                new ActionHandler<>(GetAnalyticsCollectionAction.INSTANCE, TransportGetAnalyticsCollectionAction.class),
                new ActionHandler<>(DeleteAnalyticsCollectionAction.INSTANCE, TransportDeleteAnalyticsCollectionAction.class),
                new ActionHandler<>(PostAnalyticsEventAction.INSTANCE, TransportPostAnalyticsEventAction.class),

                // Search Applications
                new ActionHandler<>(DeleteSearchApplicationAction.INSTANCE, TransportDeleteSearchApplicationAction.class),
                new ActionHandler<>(GetSearchApplicationAction.INSTANCE, TransportGetSearchApplicationAction.class),
                new ActionHandler<>(ListSearchApplicationAction.INSTANCE, TransportListSearchApplicationAction.class),
                new ActionHandler<>(PutSearchApplicationAction.INSTANCE, TransportPutSearchApplicationAction.class),
                new ActionHandler<>(QuerySearchApplicationAction.INSTANCE, TransportQuerySearchApplicationAction.class),
                new ActionHandler<>(RenderSearchApplicationQueryAction.INSTANCE, TransportRenderSearchApplicationQueryAction.class),

                // Query rules
                new ActionHandler<>(DeleteQueryRulesetAction.INSTANCE, TransportDeleteQueryRulesetAction.class),
                new ActionHandler<>(GetQueryRulesetAction.INSTANCE, TransportGetQueryRulesetAction.class),
                new ActionHandler<>(ListQueryRulesetsAction.INSTANCE, TransportListQueryRulesetsAction.class),
                new ActionHandler<>(PutQueryRulesetAction.INSTANCE, TransportPutQueryRulesetAction.class),

                usageAction,
                infoAction
            )
        );

        // Connectors
        if (ConnectorAPIFeature.isEnabled()) {
            actionHandlers.addAll(
                List.of(
                    // Connectors API
                    new ActionHandler<>(DeleteConnectorAction.INSTANCE, TransportDeleteConnectorAction.class),
                    new ActionHandler<>(GetConnectorAction.INSTANCE, TransportGetConnectorAction.class),
                    new ActionHandler<>(ListConnectorAction.INSTANCE, TransportListConnectorAction.class),
                    new ActionHandler<>(PutConnectorAction.INSTANCE, TransportPutConnectorAction.class),
                    new ActionHandler<>(UpdateConnectorConfigurationAction.INSTANCE, TransportUpdateConnectorConfigurationAction.class),
                    new ActionHandler<>(UpdateConnectorErrorAction.INSTANCE, TransportUpdateConnectorErrorAction.class),
                    new ActionHandler<>(UpdateConnectorFilteringAction.INSTANCE, TransportUpdateConnectorFilteringAction.class),
                    new ActionHandler<>(UpdateConnectorLastSeenAction.INSTANCE, TransportUpdateConnectorLastSeenAction.class),
                    new ActionHandler<>(UpdateConnectorLastSyncStatsAction.INSTANCE, TransportUpdateConnectorLastSyncStatsAction.class),
                    new ActionHandler<>(UpdateConnectorPipelineAction.INSTANCE, TransportUpdateConnectorPipelineAction.class),
                    new ActionHandler<>(UpdateConnectorSchedulingAction.INSTANCE, TransportUpdateConnectorSchedulingAction.class),

                    // SyncJob API
                    new ActionHandler<>(GetConnectorSyncJobAction.INSTANCE, TransportGetConnectorSyncJobAction.class),
                    new ActionHandler<>(PostConnectorSyncJobAction.INSTANCE, TransportPostConnectorSyncJobAction.class),
                    new ActionHandler<>(DeleteConnectorSyncJobAction.INSTANCE, TransportDeleteConnectorSyncJobAction.class),
                    new ActionHandler<>(CheckInConnectorSyncJobAction.INSTANCE, TransportCheckInConnectorSyncJobAction.class),
                    new ActionHandler<>(CancelConnectorSyncJobAction.INSTANCE, TransportCancelConnectorSyncJobAction.class),
                    new ActionHandler<>(
                        UpdateConnectorSyncJobIngestionStatsAction.INSTANCE,
                        TransportUpdateConnectorSyncJobIngestionStatsAction.class
                    )
                )
            );
        }

        return Collections.unmodifiableList(actionHandlers);
    }

    @Override
    public List<RestHandler> getRestHandlers(
        Settings settings,
        RestController restController,
        ClusterSettings clusterSettings,
        IndexScopedSettings indexScopedSettings,
        SettingsFilter settingsFilter,
        IndexNameExpressionResolver indexNameExpressionResolver,
        Supplier<DiscoveryNodes> nodesInCluster
    ) {

        if (enabled == false) {
            return Collections.emptyList();
        }

        List<RestHandler> restHandlers = new ArrayList<>(
            List.of(
                // Behavioral Analytics
                new RestPutAnalyticsCollectionAction(getLicenseState()),
                new RestGetAnalyticsCollectionAction(getLicenseState()),
                new RestDeleteAnalyticsCollectionAction(getLicenseState()),
                new RestPostAnalyticsEventAction(getLicenseState()),

                // Search Applications
                new RestDeleteSearchApplicationAction(getLicenseState()),
                new RestGetSearchApplicationAction(getLicenseState()),
                new RestListSearchApplicationAction(getLicenseState()),
                new RestPutSearchApplicationAction(getLicenseState()),
                new RestQuerySearchApplicationAction(getLicenseState()),
                new RestRenderSearchApplicationQueryAction(getLicenseState()),

                // Query rules
                new RestDeleteQueryRulesetAction(getLicenseState()),
                new RestGetQueryRulesetAction(getLicenseState()),
                new RestListQueryRulesetsAction(getLicenseState()),
                new RestPutQueryRulesetAction(getLicenseState())
            )
        );

        // Connectors
        if (ConnectorAPIFeature.isEnabled()) {
            restHandlers.addAll(
                List.of(
                    // Connectors API
                    new RestDeleteConnectorAction(),
                    new RestGetConnectorAction(),
                    new RestListConnectorAction(),
                    new RestPutConnectorAction(),
                    new RestUpdateConnectorConfigurationAction(),
                    new RestUpdateConnectorErrorAction(),
                    new RestUpdateConnectorFilteringAction(),
                    new RestUpdateConnectorLastSeenAction(),
                    new RestUpdateConnectorLastSyncStatsAction(),
                    new RestUpdateConnectorPipelineAction(),
                    new RestUpdateConnectorSchedulingAction(),

                    // SyncJob API
                    new RestGetConnectorSyncJobAction(),
                    new RestPostConnectorSyncJobAction(),
                    new RestDeleteConnectorSyncJobAction(),
                    new RestCancelConnectorSyncJobAction(),
                    new RestCheckInConnectorSyncJobAction(),
                    new RestUpdateConnectorSyncJobIngestionStatsAction()
                )
            );
        }

        return Collections.unmodifiableList(restHandlers);
    }

    @Override
    public Collection<?> createComponents(PluginServices services) {
        if (enabled == false) {
            return Collections.emptyList();
        }

        // Behavioral analytics components
        final AnalyticsTemplateRegistry analyticsTemplateRegistry = new AnalyticsTemplateRegistry(
            services.clusterService(),
            services.featureService(),
            services.threadPool(),
            services.client(),
            services.xContentRegistry()
        );
        analyticsTemplateRegistry.initialize();

        // Connector components
        final ConnectorTemplateRegistry connectorTemplateRegistry = new ConnectorTemplateRegistry(
            services.clusterService(),
            services.featureService(),
            services.threadPool(),
            services.client(),
            services.xContentRegistry()
        );
        connectorTemplateRegistry.initialize();

        return Arrays.asList(analyticsTemplateRegistry, connectorTemplateRegistry);
    }

    @Override
    public Collection<SystemIndexDescriptor> getSystemIndexDescriptors(Settings settings) {
        return Arrays.asList(SearchApplicationIndexService.getSystemIndexDescriptor(), QueryRulesIndexService.getSystemIndexDescriptor());
    }

    @Override
    public String getFeatureName() {
        return FEATURE_NAME;
    }

    @Override
    public String getFeatureDescription() {
        return "Manages configuration for Enterprise Search features";
    }

    @Override
    public List<Setting<?>> getSettings() {
        return List.of(
            AnalyticsEventIngestConfig.MAX_NUMBER_OF_EVENTS_PER_BULK_SETTING,
            AnalyticsEventIngestConfig.FLUSH_DELAY_SETTING,
            AnalyticsEventIngestConfig.MAX_NUMBER_OF_RETRIES_SETTING,
            AnalyticsEventIngestConfig.MAX_BYTES_IN_FLIGHT_SETTING,
            QueryRulesConfig.MAX_RULE_LIMIT_SETTING
        );
    }

    @Override
    public List<QuerySpec<?>> getQueries() {
        return singletonList(
            new QuerySpec<>(RuleQueryBuilder.NAME, RuleQueryBuilder::new, p -> RuleQueryBuilder.fromXContent(p, getLicenseState()))
        );
    }
}
