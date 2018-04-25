/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core;

import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.support.ActionFilter;
import org.elasticsearch.bootstrap.BootstrapCheck;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.IndexTemplateMetaData;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Module;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.network.NetworkService;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.IndexScopedSettings;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.settings.SettingsFilter;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.common.util.PageCacheRecycler;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.NodeEnvironment;
import org.elasticsearch.http.HttpServerTransport;
import org.elasticsearch.index.IndexModule;
import org.elasticsearch.index.analysis.TokenizerFactory;
import org.elasticsearch.indices.analysis.AnalysisModule;
import org.elasticsearch.indices.breaker.CircuitBreakerService;
import org.elasticsearch.ingest.Processor;
import org.elasticsearch.license.LicenseService;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.plugins.ActionPlugin;
import org.elasticsearch.plugins.AnalysisPlugin;
import org.elasticsearch.plugins.ClusterPlugin;
import org.elasticsearch.plugins.DiscoveryPlugin;
import org.elasticsearch.plugins.IngestPlugin;
import org.elasticsearch.plugins.MapperPlugin;
import org.elasticsearch.plugins.NetworkPlugin;
import org.elasticsearch.plugins.PersistentTaskPlugin;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.plugins.ScriptPlugin;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestHandler;
import org.elasticsearch.script.ScriptContext;
import org.elasticsearch.script.ScriptService;
import org.elasticsearch.threadpool.ExecutorBuilder;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.Transport;
import org.elasticsearch.transport.TransportInterceptor;
import org.elasticsearch.watcher.ResourceWatcherService;
import org.elasticsearch.persistent.PersistentTasksExecutor;
import org.elasticsearch.xpack.core.ssl.SSLService;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.function.BiConsumer;
import java.util.function.Function;
import java.util.function.Predicate;
import java.util.function.Supplier;
import java.util.function.UnaryOperator;
import java.util.stream.Collectors;

import static java.util.stream.Collectors.toList;

public class LocalStateCompositeXPackPlugin extends XPackPlugin implements ScriptPlugin, ActionPlugin, IngestPlugin, NetworkPlugin,
        ClusterPlugin, DiscoveryPlugin, MapperPlugin, AnalysisPlugin, PersistentTaskPlugin {

    private XPackLicenseState licenseState;
    private SSLService sslService;
    private LicenseService licenseService;
    protected List<Plugin> plugins = new ArrayList<>();

    public LocalStateCompositeXPackPlugin(final Settings settings, final Path configPath) throws Exception {
        super(settings, configPath);
    }

    //Get around all the setOnce nonsense in the plugin
    @Override
    protected SSLService getSslService() {
        return sslService;
    }

    @Override
    protected void setSslService(SSLService sslService) {
        this.sslService = sslService;
    }

    @Override
    protected LicenseService getLicenseService() {
        return licenseService;
    }

    @Override
    protected void setLicenseService(LicenseService licenseService) {
        this.licenseService = licenseService;
    }

    @Override
    protected XPackLicenseState getLicenseState() {
        return licenseState;
    }

    @Override
    protected void setLicenseState(XPackLicenseState licenseState) {
        this.licenseState = licenseState;
    }

    @Override
    public Collection<Module> createGuiceModules() {
        ArrayList<Module> modules = new ArrayList<>();
        modules.addAll(super.createGuiceModules());
        filterPlugins(Plugin.class).stream().forEach(p ->
            modules.addAll(p.createGuiceModules())
        );
        return modules;
    }

    @Override
    public Collection<Object> createComponents(Client client, ClusterService clusterService, ThreadPool threadPool,
                                               ResourceWatcherService resourceWatcherService, ScriptService scriptService,
                                               NamedXContentRegistry xContentRegistry, Environment environment,
                                               NodeEnvironment nodeEnvironment, NamedWriteableRegistry namedWriteableRegistry) {
        List<Object> components = new ArrayList<>();
        components.addAll(super.createComponents(client, clusterService, threadPool, resourceWatcherService, scriptService,
                xContentRegistry, environment, nodeEnvironment, namedWriteableRegistry));

        filterPlugins(Plugin.class).stream().forEach(p ->
            components.addAll(p.createComponents(client, clusterService, threadPool, resourceWatcherService, scriptService,
                    xContentRegistry, environment, nodeEnvironment, namedWriteableRegistry))
        );
        return components;
    }

    @Override
    public Collection<String> getRestHeaders() {
        List<String> headers = new ArrayList<>();
        headers.addAll(super.getRestHeaders());
        filterPlugins(ActionPlugin.class).stream().forEach(p -> headers.addAll(p.getRestHeaders()));
        return headers;
    }

    @Override
    public List<Setting<?>> getSettings() {
        ArrayList<Setting<?>> settings = new ArrayList<>();
        settings.addAll(super.getSettings());

        filterPlugins(Plugin.class).stream().forEach(p ->
                settings.addAll(p.getSettings())
        );
        return settings;
    }

    @Override
    public List<String> getSettingsFilter() {
        List<String> filters = new ArrayList<>();
        filters.addAll(super.getSettingsFilter());
        filterPlugins(Plugin.class).stream().forEach(p ->
            filters.addAll(p.getSettingsFilter())
        );
        return filters;
    }

    @Override
    public List<ActionHandler<? extends ActionRequest, ? extends ActionResponse>> getActions() {
        List<ActionHandler<? extends ActionRequest, ? extends ActionResponse>> actions = new ArrayList<>();
        actions.addAll(super.getActions());
        filterPlugins(ActionPlugin.class).stream().forEach(p ->
            actions.addAll(p.getActions())
        );
        return actions;
    }

    @Override
    public List<ActionFilter> getActionFilters() {
        List<ActionFilter> filters = new ArrayList<>();
        filters.addAll(super.getActionFilters());
        filterPlugins(ActionPlugin.class).stream().forEach(p ->
            filters.addAll(p.getActionFilters())
        );
        return filters;
    }

    @Override
    public List<RestHandler> getRestHandlers(Settings settings, RestController restController, ClusterSettings clusterSettings,
                                             IndexScopedSettings indexScopedSettings, SettingsFilter settingsFilter,
                                             IndexNameExpressionResolver indexNameExpressionResolver,
                                             Supplier<DiscoveryNodes> nodesInCluster) {
        List<RestHandler> handlers = new ArrayList<>();
        handlers.addAll(super.getRestHandlers(settings, restController, clusterSettings, indexScopedSettings, settingsFilter,
                indexNameExpressionResolver, nodesInCluster));
        filterPlugins(ActionPlugin.class).stream().forEach(p ->
            handlers.addAll(p.getRestHandlers(settings, restController, clusterSettings, indexScopedSettings,
                    settingsFilter, indexNameExpressionResolver, nodesInCluster))
        );
        return handlers;
    }

    @Override
    public List<NamedWriteableRegistry.Entry> getNamedWriteables() {
        List<NamedWriteableRegistry.Entry> entries = new ArrayList<>();
        entries.addAll(super.getNamedWriteables());
        for (Plugin p : plugins) {
            entries.addAll(p.getNamedWriteables());
        }
        return entries;
    }

    @Override
    public List<NamedXContentRegistry.Entry> getNamedXContent() {
        List<NamedXContentRegistry.Entry> entries = new ArrayList<>();
        entries.addAll(super.getNamedXContent());
        for (Plugin p : plugins) {
            entries.addAll(p.getNamedXContent());
        }
        return entries;
    }

    // End of the XPackPlugin overrides

    @Override
    public Settings additionalSettings() {
        Settings.Builder builder = Settings.builder();
        builder.put(super.additionalSettings());
        filterPlugins(Plugin.class).stream().forEach(p ->
                builder.put(p.additionalSettings())
        );
        return builder.build();
    }


    @Override
    public List<ScriptContext> getContexts() {
        List<ScriptContext> contexts = new ArrayList<>();
        contexts.addAll(super.getContexts());
        filterPlugins(ScriptPlugin.class).stream().forEach(p -> contexts.addAll(p.getContexts()));
        return contexts;
    }

    @Override
    public Map<String, Processor.Factory> getProcessors(Processor.Parameters parameters) {
        Map<String, Processor.Factory> processors = new HashMap<>();
        filterPlugins(IngestPlugin.class).stream().forEach(p -> processors.putAll(p.getProcessors(parameters)));
        return processors;
    }

    @Override
    public List<TransportInterceptor> getTransportInterceptors(NamedWriteableRegistry namedWriteableRegistry, ThreadContext threadContext) {
        List<TransportInterceptor> interceptors = new ArrayList<>();
        filterPlugins(NetworkPlugin.class).stream().forEach(p -> interceptors.addAll(p.getTransportInterceptors(namedWriteableRegistry,
                threadContext)));
        return interceptors;
    }

    @Override
    public Map<String, Supplier<Transport>> getTransports(Settings settings, ThreadPool threadPool, BigArrays bigArrays,
                                                          PageCacheRecycler pageCacheRecycler,
                                                          CircuitBreakerService circuitBreakerService,
                                                          NamedWriteableRegistry namedWriteableRegistry,
                                                          NetworkService networkService) {
        Map<String, Supplier<Transport>> transports = new HashMap<>();
        transports.putAll(super.getTransports(settings, threadPool, bigArrays, pageCacheRecycler,
                                              circuitBreakerService, namedWriteableRegistry, networkService));
        filterPlugins(NetworkPlugin.class).stream().forEach(p -> transports.putAll(p.getTransports(settings, threadPool, bigArrays,
                pageCacheRecycler, circuitBreakerService, namedWriteableRegistry, networkService)));
        return transports;


    }

    @Override
    public Map<String, Supplier<HttpServerTransport>> getHttpTransports(Settings settings, ThreadPool threadPool, BigArrays bigArrays,
                                                                        CircuitBreakerService circuitBreakerService,
                                                                        NamedWriteableRegistry namedWriteableRegistry,
                                                                        NamedXContentRegistry xContentRegistry,
                                                                        NetworkService networkService,
                                                                        HttpServerTransport.Dispatcher dispatcher) {
        Map<String, Supplier<HttpServerTransport>> transports = new HashMap<>();
        filterPlugins(NetworkPlugin.class).stream().forEach(p -> transports.putAll(p.getHttpTransports(settings, threadPool, bigArrays,
                circuitBreakerService, namedWriteableRegistry, xContentRegistry, networkService, dispatcher)));
        return transports;
    }

    @Override
    public List<BootstrapCheck> getBootstrapChecks() {
        List<BootstrapCheck> checks = new ArrayList<>();
        filterPlugins(Plugin.class).stream().forEach(p -> checks.addAll(p.getBootstrapChecks()));
        return Collections.unmodifiableList(checks);
    }

    @Override
    public UnaryOperator<RestHandler> getRestHandlerWrapper(ThreadContext threadContext) {

                // There can be only one.
        List<UnaryOperator<RestHandler>> items = filterPlugins(ActionPlugin.class).stream().map(p ->
                p.getRestHandlerWrapper(threadContext)).filter(Objects::nonNull).collect(Collectors.toList());

        if (items.size() > 1) {
            throw new UnsupportedOperationException("Only the security ActionPlugin should override this");
        } else if (items.size() == 1) {
            return items.get(0);
        } else {
            return null;
        }
    }

    @Override
    public List<ExecutorBuilder<?>> getExecutorBuilders(final Settings settings) {
        List<ExecutorBuilder<?>> builders = new ArrayList<>();
        filterPlugins(Plugin.class).stream().forEach(p -> builders.addAll(p.getExecutorBuilders(settings)));
        return builders;
    }
    @Override
    public UnaryOperator<Map<String, IndexTemplateMetaData>> getIndexTemplateMetaDataUpgrader() {
        return templates -> {
            for(Plugin p: plugins) {
                templates = p.getIndexTemplateMetaDataUpgrader().apply(templates);
            }
            return templates;
        };
    }

    @Override
    public Map<String, AnalysisModule.AnalysisProvider<TokenizerFactory>> getTokenizers() {
        Map<String, AnalysisModule.AnalysisProvider<TokenizerFactory>> tokenizers = new HashMap<>();
        filterPlugins(AnalysisPlugin.class).stream().forEach(p -> tokenizers.putAll(p.getTokenizers()));
        return tokenizers;
    }

    @Override
    public void onIndexModule(IndexModule indexModule) {
        super.onIndexModule(indexModule);
        filterPlugins(Plugin.class).stream().forEach(p -> p.onIndexModule(indexModule));
    }

    @Override
    public Map<String, Supplier<ClusterState.Custom>> getInitialClusterStateCustomSupplier() {
        Map<String, Supplier<ClusterState.Custom>> suppliers = new HashMap<>();
        filterPlugins(ClusterPlugin.class).stream().forEach(p -> suppliers.putAll(p.getInitialClusterStateCustomSupplier()));
        return suppliers;
    }

    @Override
    public Function<String, Predicate<String>> getFieldFilter() {
        List<Function<String, Predicate<String>>> items = filterPlugins(MapperPlugin.class).stream().map(p ->
                p.getFieldFilter()).collect(Collectors.toList());
        if (items.size() > 1) {
            throw new UnsupportedOperationException("Only the security MapperPlugin should override this");
        } else if (items.size() == 1) {
            return items.get(0);
        } else {
            // return the same default from MapperPlugin
            return MapperPlugin.NOOP_FIELD_FILTER;
        }
    }

    @Override
    public BiConsumer<DiscoveryNode, ClusterState> getJoinValidator() {
        // There can be only one.
        List<BiConsumer<DiscoveryNode, ClusterState>> items = filterPlugins(DiscoveryPlugin.class).stream().map(p ->
                p.getJoinValidator()).collect(Collectors.toList());
        if (items.size() > 1) {
            throw new UnsupportedOperationException("Only the security DiscoveryPlugin should override this");
        } else if (items.size() == 1) {
            return items.get(0);
        } else {
            return null;
        }
    }

    @Override
    public List<PersistentTasksExecutor<?>> getPersistentTasksExecutor(ClusterService clusterService,
                                                                       ThreadPool threadPool, Client client) {
        return filterPlugins(PersistentTaskPlugin.class).stream()
                .map(p -> p.getPersistentTasksExecutor(clusterService, threadPool, client))
                .flatMap(List::stream)
                .collect(toList());
    }

    private <T> List<T> filterPlugins(Class<T> type) {
        return plugins.stream().filter(x -> type.isAssignableFrom(x.getClass())).map(p -> ((T)p))
                .collect(Collectors.toList());
    }
}
