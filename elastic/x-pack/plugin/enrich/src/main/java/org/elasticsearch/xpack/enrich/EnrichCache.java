/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.enrich;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.cluster.metadata.IndexAbstraction;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.common.cache.Cache;
import org.elasticsearch.common.cache.CacheBuilder;
import org.elasticsearch.xpack.core.enrich.action.EnrichStatsAction;

import java.util.Objects;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutionException;
import java.util.function.BiConsumer;

import static org.elasticsearch.action.ActionListener.wrap;

/**
 * A simple cache for enrich that uses {@link Cache}. There is one instance of this cache and
 * multiple enrich processors with different policies will use this cache.
 *
 * The key of the cache is based on the search request and the enrich index that will be used.
 * Search requests that enrich generates target the alias for an enrich policy, this class
 * resolves the alias to the actual enrich index and uses that for the cache key. This way
 * no stale entries will be returned if a policy execution happens and a new enrich index is created.
 *
 * There is no cleanup mechanism of stale entries in case a new enrich index is created
 * as part of a policy execution. This shouldn't be needed as cache entries for prior enrich
 * indices will be eventually evicted, because these entries will not end up being used. The
 * latest enrich index name will be used as cache key after an enrich policy execution.
 * (Also a cleanup mechanism also wouldn't be straightforward to implement,
 * since there is no easy check to see that an enrich index used as cache key no longer is the
 * current enrich index the enrich alias of an policy refers to. It would require checking
 * all cached entries on each cluster state update)
 */
public class EnrichCache {

    protected final Cache<CacheKey, CompletableFuture<SearchResponse>> cache;
    private volatile Metadata metadata;

    EnrichCache(long maxSize) {
        this.cache = CacheBuilder.<CacheKey, CompletableFuture<SearchResponse>>builder().setMaximumWeight(maxSize).build();
    }

    /**
     * Get the value from the cache if present. Returns immediately.
     * See {@link #resolveOrDispatchSearch(SearchRequest, BiConsumer, BiConsumer)} to implement a read-through, possibly async interaction.
     * @param searchRequest the key
     * @return the cached value or null
     */
    CompletableFuture<SearchResponse> get(SearchRequest searchRequest) {
        CacheKey cacheKey = toKey(searchRequest);
        return cache.get(cacheKey);
    }

    void setMetadata(Metadata metadata) {
        this.metadata = metadata;
    }

    public EnrichStatsAction.Response.CacheStats getStats(String localNodeId) {
        Cache.CacheStats cacheStats = cache.stats();
        return new EnrichStatsAction.Response.CacheStats(
            localNodeId,
            cache.count(),
            cacheStats.getHits(),
            cacheStats.getMisses(),
            cacheStats.getEvictions()
        );
    }

    /**
     * resolves the entry from the cache and provides reports the result to the `callBack` This method does not dispatch any logic
     * to another thread. Under contention the searchDispatcher is only called once when the value is not in the cache. The
     * searchDispatcher should schedule the search / callback _asynchronously_ because if the searchDispatcher blocks, then this
     * method will block. The callback is call on the thread calling this method or under cache miss and contention, the thread running
     * the part of the searchDispatcher that calls the callback.
     * @param searchRequest the cache key and input for the search dispatcher
     * @param searchDispatcher the logical block to be called on cache miss
     * @param callBack the callback which gets the value asynchronously, which could be a searchResponse or exception (negative lookup)
     */
    public void resolveOrDispatchSearch(
        SearchRequest searchRequest,
        BiConsumer<SearchRequest, ActionListener<SearchResponse>> searchDispatcher,
        BiConsumer<SearchResponse, Exception> callBack
    ) {
        CacheKey cacheKey = toKey(searchRequest);
        try {
            CompletableFuture<SearchResponse> cacheEntry = cache.computeIfAbsent(cacheKey, request -> {
                CompletableFuture<SearchResponse> completableFuture = new CompletableFuture<>();
                searchDispatcher.accept(searchRequest, wrap(completableFuture::complete, completableFuture::completeExceptionally));
                return completableFuture;
            });
            cacheEntry.whenComplete((response, throwable) -> {
                if (throwable != null) {
                    // Don't cache failures
                    cache.invalidate(cacheKey, cacheEntry);
                    if (throwable instanceof Exception) {
                        callBack.accept(response, (Exception) throwable);
                        return;
                    }
                    // Let ElasticsearchUncaughtExceptionHandler handle this, which should halt Elasticsearch
                    throw (Error) throwable;
                }
                callBack.accept(response, null);
            });
        } catch (ExecutionException e) {
            callBack.accept(null, e);
        }
    }

    protected CacheKey toKey(SearchRequest searchRequest) {
        String enrichIndex = getEnrichIndexKey(searchRequest);
        return new CacheKey(enrichIndex, searchRequest);
    }

    private String getEnrichIndexKey(SearchRequest searchRequest) {
        String alias = searchRequest.indices()[0];
        IndexAbstraction ia = metadata.getIndicesLookup().get(alias);
        return ia.getIndices().get(0).getIndex().getName();
    }

    private static class CacheKey {

        final String enrichIndex;
        final SearchRequest searchRequest;

        private CacheKey(String enrichIndex, SearchRequest searchRequest) {
            this.enrichIndex = enrichIndex;
            this.searchRequest = searchRequest;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            CacheKey cacheKey = (CacheKey) o;
            return enrichIndex.equals(cacheKey.enrichIndex) && searchRequest.equals(cacheKey.searchRequest);
        }

        @Override
        public int hashCode() {
            return Objects.hash(enrichIndex, searchRequest);
        }
    }

}
