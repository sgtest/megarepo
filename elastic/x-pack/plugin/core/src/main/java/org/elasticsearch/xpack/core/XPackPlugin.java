/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.core;

import org.apache.lucene.util.SetOnce;
import org.elasticsearch.SpecialPermission;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.ActionType;
import org.elasticsearch.action.support.ActionFilter;
import org.elasticsearch.action.support.TransportAction;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodeRole;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.routing.allocation.decider.AllocationDecider;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Booleans;
import org.elasticsearch.common.inject.Binder;
import org.elasticsearch.common.inject.multibindings.Multibinder;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.logging.DeprecationCategory;
import org.elasticsearch.common.logging.DeprecationLogger;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.IndexScopedSettings;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.settings.SettingsFilter;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.NodeEnvironment;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.engine.EngineFactory;
import org.elasticsearch.index.shard.IndexSettingProvider;
import org.elasticsearch.indices.recovery.RecoverySettings;
import org.elasticsearch.license.LicenseService;
import org.elasticsearch.license.LicensesMetadata;
import org.elasticsearch.license.Licensing;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.plugins.ClusterPlugin;
import org.elasticsearch.plugins.EnginePlugin;
import org.elasticsearch.plugins.ExtensiblePlugin;
import org.elasticsearch.plugins.RepositoryPlugin;
import org.elasticsearch.protocol.xpack.XPackInfoRequest;
import org.elasticsearch.protocol.xpack.XPackInfoResponse;
import org.elasticsearch.protocol.xpack.XPackUsageRequest;
import org.elasticsearch.repositories.RepositoriesService;
import org.elasticsearch.repositories.Repository;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestHandler;
import org.elasticsearch.script.ScriptService;
import org.elasticsearch.snapshots.SourceOnlySnapshotRepository;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.watcher.ResourceWatcherService;
import org.elasticsearch.xpack.cluster.routing.allocation.DataTierAllocationDecider;
import org.elasticsearch.xpack.core.action.ReloadAnalyzerAction;
import org.elasticsearch.xpack.core.action.TransportReloadAnalyzersAction;
import org.elasticsearch.xpack.core.action.TransportXPackInfoAction;
import org.elasticsearch.xpack.core.action.TransportXPackUsageAction;
import org.elasticsearch.xpack.core.action.XPackInfoAction;
import org.elasticsearch.xpack.core.action.XPackInfoFeatureAction;
import org.elasticsearch.xpack.core.action.XPackUsageAction;
import org.elasticsearch.xpack.core.action.XPackUsageFeatureAction;
import org.elasticsearch.xpack.core.action.XPackUsageResponse;
import org.elasticsearch.xpack.core.async.DeleteAsyncResultAction;
import org.elasticsearch.xpack.core.async.TransportDeleteAsyncResultAction;
import org.elasticsearch.xpack.core.ml.MlMetadata;
import org.elasticsearch.xpack.core.rest.action.RestReloadAnalyzersAction;
import org.elasticsearch.xpack.core.rest.action.RestXPackInfoAction;
import org.elasticsearch.xpack.core.rest.action.RestXPackUsageAction;
import org.elasticsearch.xpack.core.search.action.ClosePointInTimeAction;
import org.elasticsearch.xpack.core.search.action.OpenPointInTimeAction;
import org.elasticsearch.xpack.core.search.action.RestClosePointInTimeAction;
import org.elasticsearch.xpack.core.search.action.RestOpenPointInTimeAction;
import org.elasticsearch.xpack.core.search.action.TransportClosePointInTimeAction;
import org.elasticsearch.xpack.core.search.action.TransportOpenPointInTimeAction;
import org.elasticsearch.xpack.core.security.authc.TokenMetadata;
import org.elasticsearch.xpack.core.ssl.SSLConfiguration;
import org.elasticsearch.xpack.core.ssl.SSLConfigurationReloader;
import org.elasticsearch.xpack.core.ssl.SSLService;
import org.elasticsearch.xpack.core.watcher.WatcherMetadata;
import org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshotsConstants;

import java.nio.file.Files;
import java.nio.file.Path;
import java.security.AccessController;
import java.security.PrivilegedAction;
import java.time.Clock;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.function.LongSupplier;
import java.util.function.Supplier;
import java.util.stream.Collectors;
import java.util.stream.StreamSupport;

public class XPackPlugin extends XPackClientPlugin implements ExtensiblePlugin, RepositoryPlugin, EnginePlugin, ClusterPlugin {
    private static final DeprecationLogger deprecationLogger = DeprecationLogger.getLogger(XPackPlugin.class);

    public static final String ASYNC_RESULTS_INDEX = ".async-search";
    public static final String XPACK_INSTALLED_NODE_ATTR = "xpack.installed";

    // TODO: clean up this library to not ask for write access to all system properties!
    static {
        // invoke this clinit in unbound with permissions to access all system properties
        SecurityManager sm = System.getSecurityManager();
        if (sm != null) {
            sm.checkPermission(new SpecialPermission());
        }
        try {
            AccessController.doPrivileged(new PrivilegedAction<Void>() {
                @Override
                public Void run() {
                    try {
                        Class.forName("com.unboundid.util.Debug");
                        Class.forName("com.unboundid.ldap.sdk.LDAPConnectionOptions");
                    } catch (ClassNotFoundException e) {
                        throw new RuntimeException(e);
                    }
                    return null;
                }
            });
            // TODO: fix gradle to add all security resources (plugin metadata) to test classpath
            // of watcher plugin, which depends on it directly. This prevents these plugins
            // from being initialized correctly by the test framework, and means we have to
            // have this leniency.
        } catch (ExceptionInInitializerError bogus) {
            if (bogus.getCause() instanceof SecurityException == false) {
                throw bogus; // some other bug
            }
        }
    }

    protected final Settings settings;
    //private final Environment env;
    protected final Licensing licensing;
    // These should not be directly accessed as they cannot be overridden in tests. Please use the getters so they can be overridden.
    private static final SetOnce<XPackLicenseState> licenseState = new SetOnce<>();
    private static final SetOnce<SSLService> sslService = new SetOnce<>();
    private static final SetOnce<LicenseService> licenseService = new SetOnce<>();
    private static final SetOnce<LongSupplier> epochMillisSupplier = new SetOnce<>();

    public XPackPlugin(
            final Settings settings,
            final Path configPath) {
        super(settings);
        // FIXME: The settings might be changed after this (e.g. from "additionalSettings" method in other plugins)
        // We should only depend on the settings from the Environment object passed to createComponents
        this.settings = settings;

        setLicenseState(new XPackLicenseState(settings, () -> getEpochMillisSupplier().getAsLong()));

        this.licensing = new Licensing(settings);
    }

    // overridable by tests
    protected Clock getClock() {
        return Clock.systemUTC();
    }

    protected SSLService getSslService() { return getSharedSslService(); }
    protected LicenseService getLicenseService() { return getSharedLicenseService(); }
    protected XPackLicenseState getLicenseState() { return getSharedLicenseState(); }
    protected LongSupplier getEpochMillisSupplier() { return getSharedEpochMillisSupplier(); }
    protected void setSslService(SSLService sslService) { XPackPlugin.sslService.set(sslService); }
    protected void setLicenseService(LicenseService licenseService) { XPackPlugin.licenseService.set(licenseService); }
    protected void setLicenseState(XPackLicenseState licenseState) { XPackPlugin.licenseState.set(licenseState); }
    protected void setEpochMillisSupplier(LongSupplier epochMillisSupplier) {
        XPackPlugin.epochMillisSupplier.set(epochMillisSupplier);
    }

    public static SSLService getSharedSslService() {
        final SSLService ssl = XPackPlugin.sslService.get();
        if (ssl == null) {
            throw new IllegalStateException("SSL Service is not constructed yet");
        }
        return ssl;
    }
    public static LicenseService getSharedLicenseService() { return licenseService.get(); }
    public static XPackLicenseState getSharedLicenseState() { return licenseState.get(); }
    public static LongSupplier getSharedEpochMillisSupplier() { return epochMillisSupplier.get(); }

    /**
     * Checks if the cluster state allows this node to add x-pack metadata to the cluster state,
     * and throws an exception otherwise.
     * This check should be called before installing any x-pack metadata to the cluster state,
     * to ensure that the other nodes that are part of the cluster will be able to deserialize
     * that metadata. Note that if the cluster state already contains x-pack metadata, this
     * check assumes that the nodes are already ready to receive additional x-pack metadata.
     * Having this check properly in place everywhere allows to install x-pack into a cluster
     * using a rolling restart.
     */
    public static void checkReadyForXPackCustomMetadata(ClusterState clusterState) {
        if (alreadyContainsXPackCustomMetadata(clusterState)) {
            return;
        }
        List<DiscoveryNode> notReadyNodes = nodesNotReadyForXPackCustomMetadata(clusterState);
        if (notReadyNodes.isEmpty() == false) {
            throw new IllegalStateException("The following nodes are not ready yet for enabling x-pack custom metadata: " + notReadyNodes);
        }
    }

    /**
     * Checks if the cluster state allows this node to add x-pack metadata to the cluster state.
     * See {@link #checkReadyForXPackCustomMetadata} for more details.
     */
    public static boolean isReadyForXPackCustomMetadata(ClusterState clusterState) {
        return alreadyContainsXPackCustomMetadata(clusterState) || nodesNotReadyForXPackCustomMetadata(clusterState).isEmpty();
    }

    /**
     * Returns the list of nodes that won't allow this node from adding x-pack metadata to the cluster state.
     * See {@link #checkReadyForXPackCustomMetadata} for more details.
     */
    public static List<DiscoveryNode> nodesNotReadyForXPackCustomMetadata(ClusterState clusterState) {
        // check that all nodes would be capable of deserializing newly added x-pack metadata
        final List<DiscoveryNode> notReadyNodes = StreamSupport.stream(clusterState.nodes().spliterator(), false).filter(node -> {
            final String xpackInstalledAttr = node.getAttributes().getOrDefault(XPACK_INSTALLED_NODE_ATTR, "false");
            return Booleans.parseBoolean(xpackInstalledAttr) == false;
        }).collect(Collectors.toList());

        return notReadyNodes;
    }

    private static boolean alreadyContainsXPackCustomMetadata(ClusterState clusterState) {
        final Metadata metadata = clusterState.metadata();
        return metadata.custom(LicensesMetadata.TYPE) != null ||
            metadata.custom(MlMetadata.TYPE) != null ||
            metadata.custom(WatcherMetadata.TYPE) != null ||
            clusterState.custom(TokenMetadata.TYPE) != null;
    }

    @Override
    public Settings additionalSettings() {
        final String xpackInstalledNodeAttrSetting = "node.attr." + XPACK_INSTALLED_NODE_ATTR;

        if (settings.get(xpackInstalledNodeAttrSetting) != null) {
            throw new IllegalArgumentException("Directly setting [" + xpackInstalledNodeAttrSetting + "] is not permitted");
        }
        return Settings.builder().put(super.additionalSettings()).put(xpackInstalledNodeAttrSetting, "true").build();
    }

    @Override
    public Collection<Object> createComponents(Client client, ClusterService clusterService, ThreadPool threadPool,
                                               ResourceWatcherService resourceWatcherService, ScriptService scriptService,
                                               NamedXContentRegistry xContentRegistry, Environment environment,
                                               NodeEnvironment nodeEnvironment, NamedWriteableRegistry namedWriteableRegistry,
                                               IndexNameExpressionResolver expressionResolver,
                                               Supplier<RepositoriesService> repositoriesServiceSupplier) {
        List<Object> components = new ArrayList<>();

        final SSLService sslService = createSSLService(environment, resourceWatcherService);
        setLicenseService(new LicenseService(settings, clusterService, getClock(),
                environment, resourceWatcherService, getLicenseState()));

        setEpochMillisSupplier(threadPool::absoluteTimeInMillis);

        // It is useful to override these as they are what guice is injecting into actions
        components.add(sslService);
        components.add(getLicenseService());
        components.add(getLicenseState());

        return components;
    }

    @Override
    public List<ActionHandler<? extends ActionRequest, ? extends ActionResponse>> getActions() {
        List<ActionHandler<? extends ActionRequest, ? extends ActionResponse>> actions = new ArrayList<>();
        actions.add(new ActionHandler<>(XPackInfoAction.INSTANCE, getInfoAction()));
        actions.add(new ActionHandler<>(XPackUsageAction.INSTANCE, getUsageAction()));
        actions.addAll(licensing.getActions());
        actions.add(new ActionHandler<>(ReloadAnalyzerAction.INSTANCE, TransportReloadAnalyzersAction.class));
        actions.add(new ActionHandler<>(DeleteAsyncResultAction.INSTANCE, TransportDeleteAsyncResultAction.class));
        actions.add(new ActionHandler<>(OpenPointInTimeAction.INSTANCE, TransportOpenPointInTimeAction.class));
        actions.add(new ActionHandler<>(ClosePointInTimeAction.INSTANCE, TransportClosePointInTimeAction.class));
        actions.add(new ActionHandler<>(XPackInfoFeatureAction.DATA_TIERS, DataTiersInfoTransportAction.class));
        actions.add(new ActionHandler<>(XPackUsageFeatureAction.DATA_TIERS, DataTiersUsageTransportAction.class));
        return actions;
    }

    // overridable for tests
    protected Class<? extends TransportAction<XPackUsageRequest, XPackUsageResponse>> getUsageAction() {
        return TransportXPackUsageAction.class;
    }

    // overridable for tests
    protected Class<? extends TransportAction<XPackInfoRequest, XPackInfoResponse>> getInfoAction() {
        return TransportXPackInfoAction.class;
    }

    @Override
    public List<ActionType<? extends ActionResponse>> getClientActions() {
        List<ActionType<? extends ActionResponse>> actions = new ArrayList<>();
        actions.addAll(licensing.getClientActions());
        actions.addAll(super.getClientActions());
        return actions;
    }

    @Override
    public List<ActionFilter> getActionFilters() {
        List<ActionFilter> filters = new ArrayList<>();
        filters.addAll(licensing.getActionFilters());
        return filters;
    }

    @Override
    public List<RestHandler> getRestHandlers(Settings settings, RestController restController, ClusterSettings clusterSettings,
            IndexScopedSettings indexScopedSettings, SettingsFilter settingsFilter, IndexNameExpressionResolver indexNameExpressionResolver,
            Supplier<DiscoveryNodes> nodesInCluster) {
        List<RestHandler> handlers = new ArrayList<>();
        handlers.add(new RestXPackInfoAction());
        handlers.add(new RestXPackUsageAction());
        handlers.add(new RestReloadAnalyzersAction());
        handlers.addAll(licensing.getRestHandlers(settings, restController, clusterSettings, indexScopedSettings, settingsFilter,
                indexNameExpressionResolver, nodesInCluster));
        handlers.add(new RestOpenPointInTimeAction());
        handlers.add(new RestClosePointInTimeAction());
        return handlers;
    }

    public static void bindFeatureSet(Binder binder, Class<? extends XPackFeatureSet> featureSet) {
        Multibinder<XPackFeatureSet> featureSetBinder = createFeatureSetMultiBinder(binder, featureSet);
        featureSetBinder.addBinding().to(featureSet);
    }

    public static Multibinder<XPackFeatureSet> createFeatureSetMultiBinder(Binder binder, Class<? extends XPackFeatureSet> featureSet) {
        binder.bind(featureSet).asEagerSingleton();
        return Multibinder.newSetBinder(binder, XPackFeatureSet.class);
    }

    public static Path resolveConfigFile(Environment env, String name) {
        Path config =  env.configFile().resolve(name);
        if (Files.exists(config) == false) {
            Path legacyConfig = env.configFile().resolve("x-pack").resolve(name);
            if (Files.exists(legacyConfig)) {
                deprecationLogger.deprecate(DeprecationCategory.OTHER, "config_file_path",
                    "Config file [" + name + "] is in a deprecated location. Move from " +
                    legacyConfig.toString() + " to " + config.toString());
                return legacyConfig;
            }
        }
        return config;
    }

    @Override
    public Map<String, Repository.Factory> getRepositories(Environment env, NamedXContentRegistry namedXContentRegistry,
                                                           ClusterService clusterService, BigArrays bigArrays,
                                                           RecoverySettings recoverySettings) {
        return Collections.singletonMap("source", SourceOnlySnapshotRepository.newRepositoryFactory());
    }

    @Override
    public Optional<EngineFactory> getEngineFactory(IndexSettings indexSettings) {
        if (indexSettings.getValue(SourceOnlySnapshotRepository.SOURCE_ONLY) &&
            SearchableSnapshotsConstants.isSearchableSnapshotStore(indexSettings.getSettings()) == false) {
            return Optional.of(SourceOnlySnapshotRepository.getEngineFactory());
        }

        return Optional.empty();
    }

    @Override
    public List<Setting<?>> getSettings() {
        List<Setting<?>> settings = super.getSettings();
        settings.add(SourceOnlySnapshotRepository.SOURCE_ONLY);
        settings.add(DataTierAllocationDecider.CLUSTER_ROUTING_REQUIRE_SETTING);
        settings.add(DataTierAllocationDecider.CLUSTER_ROUTING_INCLUDE_SETTING);
        settings.add(DataTierAllocationDecider.CLUSTER_ROUTING_EXCLUDE_SETTING);
        settings.add(DataTierAllocationDecider.INDEX_ROUTING_REQUIRE_SETTING);
        settings.add(DataTierAllocationDecider.INDEX_ROUTING_INCLUDE_SETTING);
        settings.add(DataTierAllocationDecider.INDEX_ROUTING_EXCLUDE_SETTING);
        settings.add(DataTierAllocationDecider.INDEX_ROUTING_PREFER_SETTING);
        return settings;
    }

    @Override
    public Set<DiscoveryNodeRole> getRoles() {
        return new HashSet<>(Arrays.asList(
            DataTier.DATA_CONTENT_NODE_ROLE,
            DataTier.DATA_HOT_NODE_ROLE,
            DataTier.DATA_WARM_NODE_ROLE,
            DataTier.DATA_COLD_NODE_ROLE));
    }

    @Override
    public Collection<AllocationDecider> createAllocationDeciders(Settings settings, ClusterSettings clusterSettings) {
        return Collections.singleton(new DataTierAllocationDecider(settings, clusterSettings));
    }

    @Override
    public Collection<IndexSettingProvider> getAdditionalIndexSettingProviders() {
        return Collections.singleton(new DataTier.DefaultHotAllocationSettingProvider());
    }

    /**
     * Handles the creation of the SSLService along with the necessary actions to enable reloading
     * of SSLContexts when configuration files change on disk.
     */
    private SSLService createSSLService(Environment environment, ResourceWatcherService resourceWatcherService) {
        final Map<String, SSLConfiguration> sslConfigurations = SSLService.getSSLConfigurations(environment.settings());
        final SSLConfigurationReloader reloader =
            new SSLConfigurationReloader(environment, resourceWatcherService, sslConfigurations.values());
        final SSLService sslService = new SSLService(environment, sslConfigurations);
        reloader.setSSLService(sslService);
        setSslService(sslService);
        return sslService;
    }
}
