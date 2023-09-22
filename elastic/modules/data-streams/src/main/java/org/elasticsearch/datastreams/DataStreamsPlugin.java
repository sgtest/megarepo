/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.datastreams;

import org.apache.lucene.util.SetOnce;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.datastreams.CreateDataStreamAction;
import org.elasticsearch.action.datastreams.DataStreamsStatsAction;
import org.elasticsearch.action.datastreams.DeleteDataStreamAction;
import org.elasticsearch.action.datastreams.GetDataStreamAction;
import org.elasticsearch.action.datastreams.MigrateToDataStreamAction;
import org.elasticsearch.action.datastreams.ModifyDataStreamsAction;
import org.elasticsearch.action.datastreams.PromoteDataStreamAction;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.client.internal.OriginSettingClient;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.routing.allocation.AllocationService;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.IndexScopedSettings;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.settings.SettingsFilter;
import org.elasticsearch.core.IOUtils;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.datastreams.action.CreateDataStreamTransportAction;
import org.elasticsearch.datastreams.action.DataStreamsStatsTransportAction;
import org.elasticsearch.datastreams.action.DeleteDataStreamTransportAction;
import org.elasticsearch.datastreams.action.GetDataStreamsTransportAction;
import org.elasticsearch.datastreams.action.MigrateToDataStreamTransportAction;
import org.elasticsearch.datastreams.action.ModifyDataStreamsTransportAction;
import org.elasticsearch.datastreams.action.PromoteDataStreamTransportAction;
import org.elasticsearch.datastreams.lifecycle.DataStreamLifecycleErrorStore;
import org.elasticsearch.datastreams.lifecycle.DataStreamLifecycleService;
import org.elasticsearch.datastreams.lifecycle.action.DeleteDataStreamLifecycleAction;
import org.elasticsearch.datastreams.lifecycle.action.ExplainDataStreamLifecycleAction;
import org.elasticsearch.datastreams.lifecycle.action.GetDataStreamLifecycleAction;
import org.elasticsearch.datastreams.lifecycle.action.PutDataStreamLifecycleAction;
import org.elasticsearch.datastreams.lifecycle.action.TransportDeleteDataStreamLifecycleAction;
import org.elasticsearch.datastreams.lifecycle.action.TransportExplainDataStreamLifecycleAction;
import org.elasticsearch.datastreams.lifecycle.action.TransportGetDataStreamLifecycleAction;
import org.elasticsearch.datastreams.lifecycle.action.TransportPutDataStreamLifecycleAction;
import org.elasticsearch.datastreams.lifecycle.rest.RestDeleteDataStreamLifecycleAction;
import org.elasticsearch.datastreams.lifecycle.rest.RestExplainDataStreamLifecycleAction;
import org.elasticsearch.datastreams.lifecycle.rest.RestGetDataStreamLifecycleAction;
import org.elasticsearch.datastreams.lifecycle.rest.RestPutDataStreamLifecycleAction;
import org.elasticsearch.datastreams.rest.RestCreateDataStreamAction;
import org.elasticsearch.datastreams.rest.RestDataStreamsStatsAction;
import org.elasticsearch.datastreams.rest.RestDeleteDataStreamAction;
import org.elasticsearch.datastreams.rest.RestGetDataStreamsAction;
import org.elasticsearch.datastreams.rest.RestMigrateToDataStreamAction;
import org.elasticsearch.datastreams.rest.RestModifyDataStreamsAction;
import org.elasticsearch.datastreams.rest.RestPromoteDataStreamAction;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.NodeEnvironment;
import org.elasticsearch.index.IndexSettingProvider;
import org.elasticsearch.indices.IndicesService;
import org.elasticsearch.plugins.ActionPlugin;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.repositories.RepositoriesService;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestHandler;
import org.elasticsearch.script.ScriptService;
import org.elasticsearch.telemetry.TelemetryProvider;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.watcher.ResourceWatcherService;
import org.elasticsearch.xcontent.NamedXContentRegistry;

import java.io.IOException;
import java.time.Clock;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Locale;
import java.util.function.Supplier;

import static org.elasticsearch.cluster.metadata.DataStreamLifecycle.DATA_STREAM_LIFECYCLE_ORIGIN;

public class DataStreamsPlugin extends Plugin implements ActionPlugin {

    public static final Setting<TimeValue> TIME_SERIES_POLL_INTERVAL = Setting.timeSetting(
        "time_series.poll_interval",
        TimeValue.timeValueMinutes(5),
        TimeValue.timeValueMinutes(1),
        TimeValue.timeValueMinutes(10),
        Setting.Property.NodeScope,
        Setting.Property.Dynamic
    );

    public static final Setting<TimeValue> LOOK_AHEAD_TIME = Setting.timeSetting(
        "index.look_ahead_time",
        TimeValue.timeValueHours(2),
        TimeValue.timeValueMinutes(1),
        TimeValue.timeValueDays(7),
        Setting.Property.IndexScope,
        Setting.Property.Dynamic,
        Setting.Property.ServerlessPublic
    );
    public static final String LIFECYCLE_CUSTOM_INDEX_METADATA_KEY = "data_stream_lifecycle";
    public static final Setting<TimeValue> LOOK_BACK_TIME = Setting.timeSetting(
        "index.look_back_time",
        TimeValue.timeValueHours(2),
        TimeValue.timeValueMinutes(1),
        TimeValue.timeValueDays(7),
        Setting.Property.IndexScope,
        Setting.Property.Dynamic,
        Setting.Property.ServerlessPublic
    );
    // The dependency of index.look_ahead_time is a cluster setting and currently there is no clean validation approach for this:
    private final SetOnce<UpdateTimeSeriesRangeService> updateTimeSeriesRangeService = new SetOnce<>();
    private final SetOnce<DataStreamLifecycleErrorStore> errorStoreInitialisationService = new SetOnce<>();

    private final SetOnce<DataStreamLifecycleService> dataLifecycleInitialisationService = new SetOnce<>();

    private final Settings settings;

    public DataStreamsPlugin(Settings settings) {
        this.settings = settings;
    }

    protected Clock getClock() {
        return Clock.systemUTC();
    }

    static void additionalLookAheadTimeValidation(TimeValue lookAhead, TimeValue timeSeriesPollInterval) {
        if (lookAhead.compareTo(timeSeriesPollInterval) < 0) {
            final String message = String.format(
                Locale.ROOT,
                "failed to parse value%s for setting [%s], must be lower than setting [%s] which is [%s]",
                " [" + lookAhead.getStringRep() + "]",
                LOOK_AHEAD_TIME.getKey(),
                TIME_SERIES_POLL_INTERVAL.getKey(),
                timeSeriesPollInterval.getStringRep()
            );
            throw new IllegalArgumentException(message);
        }
    }

    @Override
    public List<Setting<?>> getSettings() {
        List<Setting<?>> pluginSettings = new ArrayList<>();
        pluginSettings.add(TIME_SERIES_POLL_INTERVAL);
        pluginSettings.add(LOOK_AHEAD_TIME);
        pluginSettings.add(LOOK_BACK_TIME);
        pluginSettings.add(DataStreamLifecycleService.DATA_STREAM_LIFECYCLE_POLL_INTERVAL_SETTING);
        pluginSettings.add(DataStreamLifecycleService.DATA_STREAM_MERGE_POLICY_TARGET_FLOOR_SEGMENT_SETTING);
        pluginSettings.add(DataStreamLifecycleService.DATA_STREAM_MERGE_POLICY_TARGET_FACTOR_SETTING);
        return pluginSettings;
    }

    @Override
    public Collection<Object> createComponents(
        Client client,
        ClusterService clusterService,
        ThreadPool threadPool,
        ResourceWatcherService resourceWatcherService,
        ScriptService scriptService,
        NamedXContentRegistry xContentRegistry,
        Environment environment,
        NodeEnvironment nodeEnvironment,
        NamedWriteableRegistry namedWriteableRegistry,
        IndexNameExpressionResolver indexNameExpressionResolver,
        Supplier<RepositoriesService> repositoriesServiceSupplier,
        TelemetryProvider telemetryProvider,
        AllocationService allocationService,
        IndicesService indicesService
    ) {

        Collection<Object> components = new ArrayList<>();
        var updateTimeSeriesRangeService = new UpdateTimeSeriesRangeService(environment.settings(), threadPool, clusterService);
        this.updateTimeSeriesRangeService.set(updateTimeSeriesRangeService);
        components.add(this.updateTimeSeriesRangeService.get());
        errorStoreInitialisationService.set(new DataStreamLifecycleErrorStore());
        dataLifecycleInitialisationService.set(
            new DataStreamLifecycleService(
                settings,
                new OriginSettingClient(client, DATA_STREAM_LIFECYCLE_ORIGIN),
                clusterService,
                getClock(),
                threadPool,
                threadPool::absoluteTimeInMillis,
                errorStoreInitialisationService.get(),
                allocationService
            )
        );
        dataLifecycleInitialisationService.get().init();
        components.add(errorStoreInitialisationService.get());
        components.add(dataLifecycleInitialisationService.get());
        return components;
    }

    @Override
    public List<ActionHandler<? extends ActionRequest, ? extends ActionResponse>> getActions() {
        List<ActionHandler<? extends ActionRequest, ? extends ActionResponse>> actions = new ArrayList<>();
        actions.add(new ActionHandler<>(CreateDataStreamAction.INSTANCE, CreateDataStreamTransportAction.class));
        actions.add(new ActionHandler<>(DeleteDataStreamAction.INSTANCE, DeleteDataStreamTransportAction.class));
        actions.add(new ActionHandler<>(GetDataStreamAction.INSTANCE, GetDataStreamsTransportAction.class));
        actions.add(new ActionHandler<>(DataStreamsStatsAction.INSTANCE, DataStreamsStatsTransportAction.class));
        actions.add(new ActionHandler<>(MigrateToDataStreamAction.INSTANCE, MigrateToDataStreamTransportAction.class));
        actions.add(new ActionHandler<>(PromoteDataStreamAction.INSTANCE, PromoteDataStreamTransportAction.class));
        actions.add(new ActionHandler<>(ModifyDataStreamsAction.INSTANCE, ModifyDataStreamsTransportAction.class));
        actions.add(new ActionHandler<>(PutDataStreamLifecycleAction.INSTANCE, TransportPutDataStreamLifecycleAction.class));
        actions.add(new ActionHandler<>(GetDataStreamLifecycleAction.INSTANCE, TransportGetDataStreamLifecycleAction.class));
        actions.add(new ActionHandler<>(DeleteDataStreamLifecycleAction.INSTANCE, TransportDeleteDataStreamLifecycleAction.class));
        actions.add(new ActionHandler<>(ExplainDataStreamLifecycleAction.INSTANCE, TransportExplainDataStreamLifecycleAction.class));
        return actions;
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
        indexScopedSettings.addSettingsUpdateConsumer(LOOK_AHEAD_TIME, value -> {
            TimeValue timeSeriesPollInterval = updateTimeSeriesRangeService.get().pollInterval;
            additionalLookAheadTimeValidation(value, timeSeriesPollInterval);
        });
        List<RestHandler> handlers = new ArrayList<>();
        handlers.add(new RestCreateDataStreamAction());
        handlers.add(new RestDeleteDataStreamAction());
        handlers.add(new RestGetDataStreamsAction());
        handlers.add(new RestDataStreamsStatsAction());
        handlers.add(new RestMigrateToDataStreamAction());
        handlers.add(new RestPromoteDataStreamAction());
        handlers.add(new RestModifyDataStreamsAction());
        handlers.add(new RestPutDataStreamLifecycleAction());
        handlers.add(new RestGetDataStreamLifecycleAction());
        handlers.add(new RestDeleteDataStreamLifecycleAction());
        handlers.add(new RestExplainDataStreamLifecycleAction());
        return handlers;
    }

    @Override
    public Collection<IndexSettingProvider> getAdditionalIndexSettingProviders(IndexSettingProvider.Parameters parameters) {
        return List.of(new DataStreamIndexSettingsProvider(parameters.mapperServiceFactory()));
    }

    @Override
    public void close() throws IOException {
        try {
            IOUtils.close(dataLifecycleInitialisationService.get());
        } catch (IOException e) {
            throw new ElasticsearchException("unable to close the data stream lifecycle service", e);
        }
    }
}
