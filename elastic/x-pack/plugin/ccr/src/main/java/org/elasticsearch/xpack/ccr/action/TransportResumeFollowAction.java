/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.ccr.action;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.action.support.master.TransportMasterNodeAction;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.routing.allocation.decider.EnableAllocationDecider;
import org.elasticsearch.cluster.routing.allocation.decider.ShardsLimitAllocationDecider;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Setting;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.index.IndexNotFoundException;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.IndexingSlowLog;
import org.elasticsearch.index.SearchSlowLog;
import org.elasticsearch.index.cache.bitset.BitsetFilterCache;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.indices.IndicesRequestCache;
import org.elasticsearch.indices.IndicesService;
import org.elasticsearch.license.LicenseUtils;
import org.elasticsearch.persistent.PersistentTasksService;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.ccr.Ccr;
import org.elasticsearch.xpack.ccr.CcrLicenseChecker;
import org.elasticsearch.xpack.ccr.CcrSettings;
import org.elasticsearch.xpack.core.ccr.action.ResumeFollowAction;

import java.io.IOException;
import java.util.Collections;
import java.util.HashSet;
import java.util.Iterator;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.stream.Collectors;

public class TransportResumeFollowAction extends TransportMasterNodeAction<ResumeFollowAction.Request, AcknowledgedResponse> {

    static final ByteSizeValue DEFAULT_MAX_READ_REQUEST_SIZE = new ByteSizeValue(32, ByteSizeUnit.MB);
    static final ByteSizeValue DEFAULT_MAX_WRITE_REQUEST_SIZE = new ByteSizeValue(Long.MAX_VALUE, ByteSizeUnit.BYTES);
    private static final TimeValue DEFAULT_MAX_RETRY_DELAY = new TimeValue(500);
    private static final int DEFAULT_MAX_OUTSTANDING_WRITE_REQUESTS = 9;
    private static final int DEFAULT_MAX_WRITE_BUFFER_COUNT = Integer.MAX_VALUE;
    private static final ByteSizeValue DEFAULT_MAX_WRITE_BUFFER_SIZE = new ByteSizeValue(512, ByteSizeUnit.MB);
    private static final int DEFAULT_MAX_READ_REQUEST_OPERATION_COUNT = 5120;
    private static final int DEFAULT_MAX_WRITE_REQUEST_OPERATION_COUNT = 5120;
    private static final int DEFAULT_MAX_OUTSTANDING_READ_REQUESTS = 12;
    static final TimeValue DEFAULT_READ_POLL_TIMEOUT = TimeValue.timeValueMinutes(1);

    private final Client client;
    private final ThreadPool threadPool;
    private final PersistentTasksService persistentTasksService;
    private final IndicesService indicesService;
    private final CcrLicenseChecker ccrLicenseChecker;

    @Inject
    public TransportResumeFollowAction(
            final ThreadPool threadPool,
            final TransportService transportService,
            final ActionFilters actionFilters,
            final Client client,
            final ClusterService clusterService,
            final IndexNameExpressionResolver indexNameExpressionResolver,
            final PersistentTasksService persistentTasksService,
            final IndicesService indicesService,
            final CcrLicenseChecker ccrLicenseChecker) {
        super(ResumeFollowAction.NAME, true, transportService, clusterService, threadPool, actionFilters,
            ResumeFollowAction.Request::new, indexNameExpressionResolver);
        this.client = client;
        this.threadPool = threadPool;
        this.persistentTasksService = persistentTasksService;
        this.indicesService = indicesService;
        this.ccrLicenseChecker = Objects.requireNonNull(ccrLicenseChecker);
    }

    @Override
    protected String executor() {
        return ThreadPool.Names.SAME;
    }

    @Override
    protected AcknowledgedResponse newResponse() {
        return new AcknowledgedResponse();
    }

    @Override
    protected ClusterBlockException checkBlock(ResumeFollowAction.Request request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }

    @Override
    protected void masterOperation(final ResumeFollowAction.Request request,
                                   ClusterState state,
                                   final ActionListener<AcknowledgedResponse> listener) throws Exception {
        if (ccrLicenseChecker.isCcrAllowed() == false) {
            listener.onFailure(LicenseUtils.newComplianceException("ccr"));
            return;
        }

        final IndexMetaData followerIndexMetadata = state.getMetaData().index(request.getFollowerIndex());
        if (followerIndexMetadata == null) {
            listener.onFailure(new IndexNotFoundException(request.getFollowerIndex()));
            return;
        }

        final Map<String, String> ccrMetadata = followerIndexMetadata.getCustomData(Ccr.CCR_CUSTOM_METADATA_KEY);
        if (ccrMetadata == null) {
            throw new IllegalArgumentException("follow index ["+ request.getFollowerIndex() + "] does not have ccr metadata");
        }
        final String leaderCluster = ccrMetadata.get(Ccr.CCR_CUSTOM_METADATA_REMOTE_CLUSTER_NAME_KEY);
        // Validates whether the leader cluster has been configured properly:
        client.getRemoteClusterClient(leaderCluster);
        final String leaderIndex = ccrMetadata.get(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_NAME_KEY);
        ccrLicenseChecker.checkRemoteClusterLicenseAndFetchLeaderIndexMetadataAndHistoryUUIDs(
            client,
            leaderCluster,
            leaderIndex,
            listener::onFailure,
            (leaderHistoryUUID, leaderIndexMetadata) -> {
                try {
                    start(request, leaderCluster, leaderIndexMetadata, followerIndexMetadata, leaderHistoryUUID, listener);
                } catch (final IOException e) {
                    listener.onFailure(e);
                }
            });
    }

    /**
     * Performs validation on the provided leader and follow {@link IndexMetaData} instances and then
     * creates a persistent task for each leader primary shard. This persistent tasks track changes in the leader
     * shard and replicate these changes to a follower shard.
     *
     * Currently the following validation is performed:
     * <ul>
     *     <li>The leader index and follow index need to have the same number of primary shards</li>
     * </ul>
     */
    void start(
            ResumeFollowAction.Request request,
            String clusterNameAlias,
            IndexMetaData leaderIndexMetadata,
            IndexMetaData followIndexMetadata,
            String[] leaderIndexHistoryUUIDs,
            ActionListener<AcknowledgedResponse> listener) throws IOException {

        MapperService mapperService = followIndexMetadata != null ? indicesService.createIndexMapperService(followIndexMetadata) : null;
        validate(request, leaderIndexMetadata, followIndexMetadata, leaderIndexHistoryUUIDs, mapperService);
        final int numShards = followIndexMetadata.getNumberOfShards();
        final ResponseHandler handler = new ResponseHandler(numShards, listener);
        Map<String, String> filteredHeaders = threadPool.getThreadContext().getHeaders().entrySet().stream()
                .filter(e -> ShardFollowTask.HEADER_FILTERS.contains(e.getKey()))
                .collect(Collectors.toMap(Map.Entry::getKey, Map.Entry::getValue));

        for (int shardId = 0; shardId < numShards; shardId++) {
            String taskId = followIndexMetadata.getIndexUUID() + "-" + shardId;

            final ShardFollowTask shardFollowTask = createShardFollowTask(shardId, clusterNameAlias, request,
                leaderIndexMetadata, followIndexMetadata, filteredHeaders);
            persistentTasksService.sendStartRequest(taskId, ShardFollowTask.NAME, shardFollowTask, handler.getActionListener(shardId));
        }
    }

    static void validate(
            final ResumeFollowAction.Request request,
            final IndexMetaData leaderIndex,
            final IndexMetaData followIndex,
            final String[] leaderIndexHistoryUUID,
            final MapperService followerMapperService) {
        Map<String, String> ccrIndexMetadata = followIndex.getCustomData(Ccr.CCR_CUSTOM_METADATA_KEY);
        if (ccrIndexMetadata == null) {
            throw new IllegalArgumentException("follow index ["+ followIndex.getIndex().getName() + "] does not have ccr metadata");
        }
        String leaderIndexUUID = leaderIndex.getIndex().getUUID();
        String recordedLeaderIndexUUID = ccrIndexMetadata.get(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_UUID_KEY);
        if (leaderIndexUUID.equals(recordedLeaderIndexUUID) == false) {
            throw new IllegalArgumentException("follow index [" + request.getFollowerIndex() + "] should reference [" + leaderIndexUUID +
                    "] as leader index but instead reference [" + recordedLeaderIndexUUID + "] as leader index");
        }

        String[] recordedHistoryUUIDs = extractLeaderShardHistoryUUIDs(ccrIndexMetadata);
        assert recordedHistoryUUIDs.length == leaderIndexHistoryUUID.length;
        for (int i = 0; i < leaderIndexHistoryUUID.length; i++) {
            String recordedLeaderIndexHistoryUUID = recordedHistoryUUIDs[i];
            String actualLeaderIndexHistoryUUID = leaderIndexHistoryUUID[i];
            if (recordedLeaderIndexHistoryUUID.equals(actualLeaderIndexHistoryUUID) == false) {
                throw new IllegalArgumentException("leader shard [" + request.getFollowerIndex() + "][" + i + "] should reference [" +
                    recordedLeaderIndexHistoryUUID + "] as history uuid but instead reference [" + actualLeaderIndexHistoryUUID +
                    "] as history uuid");
            }
        }

        if (leaderIndex.getSettings().getAsBoolean(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), false) == false) {
            throw new IllegalArgumentException("leader index [" + leaderIndex.getIndex().getName() +
                "] does not have soft deletes enabled");
        }
        if (followIndex.getSettings().getAsBoolean(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), false) == false) {
            throw new IllegalArgumentException("follower index [" + request.getFollowerIndex() + "] does not have soft deletes enabled");
        }
        if (leaderIndex.getNumberOfShards() != followIndex.getNumberOfShards()) {
            throw new IllegalArgumentException("leader index primary shards [" + leaderIndex.getNumberOfShards() +
                    "] does not match with the number of shards of the follow index [" + followIndex.getNumberOfShards() + "]");
        }
        if (leaderIndex.getRoutingNumShards() != followIndex.getRoutingNumShards()) {
            throw new IllegalArgumentException("leader index number_of_routing_shards [" + leaderIndex.getRoutingNumShards() +
                    "] does not match with the number_of_routing_shards of the follow index [" + followIndex.getRoutingNumShards() + "]");
        }
        if (leaderIndex.getState() != IndexMetaData.State.OPEN || followIndex.getState() != IndexMetaData.State.OPEN) {
            throw new IllegalArgumentException("leader and follow index must be open");
        }
        if (CcrSettings.CCR_FOLLOWING_INDEX_SETTING.get(followIndex.getSettings()) == false) {
            throw new IllegalArgumentException("the following index [" + request.getFollowerIndex() + "] is not ready " +
                    "to follow; the setting [" + CcrSettings.CCR_FOLLOWING_INDEX_SETTING.getKey() + "] must be enabled.");
        }
        // Make a copy, remove settings that are allowed to be different and then compare if the settings are equal.
        Settings leaderSettings = filter(leaderIndex.getSettings());
        Settings followerSettings = filter(followIndex.getSettings());
        if (leaderSettings.equals(followerSettings) == false) {
            throw new IllegalArgumentException("the leader and follower index settings must be identical");
        }

        // Validates if the current follower mapping is mergable with the leader mapping.
        // This also validates for example whether specific mapper plugins have been installed
        followerMapperService.merge(leaderIndex, MapperService.MergeReason.MAPPING_RECOVERY);
    }

    private static ShardFollowTask createShardFollowTask(
        int shardId,
        String clusterAliasName,
        ResumeFollowAction.Request request,
        IndexMetaData leaderIndexMetadata,
        IndexMetaData followIndexMetadata,
        Map<String, String> filteredHeaders
    ) {
        int maxReadRequestOperationCount;
        if (request.getMaxReadRequestOperationCount() != null) {
            maxReadRequestOperationCount = request.getMaxReadRequestOperationCount();
        } else {
            maxReadRequestOperationCount = DEFAULT_MAX_READ_REQUEST_OPERATION_COUNT;
        }

        ByteSizeValue maxReadRequestSize;
        if (request.getMaxReadRequestSize() != null) {
            maxReadRequestSize = request.getMaxReadRequestSize();
        } else {
            maxReadRequestSize = DEFAULT_MAX_READ_REQUEST_SIZE;
        }

        int maxOutstandingReadRequests;
        if (request.getMaxOutstandingReadRequests() != null){
            maxOutstandingReadRequests = request.getMaxOutstandingReadRequests();
        } else {
            maxOutstandingReadRequests = DEFAULT_MAX_OUTSTANDING_READ_REQUESTS;
        }

        final int maxWriteRequestOperationCount;
        if (request.getMaxWriteRequestOperationCount() != null) {
            maxWriteRequestOperationCount = request.getMaxWriteRequestOperationCount();
        } else {
            maxWriteRequestOperationCount = DEFAULT_MAX_WRITE_REQUEST_OPERATION_COUNT;
        }

        final ByteSizeValue maxWriteRequestSize;
        if (request.getMaxWriteRequestSize() != null) {
            maxWriteRequestSize = request.getMaxWriteRequestSize();
        } else {
            maxWriteRequestSize = DEFAULT_MAX_WRITE_REQUEST_SIZE;
        }

        int maxOutstandingWriteRequests;
        if (request.getMaxOutstandingWriteRequests() != null) {
            maxOutstandingWriteRequests = request.getMaxOutstandingWriteRequests();
        } else {
            maxOutstandingWriteRequests = DEFAULT_MAX_OUTSTANDING_WRITE_REQUESTS;
        }

        int maxWriteBufferCount;
        if (request.getMaxWriteBufferCount() != null) {
            maxWriteBufferCount = request.getMaxWriteBufferCount();
        } else {
            maxWriteBufferCount = DEFAULT_MAX_WRITE_BUFFER_COUNT;
        }

        ByteSizeValue maxWriteBufferSize;
        if (request.getMaxWriteBufferSize() != null) {
            maxWriteBufferSize = request.getMaxWriteBufferSize();
        } else {
            maxWriteBufferSize = DEFAULT_MAX_WRITE_BUFFER_SIZE;
        }

        TimeValue maxRetryDelay = request.getMaxRetryDelay() == null ? DEFAULT_MAX_RETRY_DELAY : request.getMaxRetryDelay();
        TimeValue readPollTimeout = request.getReadPollTimeout() == null ? DEFAULT_READ_POLL_TIMEOUT : request.getReadPollTimeout();

        return new ShardFollowTask(
            clusterAliasName,
            new ShardId(followIndexMetadata.getIndex(), shardId),
            new ShardId(leaderIndexMetadata.getIndex(), shardId),
            maxReadRequestOperationCount,
            maxReadRequestSize,
            maxOutstandingReadRequests,
            maxWriteRequestOperationCount,
            maxWriteRequestSize,
            maxOutstandingWriteRequests,
            maxWriteBufferCount,
            maxWriteBufferSize,
            maxRetryDelay,
            readPollTimeout,
            filteredHeaders
        );
    }

    static String[] extractLeaderShardHistoryUUIDs(Map<String, String> ccrIndexMetaData) {
        String historyUUIDs = ccrIndexMetaData.get(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_SHARD_HISTORY_UUIDS);
        if (historyUUIDs == null) {
            throw new IllegalArgumentException("leader index shard UUIDs are missing");
        }

        return historyUUIDs.split(",");
    }

    private static final Set<Setting<?>> WHITE_LISTED_SETTINGS;

    static {
        final Set<Setting<?>> whiteListedSettings = new HashSet<>();
        whiteListedSettings.add(IndexMetaData.INDEX_NUMBER_OF_REPLICAS_SETTING);
        whiteListedSettings.add(IndexMetaData.INDEX_AUTO_EXPAND_REPLICAS_SETTING);

        whiteListedSettings.add(IndexMetaData.INDEX_ROUTING_EXCLUDE_GROUP_SETTING);
        whiteListedSettings.add(IndexMetaData.INDEX_ROUTING_INCLUDE_GROUP_SETTING);
        whiteListedSettings.add(IndexMetaData.INDEX_ROUTING_REQUIRE_GROUP_SETTING);
        whiteListedSettings.add(EnableAllocationDecider.INDEX_ROUTING_REBALANCE_ENABLE_SETTING);
        whiteListedSettings.add(EnableAllocationDecider.INDEX_ROUTING_ALLOCATION_ENABLE_SETTING);
        whiteListedSettings.add(ShardsLimitAllocationDecider.INDEX_TOTAL_SHARDS_PER_NODE_SETTING);

        whiteListedSettings.add(IndicesRequestCache.INDEX_CACHE_REQUEST_ENABLED_SETTING);
        whiteListedSettings.add(IndexSettings.MAX_RESULT_WINDOW_SETTING);
        whiteListedSettings.add(IndexSettings.INDEX_WARMER_ENABLED_SETTING);
        whiteListedSettings.add(IndexSettings.INDEX_REFRESH_INTERVAL_SETTING);
        whiteListedSettings.add(IndexSettings.MAX_RESCORE_WINDOW_SETTING);
        whiteListedSettings.add(IndexSettings.MAX_INNER_RESULT_WINDOW_SETTING);
        whiteListedSettings.add(IndexSettings.DEFAULT_FIELD_SETTING);
        whiteListedSettings.add(IndexSettings.QUERY_STRING_LENIENT_SETTING);
        whiteListedSettings.add(IndexSettings.QUERY_STRING_ANALYZE_WILDCARD);
        whiteListedSettings.add(IndexSettings.QUERY_STRING_ALLOW_LEADING_WILDCARD);
        whiteListedSettings.add(IndexSettings.ALLOW_UNMAPPED);
        whiteListedSettings.add(IndexSettings.INDEX_SEARCH_IDLE_AFTER);
        whiteListedSettings.add(BitsetFilterCache.INDEX_LOAD_RANDOM_ACCESS_FILTERS_EAGERLY_SETTING);

        whiteListedSettings.add(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_DEBUG_SETTING);
        whiteListedSettings.add(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_WARN_SETTING);
        whiteListedSettings.add(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_INFO_SETTING);
        whiteListedSettings.add(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_FETCH_TRACE_SETTING);
        whiteListedSettings.add(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_WARN_SETTING);
        whiteListedSettings.add(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_DEBUG_SETTING);
        whiteListedSettings.add(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_INFO_SETTING);
        whiteListedSettings.add(SearchSlowLog.INDEX_SEARCH_SLOWLOG_THRESHOLD_QUERY_TRACE_SETTING);
        whiteListedSettings.add(SearchSlowLog.INDEX_SEARCH_SLOWLOG_LEVEL);
        whiteListedSettings.add(IndexingSlowLog.INDEX_INDEXING_SLOWLOG_THRESHOLD_INDEX_WARN_SETTING);
        whiteListedSettings.add(IndexingSlowLog.INDEX_INDEXING_SLOWLOG_THRESHOLD_INDEX_DEBUG_SETTING);
        whiteListedSettings.add(IndexingSlowLog.INDEX_INDEXING_SLOWLOG_THRESHOLD_INDEX_INFO_SETTING);
        whiteListedSettings.add(IndexingSlowLog.INDEX_INDEXING_SLOWLOG_THRESHOLD_INDEX_TRACE_SETTING);
        whiteListedSettings.add(IndexingSlowLog.INDEX_INDEXING_SLOWLOG_LEVEL_SETTING);
        whiteListedSettings.add(IndexingSlowLog.INDEX_INDEXING_SLOWLOG_REFORMAT_SETTING);
        whiteListedSettings.add(IndexingSlowLog.INDEX_INDEXING_SLOWLOG_MAX_SOURCE_CHARS_TO_LOG_SETTING);

        whiteListedSettings.add(IndexSettings.INDEX_SOFT_DELETES_RETENTION_OPERATIONS_SETTING);

        WHITE_LISTED_SETTINGS = Collections.unmodifiableSet(whiteListedSettings);
    }

    private static Settings filter(Settings originalSettings) {
        Settings.Builder settings = Settings.builder().put(originalSettings);
        // Remove settings that are always going to be different between leader and follow index:
        settings.remove(CcrSettings.CCR_FOLLOWING_INDEX_SETTING.getKey());
        settings.remove(IndexMetaData.SETTING_INDEX_UUID);
        settings.remove(IndexMetaData.SETTING_INDEX_PROVIDED_NAME);
        settings.remove(IndexMetaData.SETTING_CREATION_DATE);

        Iterator<String> iterator = settings.keys().iterator();
        while (iterator.hasNext()) {
            String key = iterator.next();
            for (Setting<?> whitelistedSetting : WHITE_LISTED_SETTINGS) {
                if (whitelistedSetting.match(key)) {
                    iterator.remove();
                    break;
                }
            }
        }
        return settings.build();
    }

}
