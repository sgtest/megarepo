/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.rollup;

import org.elasticsearch.Version;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Module;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.IndexScopedSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.settings.SettingsFilter;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
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
import org.elasticsearch.xpack.core.rollup.RollupField;
import org.elasticsearch.xpack.core.rollup.action.DeleteRollupJobAction;
import org.elasticsearch.xpack.core.rollup.action.GetRollupCapsAction;
import org.elasticsearch.xpack.core.rollup.action.GetRollupIndexCapsAction;
import org.elasticsearch.xpack.core.rollup.action.GetRollupJobsAction;
import org.elasticsearch.xpack.core.rollup.action.PutRollupJobAction;
import org.elasticsearch.xpack.core.rollup.action.RollupSearchAction;
import org.elasticsearch.xpack.core.rollup.action.StartRollupJobAction;
import org.elasticsearch.xpack.core.rollup.action.StopRollupJobAction;
import org.elasticsearch.xpack.core.scheduler.SchedulerEngine;
import org.elasticsearch.xpack.core.template.TemplateUtils;
import org.elasticsearch.xpack.rollup.action.TransportDeleteRollupJobAction;
import org.elasticsearch.xpack.rollup.action.TransportGetRollupCapsAction;
import org.elasticsearch.xpack.rollup.action.TransportGetRollupIndexCapsAction;
import org.elasticsearch.xpack.rollup.action.TransportGetRollupJobAction;
import org.elasticsearch.xpack.rollup.action.TransportPutRollupJobAction;
import org.elasticsearch.xpack.rollup.action.TransportRollupSearchAction;
import org.elasticsearch.xpack.rollup.action.TransportStartRollupAction;
import org.elasticsearch.xpack.rollup.action.TransportStopRollupAction;
import org.elasticsearch.xpack.rollup.job.RollupJobTask;
import org.elasticsearch.xpack.rollup.rest.RestDeleteRollupJobAction;
import org.elasticsearch.xpack.rollup.rest.RestGetRollupCapsAction;
import org.elasticsearch.xpack.rollup.rest.RestGetRollupIndexCapsAction;
import org.elasticsearch.xpack.rollup.rest.RestGetRollupJobsAction;
import org.elasticsearch.xpack.rollup.rest.RestPutRollupJobAction;
import org.elasticsearch.xpack.rollup.rest.RestRollupSearchAction;
import org.elasticsearch.xpack.rollup.rest.RestStartRollupJobAction;
import org.elasticsearch.xpack.rollup.rest.RestStopRollupJobAction;

import java.time.Clock;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.function.Supplier;
import java.util.regex.Pattern;

import static java.util.Collections.emptyList;

public class Rollup extends Plugin implements ActionPlugin, PersistentTaskPlugin {

    public static final String BASE_PATH = "/_xpack/rollup/";

    // Introduced in ES version 6.3
    public static final int ROLLUP_VERSION_V1 = 1;
    // Introduced in ES Version 6.4
    // Bumped due to ID collision, see #32372
    public static final int ROLLUP_VERSION_V2 = 2;
    public static final int CURRENT_ROLLUP_VERSION = ROLLUP_VERSION_V2;

    public static final String TASK_THREAD_POOL_NAME = RollupField.NAME + "_indexing";
    public static final String SCHEDULE_THREAD_POOL_NAME = RollupField.NAME + "_scheduler";

    public static final String MAPPING_METADATA_PLACEHOLDER = "\"ROLLUP_METADATA_PLACEHOLDER\":\"ROLLUP_METADATA_PLACEHOLDER\"";
    public static final String ROLLUP_TEMPLATE_VERSION_FIELD = "rollup-version";
    public static final String ROLLUP_TEMPLATE_VERSION_PATTERN =
            Pattern.quote("${rollup.dynamic_template.version}");

    private static final String ROLLUP_TEMPLATE_NAME = "/rollup-dynamic-template.json";
    public static final String DYNAMIC_MAPPING_TEMPLATE = TemplateUtils.loadTemplate(ROLLUP_TEMPLATE_NAME,
            Version.CURRENT.toString(), Rollup.ROLLUP_TEMPLATE_VERSION_PATTERN);

    // list of headers that will be stored when a job is created
    public static final Set<String> HEADER_FILTERS =
            new HashSet<>(Arrays.asList("es-security-runas-user", "_xpack_security_authentication"));


    private final Settings settings;
    private final boolean enabled;
    private final boolean transportClientMode;

    public Rollup(Settings settings) {
        this.settings = settings;
        this.enabled = XPackSettings.ROLLUP_ENABLED.get(settings);
        this.transportClientMode = XPackPlugin.transportClientMode(settings);
    }

    @Override
    public Collection<Object> createComponents(Client client, ClusterService clusterService, ThreadPool threadPool,
                                               ResourceWatcherService resourceWatcherService, ScriptService scriptService,
                                               NamedXContentRegistry xContentRegistry, Environment environment,
                                               NodeEnvironment nodeEnvironment, NamedWriteableRegistry namedWriteableRegistry) {
        return emptyList();
    }

    @Override
    public Collection<Module> createGuiceModules() {
        List<Module> modules = new ArrayList<>();

        if (transportClientMode) {
            return modules;
        }
        modules.add(b -> XPackPlugin.bindFeatureSet(b, RollupFeatureSet.class));
        return modules;
    }

    protected XPackLicenseState getLicenseState() { return XPackPlugin.getSharedLicenseState(); }

    @Override
    public List<RestHandler> getRestHandlers(Settings settings, RestController restController, ClusterSettings clusterSettings,
                                             IndexScopedSettings indexScopedSettings, SettingsFilter settingsFilter,
                                             IndexNameExpressionResolver indexNameExpressionResolver,
                                             Supplier<DiscoveryNodes> nodesInCluster) {
        if (!enabled) {
            return emptyList();
        }

        return Arrays.asList(
            new RestRollupSearchAction(settings, restController),
            new RestPutRollupJobAction(settings, restController),
            new RestStartRollupJobAction(settings, restController),
            new RestStopRollupJobAction(settings, restController),
            new RestDeleteRollupJobAction(settings, restController),
            new RestGetRollupJobsAction(settings, restController),
            new RestGetRollupCapsAction(settings, restController),
            new RestGetRollupIndexCapsAction(settings, restController)
        );

    }

    @Override
    public List<ActionHandler<? extends ActionRequest, ? extends ActionResponse>> getActions() {
        if (!enabled) {
            return emptyList();
        }
        return Arrays.asList(
            new ActionHandler<>(RollupSearchAction.INSTANCE, TransportRollupSearchAction.class),
            new ActionHandler<>(PutRollupJobAction.INSTANCE, TransportPutRollupJobAction.class),
            new ActionHandler<>(StartRollupJobAction.INSTANCE, TransportStartRollupAction.class),
            new ActionHandler<>(StopRollupJobAction.INSTANCE, TransportStopRollupAction.class),
            new ActionHandler<>(DeleteRollupJobAction.INSTANCE, TransportDeleteRollupJobAction.class),
            new ActionHandler<>(GetRollupJobsAction.INSTANCE, TransportGetRollupJobAction.class),
            new ActionHandler<>(GetRollupCapsAction.INSTANCE, TransportGetRollupCapsAction.class),
            new ActionHandler<>(GetRollupIndexCapsAction.INSTANCE, TransportGetRollupIndexCapsAction.class)
        );
    }

    @Override
    public List<ExecutorBuilder<?>> getExecutorBuilders(Settings settings) {
        if (false == enabled || transportClientMode) {
            return emptyList();
        }

        FixedExecutorBuilder indexing = new FixedExecutorBuilder(settings, Rollup.TASK_THREAD_POOL_NAME,
                4, 4, "xpack.rollup.task_thread_pool");

        return Collections.singletonList(indexing);
    }

    @Override
    public List<PersistentTasksExecutor<?>> getPersistentTasksExecutor(ClusterService clusterService,
                                                                       ThreadPool threadPool, Client client) {
        if (enabled == false || transportClientMode ) {
            return emptyList();
        }

        SchedulerEngine schedulerEngine = new SchedulerEngine(settings, getClock());
        return Collections.singletonList(new RollupJobTask.RollupJobPersistentTasksExecutor(settings, client, schedulerEngine, threadPool));
    }

    // overridable by tests
    protected Clock getClock() {
        return Clock.systemUTC();
    }
}
