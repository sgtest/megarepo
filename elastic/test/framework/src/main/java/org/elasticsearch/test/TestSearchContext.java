/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.test;

import org.apache.lucene.search.Collector;
import org.apache.lucene.search.FieldDoc;
import org.apache.lucene.search.Query;
import org.elasticsearch.action.OriginalIndices;
import org.elasticsearch.action.search.SearchShardTask;
import org.elasticsearch.action.search.SearchType;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.index.IndexService;
import org.elasticsearch.index.cache.bitset.BitsetFilterCache;
import org.elasticsearch.index.query.ParsedQuery;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.index.shard.IndexShard;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.search.SearchExtBuilder;
import org.elasticsearch.search.SearchShardTarget;
import org.elasticsearch.search.aggregations.SearchContextAggregations;
import org.elasticsearch.search.collapse.CollapseContext;
import org.elasticsearch.search.dfs.DfsSearchResult;
import org.elasticsearch.search.fetch.FetchPhase;
import org.elasticsearch.search.fetch.FetchSearchResult;
import org.elasticsearch.search.fetch.StoredFieldsContext;
import org.elasticsearch.search.fetch.subphase.FetchDocValuesContext;
import org.elasticsearch.search.fetch.subphase.FetchFieldsContext;
import org.elasticsearch.search.fetch.subphase.FetchSourceContext;
import org.elasticsearch.search.fetch.subphase.ScriptFieldsContext;
import org.elasticsearch.search.fetch.subphase.highlight.SearchHighlightContext;
import org.elasticsearch.search.internal.ContextIndexSearcher;
import org.elasticsearch.search.internal.ReaderContext;
import org.elasticsearch.search.internal.ScrollContext;
import org.elasticsearch.search.internal.SearchContext;
import org.elasticsearch.search.internal.ShardSearchContextId;
import org.elasticsearch.search.internal.ShardSearchRequest;
import org.elasticsearch.search.profile.Profilers;
import org.elasticsearch.search.query.QuerySearchResult;
import org.elasticsearch.search.rescore.RescoreContext;
import org.elasticsearch.search.sort.SortAndFormats;
import org.elasticsearch.search.suggest.SuggestionSearchContext;

import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static java.util.Collections.emptyMap;

public class TestSearchContext extends SearchContext {
    public static final SearchShardTarget SHARD_TARGET =
        new SearchShardTarget("test", new ShardId("test", "test", 0), null, OriginalIndices.NONE);

    final IndexService indexService;
    final BitsetFilterCache fixedBitSetFilterCache;
    final Map<Class<?>, Collector> queryCollectors = new HashMap<>();
    final IndexShard indexShard;
    final QuerySearchResult queryResult = new QuerySearchResult();
    final SearchExecutionContext searchExecutionContext;
    ParsedQuery originalQuery;
    ParsedQuery postFilter;
    Query query;
    Float minScore;
    SearchShardTask task;
    SortAndFormats sort;
    boolean trackScores = false;
    int trackTotalHitsUpTo = SearchContext.DEFAULT_TRACK_TOTAL_HITS_UP_TO;

    ContextIndexSearcher searcher;
    int from;
    int size;
    private int terminateAfter = DEFAULT_TERMINATE_AFTER;
    private SearchContextAggregations aggregations;
    private ScrollContext scrollContext;

    private final Map<String, SearchExtBuilder> searchExtBuilders = new HashMap<>();

    public TestSearchContext(IndexService indexService) {
        this.indexService = indexService;
        this.fixedBitSetFilterCache = indexService.cache().bitsetFilterCache();
        this.indexShard = indexService.getShardOrNull(0);
        searchExecutionContext = indexService.newSearchExecutionContext(0, 0, null, () -> 0L, null, emptyMap());
    }

    public TestSearchContext(SearchExecutionContext searchExecutionContext) {
        this(searchExecutionContext, null, null, null);
    }

    public TestSearchContext(SearchExecutionContext searchExecutionContext, IndexShard indexShard, ContextIndexSearcher searcher) {
        this(searchExecutionContext, indexShard, searcher, null);
    }

    public TestSearchContext(SearchExecutionContext searchExecutionContext, IndexShard indexShard,
                             ContextIndexSearcher searcher, ScrollContext scrollContext) {
        this.indexService = null;
        this.fixedBitSetFilterCache = null;
        this.indexShard = indexShard;
        this.searchExecutionContext = searchExecutionContext;
        this.searcher = searcher;
        this.scrollContext = scrollContext;
    }

    public void setSearcher(ContextIndexSearcher searcher) {
        this.searcher = searcher;
    }

    @Override
    public void preProcess(boolean rewrite) {
    }

    @Override
    public Query buildFilteredQuery(Query query) {
        return null;
    }

    @Override
    public ShardSearchContextId id() {
        return new ShardSearchContextId("", 0);
    }

    @Override
    public String source() {
        return null;
    }

    @Override
    public ShardSearchRequest request() {
        return null;
    }

    @Override
    public SearchType searchType() {
        return null;
    }

    @Override
    public SearchShardTarget shardTarget() {
        return null;
    }

    @Override
    public int numberOfShards() {
        return 1;
    }

    @Override
    public ScrollContext scrollContext() {
        return scrollContext;
    }

    @Override
    public SearchContextAggregations aggregations() {
        return aggregations;
    }

    @Override
    public SearchContext aggregations(SearchContextAggregations aggregations) {
        this.aggregations = aggregations;
        return this;
    }

    @Override
    public void addSearchExt(SearchExtBuilder searchExtBuilder) {
        searchExtBuilders.put(searchExtBuilder.getWriteableName(), searchExtBuilder);
    }

    @Override
    public SearchExtBuilder getSearchExt(String name) {
        return searchExtBuilders.get(name);
    }

    @Override
    public SearchHighlightContext highlight() {
        return null;
    }

    @Override
    public void highlight(SearchHighlightContext highlight) {
    }

    @Override
    public SuggestionSearchContext suggest() {
        return null;
    }

    @Override
    public void suggest(SuggestionSearchContext suggest) {
    }

    @Override
    public List<RescoreContext> rescore() {
        return Collections.emptyList();
    }

    @Override
    public boolean hasScriptFields() {
        return false;
    }

    @Override
    public ScriptFieldsContext scriptFields() {
        return null;
    }

    @Override
    public boolean sourceRequested() {
        return false;
    }

    @Override
    public boolean hasFetchSourceContext() {
        return false;
    }

    @Override
    public FetchSourceContext fetchSourceContext() {
        return null;
    }

    @Override
    public SearchContext fetchSourceContext(FetchSourceContext fetchSourceContext) {
        return null;
    }

    @Override
    public FetchDocValuesContext docValuesContext() {
        return null;
    }

    @Override
    public SearchContext docValuesContext(FetchDocValuesContext docValuesContext) {
        return null;
    }

    @Override
    public FetchFieldsContext fetchFieldsContext() {
        return null;
    }

    @Override
    public SearchContext fetchFieldsContext(FetchFieldsContext fetchFieldsContext) {
        return null;
    }

    @Override
    public ContextIndexSearcher searcher() {
        return searcher;
    }

    @Override
    public IndexShard indexShard() {
        return indexShard;
    }

    @Override
    public BitsetFilterCache bitsetFilterCache() {
        return fixedBitSetFilterCache;
    }

    @Override
    public TimeValue timeout() {
        return TimeValue.ZERO;
    }

    @Override
    public void timeout(TimeValue timeout) {
    }

    @Override
    public int terminateAfter() {
        return terminateAfter;
    }

    @Override
    public void terminateAfter(int terminateAfter) {
        this.terminateAfter = terminateAfter;
    }

    @Override
    public boolean lowLevelCancellation() {
        return false;
    }

    @Override
    public SearchContext minimumScore(float minimumScore) {
        this.minScore = minimumScore;
        return this;
    }

    @Override
    public Float minimumScore() {
        return minScore;
    }

    @Override
    public SearchContext sort(SortAndFormats sort) {
        this.sort = sort;
        return this;
    }

    @Override
    public SortAndFormats sort() {
        return sort;
    }

    @Override
    public SearchContext trackScores(boolean trackScores) {
        this.trackScores = trackScores;
        return this;
    }

    @Override
    public boolean trackScores() {
        return trackScores;
    }

    @Override
    public SearchContext trackTotalHitsUpTo(int trackTotalHitsUpTo) {
        this.trackTotalHitsUpTo = trackTotalHitsUpTo;
        return this;
    }

    @Override
    public int trackTotalHitsUpTo() {
        return trackTotalHitsUpTo;
    }

    @Override
    public SearchContext searchAfter(FieldDoc searchAfter) {
        return null;
    }

    @Override
    public FieldDoc searchAfter() {
        return null;
    }

    @Override
    public SearchContext collapse(CollapseContext collapse) {
        return null;
    }

    @Override
    public CollapseContext collapse() {
        return null;
    }

    @Override
    public SearchContext parsedPostFilter(ParsedQuery postFilter) {
        this.postFilter = postFilter;
        return this;
    }

    @Override
    public ParsedQuery parsedPostFilter() {
        return postFilter;
    }

    @Override
    public SearchContext parsedQuery(ParsedQuery query) {
        this.originalQuery = query;
        this.query = query.query();
        return this;
    }

    @Override
    public ParsedQuery parsedQuery() {
        return originalQuery;
    }

    @Override
    public Query query() {
        return query;
    }

    @Override
    public int from() {
        return from;
    }

    @Override
    public SearchContext from(int from) {
        this.from = from;
        return this;
    }

    @Override
    public int size() {
        return size;
    }

    public void setSize(int size) {
        this.size = size;
    }


    @Override
    public SearchContext size(int size) {
        return null;
    }

    @Override
    public boolean hasStoredFields() {
        return false;
    }

    @Override
    public StoredFieldsContext storedFieldsContext() {
        return null;
    }

    @Override
    public SearchContext storedFieldsContext(StoredFieldsContext storedFieldsContext) {
        return null;
    }

    @Override
    public boolean explain() {
        return false;
    }

    @Override
    public void explain(boolean explain) {
    }

    @Override
    public List<String> groupStats() {
        return null;
    }

    @Override
    public void groupStats(List<String> groupStats) {
    }

    @Override
    public boolean version() {
        return false;
    }

    @Override
    public void version(boolean version) {
    }

    @Override
    public boolean seqNoAndPrimaryTerm() {
        return false;
    }

    @Override
    public void seqNoAndPrimaryTerm(boolean seqNoAndPrimaryTerm) {

    }

    @Override
    public int[] docIdsToLoad() {
        return new int[0];
    }

    @Override
    public int docIdsToLoadSize() {
        return 0;
    }

    @Override
    public SearchContext docIdsToLoad(int[] docIdsToLoad, int docsIdsToLoadSize) {
        return null;
    }

    @Override
    public DfsSearchResult dfsResult() {
        return null;
    }

    @Override
    public QuerySearchResult queryResult() {
        return queryResult;
    }

    @Override
    public FetchSearchResult fetchResult() {
        return null;
    }

    @Override
    public FetchPhase fetchPhase() {
        return null;
    }

    @Override
    public long getRelativeTimeInMillis() {
        return 0L;
    }

    @Override
    public Profilers getProfilers() {
        return null; // no profiling
    }

    @Override
    public Map<Class<?>, Collector> queryCollectors() {return queryCollectors;}

    @Override
    public SearchExecutionContext getSearchExecutionContext() {
        return searchExecutionContext;
    }

    @Override
    public void setTask(SearchShardTask task) {
        this.task = task;
    }

    @Override
    public SearchShardTask getTask() {
        return task;
    }

    @Override
    public boolean isCancelled() {
        return task.isCancelled();
    }

    @Override
    public void addRescore(RescoreContext rescore) {

    }

    @Override
    public ReaderContext readerContext() {
        throw new UnsupportedOperationException();
    }
}
