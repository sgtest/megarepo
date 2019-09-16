/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.transform;

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
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.IndexScopedSettings;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.settings.SettingsFilter;
import org.elasticsearch.common.settings.SettingsModule;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.NamedXContentRegistry.Entry;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.NodeEnvironment;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.persistent.PersistentTasksExecutor;
import org.elasticsearch.plugins.ActionPlugin;
import org.elasticsearch.plugins.PersistentTaskPlugin;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestHandler;
import org.elasticsearch.script.ScriptService;
import org.elasticsearch.threadpool.ExecutorBuilder;
import org.elasticsearch.threadpool.FixedExecutorBuilder;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.watcher.ResourceWatcherService;
import org.elasticsearch.xpack.core.XPackPlugin;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.core.action.XPackInfoFeatureAction;
import org.elasticsearch.xpack.core.action.XPackUsageFeatureAction;
import org.elasticsearch.xpack.core.scheduler.SchedulerEngine;
import org.elasticsearch.xpack.core.transform.TransformNamedXContentProvider;
import org.elasticsearch.xpack.core.transform.action.DeleteTransformAction;
import org.elasticsearch.xpack.core.transform.action.GetTransformsAction;
import org.elasticsearch.xpack.core.transform.action.GetTransformsStatsAction;
import org.elasticsearch.xpack.core.transform.action.PreviewTransformAction;
import org.elasticsearch.xpack.core.transform.action.PutTransformAction;
import org.elasticsearch.xpack.core.transform.action.StartTransformAction;
import org.elasticsearch.xpack.core.transform.action.StopTransformAction;
import org.elasticsearch.xpack.core.transform.action.UpdateTransformAction;
import org.elasticsearch.xpack.transform.action.TransportDeleteDataFrameTransformAction;
import org.elasticsearch.xpack.transform.action.TransportGetDataFrameTransformsAction;
import org.elasticsearch.xpack.transform.action.TransportGetDataFrameTransformsStatsAction;
import org.elasticsearch.xpack.transform.action.TransportPreviewDataFrameTransformAction;
import org.elasticsearch.xpack.transform.action.TransportPutDataFrameTransformAction;
import org.elasticsearch.xpack.transform.action.TransportStartDataFrameTransformAction;
import org.elasticsearch.xpack.transform.action.TransportStopDataFrameTransformAction;
import org.elasticsearch.xpack.transform.action.TransportUpdateDataFrameTransformAction;
import org.elasticsearch.xpack.transform.checkpoint.DataFrameTransformsCheckpointService;
import org.elasticsearch.xpack.transform.notifications.DataFrameAuditor;
import org.elasticsearch.xpack.transform.persistence.DataFrameInternalIndex;
import org.elasticsearch.xpack.transform.persistence.DataFrameTransformsConfigManager;
import org.elasticsearch.xpack.transform.rest.action.RestDeleteDataFrameTransformAction;
import org.elasticsearch.xpack.transform.rest.action.RestGetDataFrameTransformsAction;
import org.elasticsearch.xpack.transform.rest.action.RestGetDataFrameTransformsStatsAction;
import org.elasticsearch.xpack.transform.rest.action.RestPreviewDataFrameTransformAction;
import org.elasticsearch.xpack.transform.rest.action.RestPutDataFrameTransformAction;
import org.elasticsearch.xpack.transform.rest.action.RestStartDataFrameTransformAction;
import org.elasticsearch.xpack.transform.rest.action.RestStopDataFrameTransformAction;
import org.elasticsearch.xpack.transform.rest.action.RestUpdateDataFrameTransformAction;
import org.elasticsearch.xpack.transform.transforms.DataFrameTransformPersistentTasksExecutor;
import org.elasticsearch.xpack.transform.transforms.DataFrameTransformTask;

import java.io.IOException;
import java.time.Clock;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;
import java.util.function.UnaryOperator;

import static java.util.Collections.emptyList;

public class DataFrame extends Plugin implements ActionPlugin, PersistentTaskPlugin {

    public static final String NAME = "data_frame";
    public static final String TASK_THREAD_POOL_NAME = "data_frame_indexing";

    private static final Logger logger = LogManager.getLogger(DataFrame.class);

    private final boolean enabled;
    private final Settings settings;
    private final SetOnce<DataFrameTransformsConfigManager> dataFrameTransformsConfigManager = new SetOnce<>();
    private final SetOnce<DataFrameAuditor> dataFrameAuditor = new SetOnce<>();
    private final SetOnce<DataFrameTransformsCheckpointService> dataFrameTransformsCheckpointService = new SetOnce<>();
    private final SetOnce<SchedulerEngine> schedulerEngine = new SetOnce<>();

    public DataFrame(Settings settings) {
        this.settings = settings;
        this.enabled = XPackSettings.DATA_FRAME_ENABLED.get(settings);
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
                new RestPutDataFrameTransformAction(restController),
                new RestStartDataFrameTransformAction(restController),
                new RestStopDataFrameTransformAction(restController),
                new RestDeleteDataFrameTransformAction(restController),
                new RestGetDataFrameTransformsAction(restController),
                new RestGetDataFrameTransformsStatsAction(restController),
                new RestPreviewDataFrameTransformAction(restController),
                new RestUpdateDataFrameTransformAction(restController)
        );
    }

    @Override
    public List<ActionHandler<? extends ActionRequest, ? extends ActionResponse>> getActions() {
        var usageAction = new ActionHandler<>(XPackUsageFeatureAction.DATA_FRAME, DataFrameUsageTransportAction.class);
        var infoAction = new ActionHandler<>(XPackInfoFeatureAction.DATA_FRAME, DataFrameInfoTransportAction.class);
        if (enabled == false) {
            return Arrays.asList(usageAction, infoAction);
        }

        return Arrays.asList(
                new ActionHandler<>(PutTransformAction.INSTANCE, TransportPutDataFrameTransformAction.class),
                new ActionHandler<>(StartTransformAction.INSTANCE, TransportStartDataFrameTransformAction.class),
                new ActionHandler<>(StopTransformAction.INSTANCE, TransportStopDataFrameTransformAction.class),
                new ActionHandler<>(DeleteTransformAction.INSTANCE, TransportDeleteDataFrameTransformAction.class),
                new ActionHandler<>(GetTransformsAction.INSTANCE, TransportGetDataFrameTransformsAction.class),
                new ActionHandler<>(GetTransformsStatsAction.INSTANCE, TransportGetDataFrameTransformsStatsAction.class),
                new ActionHandler<>(PreviewTransformAction.INSTANCE, TransportPreviewDataFrameTransformAction.class),
                new ActionHandler<>(UpdateTransformAction.INSTANCE, TransportUpdateDataFrameTransformAction.class),
                usageAction,
                infoAction);
    }

    @Override
    public List<ExecutorBuilder<?>> getExecutorBuilders(Settings settings) {
        if (false == enabled) {
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
        if (enabled == false) {
            return emptyList();
        }
        dataFrameAuditor.set(new DataFrameAuditor(client, clusterService.getNodeName()));
        dataFrameTransformsConfigManager.set(new DataFrameTransformsConfigManager(client, xContentRegistry));
        dataFrameTransformsCheckpointService.set(new DataFrameTransformsCheckpointService(client,
                                                                                          dataFrameTransformsConfigManager.get(),
                                                                                          dataFrameAuditor.get()));

        return Arrays.asList(dataFrameTransformsConfigManager.get(), dataFrameAuditor.get(), dataFrameTransformsCheckpointService.get());
    }

    @Override
    public UnaryOperator<Map<String, IndexTemplateMetaData>> getIndexTemplateMetaDataUpgrader() {
        return templates -> {
            try {
                templates.put(DataFrameInternalIndex.LATEST_INDEX_VERSIONED_NAME, DataFrameInternalIndex.getIndexTemplateMetaData());
            } catch (IOException e) {
                logger.error("Error creating data frame index template", e);
            }
            try {
                templates.put(DataFrameInternalIndex.AUDIT_INDEX, DataFrameInternalIndex.getAuditIndexTemplateMetaData());
            } catch (IOException e) {
                logger.warn("Error creating data frame audit index", e);
            }
            return templates;
        };
    }

    @Override
    public List<PersistentTasksExecutor<?>> getPersistentTasksExecutor(ClusterService clusterService, ThreadPool threadPool,
            Client client, SettingsModule settingsModule) {
        if (enabled == false) {
            return emptyList();
        }

        schedulerEngine.set(new SchedulerEngine(settings, Clock.systemUTC()));

        // the transforms config manager should have been created
        assert dataFrameTransformsConfigManager.get() != null;
        // the auditor should have been created
        assert dataFrameAuditor.get() != null;
        assert dataFrameTransformsCheckpointService.get() != null;

        return Collections.singletonList(
            new DataFrameTransformPersistentTasksExecutor(client,
                dataFrameTransformsConfigManager.get(),
                dataFrameTransformsCheckpointService.get(),
                schedulerEngine.get(),
                dataFrameAuditor.get(),
                threadPool,
                clusterService,
                settingsModule.getSettings()));
    }

    @Override
    public List<Setting<?>> getSettings() {
        return Collections.singletonList(DataFrameTransformTask.NUM_FAILURE_RETRIES_SETTING);
    }

    @Override
    public void close() {
        if (schedulerEngine.get() != null) {
            schedulerEngine.get().stop();
        }
    }

    @Override
    public List<Entry> getNamedXContent() {
        return new TransformNamedXContentProvider().getNamedXContentParsers();
    }
}
