/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.dataframe;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.lucene.util.SetOnce;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.IndexTemplateMetaData;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.inject.Module;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.IndexScopedSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.settings.SettingsFilter;
import org.elasticsearch.common.settings.SettingsModule;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.NodeEnvironment;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.persistent.PersistentTaskParams;
import org.elasticsearch.persistent.PersistentTaskState;
import org.elasticsearch.persistent.PersistentTasksExecutor;
import org.elasticsearch.plugins.ActionPlugin;
import org.elasticsearch.plugins.PersistentTaskPlugin;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestHandler;
import org.elasticsearch.script.ScriptService;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ExecutorBuilder;
import org.elasticsearch.threadpool.FixedExecutorBuilder;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.watcher.ResourceWatcherService;
import org.elasticsearch.xpack.core.XPackPlugin;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.core.dataframe.DataFrameField;
import org.elasticsearch.xpack.core.dataframe.transform.DataFrameTransformState;
import org.elasticsearch.xpack.core.scheduler.SchedulerEngine;
import org.elasticsearch.xpack.dataframe.action.DeleteDataFrameTransformAction;
import org.elasticsearch.xpack.dataframe.action.GetDataFrameTransformsAction;
import org.elasticsearch.xpack.dataframe.action.GetDataFrameTransformsStatsAction;
import org.elasticsearch.xpack.dataframe.action.PreviewDataFrameTransformAction;
import org.elasticsearch.xpack.dataframe.action.PutDataFrameTransformAction;
import org.elasticsearch.xpack.dataframe.action.StartDataFrameTransformAction;
import org.elasticsearch.xpack.dataframe.action.StopDataFrameTransformAction;
import org.elasticsearch.xpack.dataframe.action.TransportDeleteDataFrameTransformAction;
import org.elasticsearch.xpack.dataframe.action.TransportGetDataFrameTransformsAction;
import org.elasticsearch.xpack.dataframe.action.TransportGetDataFrameTransformsStatsAction;
import org.elasticsearch.xpack.dataframe.action.TransportPreviewDataFrameTransformAction;
import org.elasticsearch.xpack.dataframe.action.TransportPutDataFrameTransformAction;
import org.elasticsearch.xpack.dataframe.action.TransportStartDataFrameTransformAction;
import org.elasticsearch.xpack.dataframe.action.TransportStopDataFrameTransformAction;
import org.elasticsearch.xpack.dataframe.persistence.DataFrameInternalIndex;
import org.elasticsearch.xpack.dataframe.persistence.DataFrameTransformsConfigManager;
import org.elasticsearch.xpack.dataframe.rest.action.RestDeleteDataFrameTransformAction;
import org.elasticsearch.xpack.dataframe.rest.action.RestGetDataFrameTransformsAction;
import org.elasticsearch.xpack.dataframe.rest.action.RestGetDataFrameTransformsStatsAction;
import org.elasticsearch.xpack.dataframe.rest.action.RestPreviewDataFrameTransformAction;
import org.elasticsearch.xpack.dataframe.rest.action.RestPutDataFrameTransformAction;
import org.elasticsearch.xpack.dataframe.rest.action.RestStartDataFrameTransformAction;
import org.elasticsearch.xpack.dataframe.rest.action.RestStopDataFrameTransformAction;
import org.elasticsearch.xpack.dataframe.transforms.DataFrameTransform;
import org.elasticsearch.xpack.dataframe.transforms.DataFrameTransformPersistentTasksExecutor;

import java.io.IOException;
import java.time.Clock;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Supplier;
import java.util.function.UnaryOperator;

import static java.util.Collections.emptyList;

public class DataFrame extends Plugin implements ActionPlugin, PersistentTaskPlugin {

    public static final String NAME = "data_frame";
    public static final String TASK_THREAD_POOL_NAME = "data_frame_indexing";

    // list of headers that will be stored when a transform is created
    public static final Set<String> HEADER_FILTERS = new HashSet<>(
            Arrays.asList("es-security-runas-user", "_xpack_security_authentication"));

    private static final Logger logger = LogManager.getLogger(XPackPlugin.class);

    private final boolean enabled;
    private final Settings settings;
    private final boolean transportClientMode;
    private final SetOnce<DataFrameTransformsConfigManager> dataFrameTransformsConfigManager = new SetOnce<>();
    private final SetOnce<SchedulerEngine> schedulerEngine = new SetOnce<>();

    public DataFrame(Settings settings) {
        this.settings = settings;

        this.enabled = XPackSettings.DATA_FRAME_ENABLED.get(settings);
        this.transportClientMode = XPackPlugin.transportClientMode(settings);
    }

    @Override
    public Collection<Module> createGuiceModules() {
        List<Module> modules = new ArrayList<>();

        if (transportClientMode) {
            return modules;
        }

        modules.add(b -> XPackPlugin.bindFeatureSet(b, DataFrameFeatureSet.class));
        return modules;
    }

    protected XPackLicenseState getLicenseState() { return XPackPlugin.getSharedLicenseState(); }

    @Override
    public List<RestHandler> getRestHandlers(final Settings settings, final RestController restController,
            final ClusterSettings clusterSettings, final IndexScopedSettings indexScopedSettings, final SettingsFilter settingsFilter,
            final IndexNameExpressionResolver indexNameExpressionResolver, final Supplier<DiscoveryNodes> nodesInCluster) {

        if (!enabled) {
            return emptyList();
        }

        return Arrays.asList(
                new RestPutDataFrameTransformAction(settings, restController),
                new RestStartDataFrameTransformAction(settings, restController),
                new RestStopDataFrameTransformAction(settings, restController),
                new RestDeleteDataFrameTransformAction(settings, restController),
                new RestGetDataFrameTransformsAction(settings, restController),
                new RestGetDataFrameTransformsStatsAction(settings, restController),
                new RestPreviewDataFrameTransformAction(settings, restController)
        );
    }

    @Override
    public List<ActionHandler<? extends ActionRequest, ? extends ActionResponse>> getActions() {
        if (!enabled) {
            return emptyList();
        }

        return Arrays.asList(
                new ActionHandler<>(PutDataFrameTransformAction.INSTANCE, TransportPutDataFrameTransformAction.class),
                new ActionHandler<>(StartDataFrameTransformAction.INSTANCE, TransportStartDataFrameTransformAction.class),
                new ActionHandler<>(StopDataFrameTransformAction.INSTANCE, TransportStopDataFrameTransformAction.class),
                new ActionHandler<>(DeleteDataFrameTransformAction.INSTANCE, TransportDeleteDataFrameTransformAction.class),
                new ActionHandler<>(GetDataFrameTransformsAction.INSTANCE, TransportGetDataFrameTransformsAction.class),
                new ActionHandler<>(GetDataFrameTransformsStatsAction.INSTANCE, TransportGetDataFrameTransformsStatsAction.class),
                new ActionHandler<>(PreviewDataFrameTransformAction.INSTANCE, TransportPreviewDataFrameTransformAction.class)
                );
    }

    @Override
    public List<ExecutorBuilder<?>> getExecutorBuilders(Settings settings) {
        if (false == enabled || transportClientMode) {
            return emptyList();
        }

        FixedExecutorBuilder indexing = new FixedExecutorBuilder(settings, TASK_THREAD_POOL_NAME, 4, 4,
                "data_frame.task_thread_pool");

        return Collections.singletonList(indexing);
    }

    @Override
    public Collection<Object> createComponents(Client client, ClusterService clusterService, ThreadPool threadPool,
            ResourceWatcherService resourceWatcherService, ScriptService scriptService, NamedXContentRegistry xContentRegistry,
            Environment environment, NodeEnvironment nodeEnvironment, NamedWriteableRegistry namedWriteableRegistry) {
        if (enabled == false || transportClientMode) {
            return emptyList();
        }

        dataFrameTransformsConfigManager.set(new DataFrameTransformsConfigManager(client, xContentRegistry));

        return Collections.singletonList(dataFrameTransformsConfigManager.get());
    }

    @Override
    public UnaryOperator<Map<String, IndexTemplateMetaData>> getIndexTemplateMetaDataUpgrader() {
        return templates -> {
            try {
                templates.put(DataFrameInternalIndex.INDEX_TEMPLATE_NAME, DataFrameInternalIndex.getIndexTemplateMetaData());
            } catch (IOException e) {
                logger.error("Error creating data frame index template", e);
            }
            return templates;
        };
    }

    @Override
    public List<PersistentTasksExecutor<?>> getPersistentTasksExecutor(ClusterService clusterService, ThreadPool threadPool,
            Client client, SettingsModule settingsModule) {
        if (enabled == false || transportClientMode) {
            return emptyList();
        }

        schedulerEngine.set(new SchedulerEngine(settings, Clock.systemUTC()));

        // the transforms config manager should have been created
        assert dataFrameTransformsConfigManager.get() != null;
        return Collections.singletonList(new DataFrameTransformPersistentTasksExecutor(client, dataFrameTransformsConfigManager.get(),
            schedulerEngine.get(), threadPool));
    }

    @Override
    public List<NamedXContentRegistry.Entry> getNamedXContent() {
        if (enabled == false) {
            return emptyList();
        }
        return  Arrays.asList(
                new NamedXContentRegistry.Entry(PersistentTaskParams.class, new ParseField(DataFrameField.TASK_NAME),
                        DataFrameTransform::fromXContent),
                new NamedXContentRegistry.Entry(Task.Status.class, new ParseField(DataFrameTransformState.NAME),
                        DataFrameTransformState::fromXContent),
                new NamedXContentRegistry.Entry(PersistentTaskState.class, new ParseField(DataFrameTransformState.NAME),
                        DataFrameTransformState::fromXContent)
                );
    }

    @Override
    public void close() {
        if (schedulerEngine.get() != null) {
            schedulerEngine.get().stop();
        }
    }
}
