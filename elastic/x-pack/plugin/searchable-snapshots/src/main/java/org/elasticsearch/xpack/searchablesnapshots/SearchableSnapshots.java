/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.searchablesnapshots;

import org.apache.lucene.util.SetOnce;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.blobstore.cache.BlobStoreCacheService;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.routing.allocation.ExistingShardsAllocator;
import org.elasticsearch.cluster.routing.allocation.decider.AllocationDecider;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.IndexScopedSettings;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.settings.SettingsFilter;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.NodeEnvironment;
import org.elasticsearch.index.IndexModule;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.engine.Engine;
import org.elasticsearch.index.engine.EngineFactory;
import org.elasticsearch.index.engine.ReadOnlyEngine;
import org.elasticsearch.index.store.SearchableSnapshotDirectory;
import org.elasticsearch.index.store.Store;
import org.elasticsearch.index.translog.Translog;
import org.elasticsearch.index.translog.TranslogStats;
import org.elasticsearch.indices.SystemIndexDescriptor;
import org.elasticsearch.indices.recovery.SearchableSnapshotRecoveryState;
import org.elasticsearch.license.LicenseUtils;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.plugins.ActionPlugin;
import org.elasticsearch.plugins.ClusterPlugin;
import org.elasticsearch.plugins.EnginePlugin;
import org.elasticsearch.plugins.IndexStorePlugin;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.plugins.SystemIndexPlugin;
import org.elasticsearch.repositories.RepositoriesService;
import org.elasticsearch.rest.RestController;
import org.elasticsearch.rest.RestHandler;
import org.elasticsearch.script.ScriptService;
import org.elasticsearch.snapshots.SourceOnlySnapshotRepository;
import org.elasticsearch.threadpool.ExecutorBuilder;
import org.elasticsearch.threadpool.ScalingExecutorBuilder;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.watcher.ResourceWatcherService;
import org.elasticsearch.xpack.cluster.routing.allocation.DataTierAllocationDecider;
import org.elasticsearch.xpack.core.DataTier;
import org.elasticsearch.xpack.core.XPackPlugin;
import org.elasticsearch.xpack.core.action.XPackInfoFeatureAction;
import org.elasticsearch.xpack.core.action.XPackUsageFeatureAction;
import org.elasticsearch.xpack.core.searchablesnapshots.MountSearchableSnapshotAction;
import org.elasticsearch.xpack.searchablesnapshots.action.ClearSearchableSnapshotsCacheAction;
import org.elasticsearch.xpack.searchablesnapshots.action.SearchableSnapshotsStatsAction;
import org.elasticsearch.xpack.searchablesnapshots.action.TransportClearSearchableSnapshotsCacheAction;
import org.elasticsearch.xpack.searchablesnapshots.action.TransportMountSearchableSnapshotAction;
import org.elasticsearch.xpack.searchablesnapshots.action.TransportSearchableSnapshotsStatsAction;
import org.elasticsearch.xpack.searchablesnapshots.action.cache.TransportSearchableSnapshotCacheStoresAction;
import org.elasticsearch.xpack.searchablesnapshots.cache.CacheService;
import org.elasticsearch.xpack.searchablesnapshots.cache.PersistentCache;
import org.elasticsearch.xpack.searchablesnapshots.rest.RestClearSearchableSnapshotsCacheAction;
import org.elasticsearch.xpack.searchablesnapshots.rest.RestMountSearchableSnapshotAction;
import org.elasticsearch.xpack.searchablesnapshots.rest.RestSearchableSnapshotsStatsAction;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.function.Function;
import java.util.function.Supplier;

import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.elasticsearch.index.mapper.MapperService.SINGLE_MAPPING_NAME;
import static org.elasticsearch.xpack.core.ClientHelper.SEARCHABLE_SNAPSHOTS_ORIGIN;
import static org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshotsConstants.CACHE_FETCH_ASYNC_THREAD_POOL_NAME;
import static org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshotsConstants.CACHE_FETCH_ASYNC_THREAD_POOL_SETTING;
import static org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshotsConstants.CACHE_PREWARMING_THREAD_POOL_NAME;
import static org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshotsConstants.CACHE_PREWARMING_THREAD_POOL_SETTING;
import static org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshotsConstants.SNAPSHOT_BLOB_CACHE_INDEX;
import static org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshotsConstants.SNAPSHOT_DIRECTORY_FACTORY_KEY;
import static org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshotsConstants.SNAPSHOT_RECOVERY_STATE_FACTORY_KEY;
import static org.elasticsearch.xpack.searchablesnapshots.SearchableSnapshotsUtils.emptyIndexCommit;

/**
 * Plugin for Searchable Snapshots feature
 */
public class SearchableSnapshots extends Plugin implements IndexStorePlugin, EnginePlugin, ActionPlugin, ClusterPlugin, SystemIndexPlugin {

    public static final Setting<String> SNAPSHOT_REPOSITORY_NAME_SETTING = Setting.simpleString(
        "index.store.snapshot.repository_name",
        Setting.Property.IndexScope,
        Setting.Property.PrivateIndex,
        Setting.Property.NotCopyableOnResize
    );
    public static final Setting<String> SNAPSHOT_REPOSITORY_UUID_SETTING = Setting.simpleString(
        "index.store.snapshot.repository_uuid",
        Setting.Property.IndexScope,
        Setting.Property.PrivateIndex,
        Setting.Property.NotCopyableOnResize
    );
    public static final Setting<String> SNAPSHOT_SNAPSHOT_NAME_SETTING = Setting.simpleString(
        "index.store.snapshot.snapshot_name",
        Setting.Property.IndexScope,
        Setting.Property.PrivateIndex,
        Setting.Property.NotCopyableOnResize
    );
    public static final Setting<String> SNAPSHOT_SNAPSHOT_ID_SETTING = Setting.simpleString(
        "index.store.snapshot.snapshot_uuid",
        Setting.Property.IndexScope,
        Setting.Property.PrivateIndex,
        Setting.Property.NotCopyableOnResize
    );
    public static final Setting<String> SNAPSHOT_INDEX_NAME_SETTING = Setting.simpleString(
        "index.store.snapshot.index_name",
        Setting.Property.IndexScope,
        Setting.Property.PrivateIndex,
        Setting.Property.NotCopyableOnResize
    );
    public static final Setting<String> SNAPSHOT_INDEX_ID_SETTING = Setting.simpleString(
        "index.store.snapshot.index_uuid",
        Setting.Property.IndexScope,
        Setting.Property.PrivateIndex,
        Setting.Property.NotCopyableOnResize
    );
    public static final Setting<Boolean> SNAPSHOT_CACHE_ENABLED_SETTING = Setting.boolSetting(
        "index.store.snapshot.cache.enabled",
        true,
        Setting.Property.IndexScope,
        Setting.Property.NotCopyableOnResize
    );
    public static final Setting<Boolean> SNAPSHOT_CACHE_PREWARM_ENABLED_SETTING = Setting.boolSetting(
        "index.store.snapshot.cache.prewarm.enabled",
        true,
        Setting.Property.IndexScope,
        Setting.Property.NotCopyableOnResize
    );
    // The file extensions that are excluded from the cache
    public static final Setting<List<String>> SNAPSHOT_CACHE_EXCLUDED_FILE_TYPES_SETTING = Setting.listSetting(
        "index.store.snapshot.cache.excluded_file_types",
        Collections.emptyList(),
        Function.identity(),
        Setting.Property.IndexScope,
        Setting.Property.NodeScope,
        Setting.Property.NotCopyableOnResize
    );
    public static final Setting<ByteSizeValue> SNAPSHOT_UNCACHED_CHUNK_SIZE_SETTING = Setting.byteSizeSetting(
        "index.store.snapshot.uncached_chunk_size",
        new ByteSizeValue(-1, ByteSizeUnit.BYTES),
        Setting.Property.IndexScope,
        Setting.Property.NodeScope,
        Setting.Property.NotCopyableOnResize
    );

    /**
     * Prefer to allocate to the cold tier, then the warm tier, then the hot tier
     */
    public static final String DATA_TIERS_PREFERENCE = String.join(",", DataTier.DATA_COLD, DataTier.DATA_WARM, DataTier.DATA_HOT);

    private volatile Supplier<RepositoriesService> repositoriesServiceSupplier;
    private final SetOnce<BlobStoreCacheService> blobStoreCacheService = new SetOnce<>();
    private final SetOnce<CacheService> cacheService = new SetOnce<>();
    private final SetOnce<ThreadPool> threadPool = new SetOnce<>();
    private final SetOnce<FailShardsOnInvalidLicenseClusterListener> failShardsListener = new SetOnce<>();
    private final SetOnce<SearchableSnapshotAllocator> allocator = new SetOnce<>();
    private final Settings settings;

    public SearchableSnapshots(final Settings settings) {
        this.settings = settings;
    }

    public static void ensureValidLicense(XPackLicenseState licenseState) {
        if (licenseState.isAllowed(XPackLicenseState.Feature.SEARCHABLE_SNAPSHOTS) == false) {
            throw LicenseUtils.newComplianceException("searchable-snapshots");
        }
    }

    @Override
    public List<Setting<?>> getSettings() {
        return List.of(
            SNAPSHOT_REPOSITORY_UUID_SETTING,
            SNAPSHOT_REPOSITORY_NAME_SETTING,
            SNAPSHOT_SNAPSHOT_NAME_SETTING,
            SNAPSHOT_SNAPSHOT_ID_SETTING,
            SNAPSHOT_INDEX_NAME_SETTING,
            SNAPSHOT_INDEX_ID_SETTING,
            SNAPSHOT_CACHE_ENABLED_SETTING,
            SNAPSHOT_CACHE_PREWARM_ENABLED_SETTING,
            SNAPSHOT_CACHE_EXCLUDED_FILE_TYPES_SETTING,
            SNAPSHOT_UNCACHED_CHUNK_SIZE_SETTING,
            CacheService.SNAPSHOT_CACHE_SIZE_SETTING,
            CacheService.SNAPSHOT_CACHE_RANGE_SIZE_SETTING,
            CacheService.SNAPSHOT_CACHE_RECOVERY_RANGE_SIZE_SETTING,
            CacheService.SNAPSHOT_CACHE_SYNC_INTERVAL_SETTING,
            CacheService.SNAPSHOT_CACHE_MAX_FILES_TO_SYNC_AT_ONCE_SETTING,
            CacheService.SNAPSHOT_CACHE_SYNC_SHUTDOWN_TIMEOUT,
            SearchableSnapshotEnableAllocationDecider.SEARCHABLE_SNAPSHOTS_ALLOCATE_ON_ROLLING_RESTART
        );
    }

    @Override
    public Collection<Object> createComponents(
        final Client client,
        final ClusterService clusterService,
        final ThreadPool threadPool,
        final ResourceWatcherService resourceWatcherService,
        final ScriptService scriptService,
        final NamedXContentRegistry xContentRegistry,
        final Environment environment,
        final NodeEnvironment nodeEnvironment,
        final NamedWriteableRegistry registry,
        final IndexNameExpressionResolver resolver,
        final Supplier<RepositoriesService> repositoriesServiceSupplier
    ) {
        final List<Object> components = new ArrayList<>();
        this.repositoriesServiceSupplier = repositoriesServiceSupplier;
        this.threadPool.set(threadPool);
        this.failShardsListener.set(new FailShardsOnInvalidLicenseClusterListener(getLicenseState(), clusterService.getRerouteService()));
        if (DiscoveryNode.isDataNode(settings)) {
            final CacheService cacheService = new CacheService(settings, clusterService, threadPool, new PersistentCache(nodeEnvironment));
            this.cacheService.set(cacheService);
            components.add(cacheService);
            final BlobStoreCacheService blobStoreCacheService = new BlobStoreCacheService(threadPool, client, SNAPSHOT_BLOB_CACHE_INDEX);
            this.blobStoreCacheService.set(blobStoreCacheService);
            components.add(blobStoreCacheService);
        } else {
            PersistentCache.cleanUp(settings, nodeEnvironment);
        }
        this.allocator.set(new SearchableSnapshotAllocator(client, clusterService.getRerouteService()));
        components.add(new CacheServiceSupplier(cacheService.get()));
        return Collections.unmodifiableList(components);
    }

    @Override
    public void onIndexModule(IndexModule indexModule) {
        if (SearchableSnapshotsConstants.isSearchableSnapshotStore(indexModule.getSettings())) {
            indexModule.addIndexEventListener(new SearchableSnapshotIndexEventListener(settings, cacheService.get()));
            indexModule.addIndexEventListener(failShardsListener.get());

            indexModule.addSettingsUpdateConsumer(IndexMetadata.INDEX_BLOCKS_WRITE_SETTING, s -> {}, write -> {
                if (write == false) {
                    throw new IllegalArgumentException("Cannot remove write block from searchable snapshot index");
                }
            });
        }
    }

    @Override
    public List<IndexFoldersDeletionListener> getIndexFoldersDeletionListeners() {
        if (DiscoveryNode.isDataNode(settings)) {
            return List.of(new SearchableSnapshotIndexFoldersDeletionListener(cacheService::get));
        }
        return List.of();
    }

    @Override
    public Collection<SystemIndexDescriptor> getSystemIndexDescriptors(Settings settings) {
        return List.of(
            SystemIndexDescriptor.builder()
                .setIndexPattern(SNAPSHOT_BLOB_CACHE_INDEX)
                .setDescription("Contains cached data of blob store repositories")
                .setPrimaryIndex(SNAPSHOT_BLOB_CACHE_INDEX)
                .setMappings(getIndexMappings())
                .setSettings(getIndexSettings())
                .setOrigin(SEARCHABLE_SNAPSHOTS_ORIGIN)
                .setVersionMetaKey("version")
                .build()
        );
    }

    @Override
    public Map<String, DirectoryFactory> getDirectoryFactories() {
        return Map.of(SNAPSHOT_DIRECTORY_FACTORY_KEY, (indexSettings, shardPath) -> {
            final RepositoriesService repositories = repositoriesServiceSupplier.get();
            assert repositories != null;
            final CacheService cache = cacheService.get();
            assert cache != null;
            final ThreadPool threadPool = this.threadPool.get();
            assert threadPool != null;
            final BlobStoreCacheService blobCache = blobStoreCacheService.get();
            assert blobCache != null;
            return SearchableSnapshotDirectory.create(
                repositories,
                cache,
                indexSettings,
                shardPath,
                System::nanoTime,
                threadPool,
                blobCache
            );
        });
    }

    @Override
    public Optional<EngineFactory> getEngineFactory(IndexSettings indexSettings) {
        if (SearchableSnapshotsConstants.isSearchableSnapshotStore(indexSettings.getSettings())
            && indexSettings.getSettings().getAsBoolean("index.frozen", false) == false) {
            return Optional.of(
                engineConfig -> new ReadOnlyEngine(
                    engineConfig,
                    null,
                    new TranslogStats(),
                    false,
                    indexSettings.getValue(SourceOnlySnapshotRepository.SOURCE_ONLY)
                        ? SourceOnlySnapshotRepository.readerWrapper(engineConfig)
                        : Function.identity(),
                    false
                )
            );
        }
        return Optional.empty();
    }

    @Override
    public Map<String, SnapshotCommitSupplier> getSnapshotCommitSuppliers() {
        return Map.of(SNAPSHOT_DIRECTORY_FACTORY_KEY, e -> {
            final Store store = e.config().getStore();
            store.incRef();
            return new Engine.IndexCommitRef(emptyIndexCommit(store.directory()), store::decRef);
        });
    }

    @Override
    public List<ActionHandler<? extends ActionRequest, ? extends ActionResponse>> getActions() {
        return List.of(
            new ActionHandler<>(SearchableSnapshotsStatsAction.INSTANCE, TransportSearchableSnapshotsStatsAction.class),
            new ActionHandler<>(ClearSearchableSnapshotsCacheAction.INSTANCE, TransportClearSearchableSnapshotsCacheAction.class),
            new ActionHandler<>(MountSearchableSnapshotAction.INSTANCE, TransportMountSearchableSnapshotAction.class),
            new ActionHandler<>(XPackUsageFeatureAction.SEARCHABLE_SNAPSHOTS, SearchableSnapshotsUsageTransportAction.class),
            new ActionHandler<>(XPackInfoFeatureAction.SEARCHABLE_SNAPSHOTS, SearchableSnapshotsInfoTransportAction.class),
            new ActionHandler<>(TransportSearchableSnapshotCacheStoresAction.TYPE, TransportSearchableSnapshotCacheStoresAction.class)
        );
    }

    public List<RestHandler> getRestHandlers(
        Settings settings,
        RestController restController,
        ClusterSettings clusterSettings,
        IndexScopedSettings indexScopedSettings,
        SettingsFilter settingsFilter,
        IndexNameExpressionResolver indexNameExpressionResolver,
        Supplier<DiscoveryNodes> nodesInCluster
    ) {
        return List.of(
            new RestSearchableSnapshotsStatsAction(),
            new RestClearSearchableSnapshotsCacheAction(),
            new RestMountSearchableSnapshotAction()
        );
    }

    @Override
    public Map<String, ExistingShardsAllocator> getExistingShardsAllocators() {
        return Map.of(SearchableSnapshotAllocator.ALLOCATOR_NAME, allocator.get());
    }

    // overridable by tests
    protected XPackLicenseState getLicenseState() {
        return XPackPlugin.getSharedLicenseState();
    }

    @Override
    public Collection<AllocationDecider> createAllocationDeciders(Settings settings, ClusterSettings clusterSettings) {
        return List.of(
            new SearchableSnapshotAllocationDecider(() -> getLicenseState().isAllowed(XPackLicenseState.Feature.SEARCHABLE_SNAPSHOTS)),
            new SearchableSnapshotEnableAllocationDecider(settings, clusterSettings)
        );
    }

    public List<ExecutorBuilder<?>> getExecutorBuilders(Settings settings) {
        return List.of(executorBuilders());
    }

    @Override
    public Map<String, RecoveryStateFactory> getRecoveryStateFactories() {
        return Map.of(SNAPSHOT_RECOVERY_STATE_FACTORY_KEY, SearchableSnapshotRecoveryState::new);
    }

    public static ScalingExecutorBuilder[] executorBuilders() {
        return new ScalingExecutorBuilder[] {
            new ScalingExecutorBuilder(
                CACHE_FETCH_ASYNC_THREAD_POOL_NAME,
                0,
                28,
                TimeValue.timeValueSeconds(30L),
                CACHE_FETCH_ASYNC_THREAD_POOL_SETTING
            ),
            new ScalingExecutorBuilder(
                CACHE_PREWARMING_THREAD_POOL_NAME,
                0,
                16,
                TimeValue.timeValueSeconds(30L),
                CACHE_PREWARMING_THREAD_POOL_SETTING
            ) };
    }

    private Settings getIndexSettings() {
        return Settings.builder()
            .put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, 1)
            .put(IndexMetadata.SETTING_AUTO_EXPAND_REPLICAS, "0-1")
            .put(IndexMetadata.SETTING_PRIORITY, "900")
            .put(IndexSettings.INDEX_TRANSLOG_DURABILITY_SETTING.getKey(), Translog.Durability.ASYNC)
            .put(DataTierAllocationDecider.INDEX_ROUTING_PREFER, DATA_TIERS_PREFERENCE)
            .build();
    }

    private XContentBuilder getIndexMappings() {
        try {
            final XContentBuilder builder = jsonBuilder();
            {
                builder.startObject();
                {
                    builder.startObject(SINGLE_MAPPING_NAME);
                    builder.field("dynamic", "strict");
                    {
                        builder.startObject("_meta");
                        builder.field("version", Version.CURRENT);
                        builder.endObject();
                    }
                    {
                        builder.startObject("properties");
                        {
                            builder.startObject("type");
                            builder.field("type", "keyword");
                            builder.endObject();
                        }
                        {
                            builder.startObject("creation_time");
                            builder.field("type", "date");
                            builder.field("format", "epoch_millis");
                            builder.endObject();
                        }
                        {
                            builder.startObject("version");
                            builder.field("type", "integer");
                            builder.endObject();
                        }
                        {
                            builder.startObject("repository");
                            builder.field("type", "keyword");
                            builder.endObject();
                        }
                        {
                            builder.startObject("blob");
                            builder.field("type", "object");
                            {
                                builder.startObject("properties");
                                {
                                    builder.startObject("name");
                                    builder.field("type", "keyword");
                                    builder.endObject();
                                    builder.startObject("path");
                                    builder.field("type", "keyword");
                                    builder.endObject();
                                }
                                builder.endObject();
                            }
                            builder.endObject();
                        }
                        {
                            builder.startObject("data");
                            builder.field("type", "object");
                            {
                                builder.startObject("properties");
                                {
                                    builder.startObject("content");
                                    builder.field("type", "binary");
                                    builder.endObject();
                                }
                                {
                                    builder.startObject("length");
                                    builder.field("type", "long");
                                    builder.endObject();
                                }
                                {
                                    builder.startObject("from");
                                    builder.field("type", "long");
                                    builder.endObject();
                                }
                                {
                                    builder.startObject("to");
                                    builder.field("type", "long");
                                    builder.endObject();
                                }
                                builder.endObject();
                            }
                            builder.endObject();
                        }
                        builder.endObject();
                    }
                    builder.endObject();
                }
                builder.endObject();
            }
            return builder;
        } catch (IOException e) {
            throw new UncheckedIOException("Failed to build " + SNAPSHOT_BLOB_CACHE_INDEX + " index mappings", e);
        }
    }

    public static final class CacheServiceSupplier implements Supplier<CacheService> {

        @Nullable
        private final CacheService cacheService;

        CacheServiceSupplier(@Nullable CacheService cacheService) {
            this.cacheService = cacheService;
        }

        @Override
        public CacheService get() {
            return cacheService;
        }
    }
}
