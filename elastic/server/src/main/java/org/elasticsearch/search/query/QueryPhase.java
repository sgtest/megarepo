/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.search.query;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.lucene.index.IndexReader;
import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.search.BooleanClause;
import org.apache.lucene.search.BooleanQuery;
import org.apache.lucene.search.Collector;
import org.apache.lucene.search.CollectorManager;
import org.apache.lucene.search.FieldDoc;
import org.apache.lucene.search.Query;
import org.apache.lucene.search.ScoreDoc;
import org.apache.lucene.search.ScoreMode;
import org.apache.lucene.search.Sort;
import org.apache.lucene.search.TopDocs;
import org.apache.lucene.search.TotalHits;
import org.apache.lucene.search.Weight;
import org.elasticsearch.common.lucene.Lucene;
import org.elasticsearch.common.lucene.search.TopDocsAndMaxScore;
import org.elasticsearch.common.util.concurrent.EsThreadPoolExecutor;
import org.elasticsearch.common.util.concurrent.TaskExecutionTimeTrackingEsThreadPoolExecutor;
import org.elasticsearch.lucene.queries.SearchAfterSortedDocQuery;
import org.elasticsearch.search.DocValueFormat;
import org.elasticsearch.search.SearchContextSourcePrinter;
import org.elasticsearch.search.SearchService;
import org.elasticsearch.search.aggregations.AggregationPhase;
import org.elasticsearch.search.internal.ContextIndexSearcher;
import org.elasticsearch.search.internal.ScrollContext;
import org.elasticsearch.search.internal.SearchContext;
import org.elasticsearch.search.profile.query.InternalProfileCollector;
import org.elasticsearch.search.profile.query.InternalProfileCollectorManager;
import org.elasticsearch.search.rank.RankSearchContext;
import org.elasticsearch.search.rank.RankShardContext;
import org.elasticsearch.search.rescore.RescorePhase;
import org.elasticsearch.search.sort.SortAndFormats;
import org.elasticsearch.search.suggest.SuggestPhase;
import org.elasticsearch.threadpool.ThreadPool;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ExecutorService;

import static org.elasticsearch.search.internal.SearchContext.TRACK_TOTAL_HITS_DISABLED;
import static org.elasticsearch.search.profile.query.CollectorResult.REASON_SEARCH_QUERY_PHASE;
import static org.elasticsearch.search.query.TopDocsCollectorManagerFactory.createTopDocsCollectorFactory;

/**
 * Query phase of a search request, used to run the query and get back from each shard information about the matching documents
 * (document ids and score or sort criteria) so that matches can be reduced on the coordinating node
 */
public class QueryPhase {
    private static final Logger LOGGER = LogManager.getLogger(QueryPhase.class);

    private QueryPhase() {}

    public static void execute(SearchContext searchContext) throws QueryPhaseExecutionException {
        if (searchContext.rankShardContext() == null) {
            executeQuery(searchContext);
        } else {
            executeRank(searchContext);
        }
    }

    static void executeRank(SearchContext searchContext) throws QueryPhaseExecutionException {
        RankShardContext rankShardContext = searchContext.rankShardContext();
        QuerySearchResult querySearchResult = searchContext.queryResult();

        // run the combined boolean query total hits or aggregations
        // otherwise mark top docs as empty
        if (searchContext.trackTotalHitsUpTo() != TRACK_TOTAL_HITS_DISABLED || searchContext.aggregations() != null) {
            searchContext.size(0);
            QueryPhase.executeQuery(searchContext);
        } else {
            searchContext.queryResult()
                .topDocs(
                    new TopDocsAndMaxScore(new TopDocs(new TotalHits(0, TotalHits.Relation.EQUAL_TO), Lucene.EMPTY_SCORE_DOCS), Float.NaN),
                    new DocValueFormat[0]
                );
        }

        List<TopDocs> rrfRankResults = new ArrayList<>();
        boolean searchTimedOut = querySearchResult.searchTimedOut();
        long serviceTimeEWMA = querySearchResult.serviceTimeEWMA();
        int nodeQueueSize = querySearchResult.nodeQueueSize();

        // run each of the rank queries
        for (Query rankQuery : rankShardContext.queries()) {
            // if a search timeout occurs, exit with partial results
            if (searchTimedOut) {
                break;
            }
            RankSearchContext rankSearchContext = new RankSearchContext(searchContext, rankQuery, rankShardContext.windowSize());
            QueryPhase.addCollectorsAndSearch(rankSearchContext);
            QuerySearchResult rrfQuerySearchResult = rankSearchContext.queryResult();
            rrfRankResults.add(rrfQuerySearchResult.topDocs().topDocs);
            serviceTimeEWMA += rrfQuerySearchResult.serviceTimeEWMA();
            nodeQueueSize = Math.max(nodeQueueSize, rrfQuerySearchResult.nodeQueueSize());
            searchTimedOut = rrfQuerySearchResult.searchTimedOut();
        }

        querySearchResult.setRankShardResult(rankShardContext.combine(rrfRankResults));

        // record values relevant to all queries
        querySearchResult.searchTimedOut(searchTimedOut);
        querySearchResult.serviceTimeEWMA(serviceTimeEWMA);
        querySearchResult.nodeQueueSize(nodeQueueSize);
    }

    static void executeQuery(SearchContext searchContext) throws QueryPhaseExecutionException {
        if (searchContext.hasOnlySuggest()) {
            SuggestPhase.execute(searchContext);
            searchContext.queryResult()
                .topDocs(
                    new TopDocsAndMaxScore(new TopDocs(new TotalHits(0, TotalHits.Relation.EQUAL_TO), Lucene.EMPTY_SCORE_DOCS), Float.NaN),
                    new DocValueFormat[0]
                );
            return;
        }

        if (LOGGER.isTraceEnabled()) {
            LOGGER.trace("{}", new SearchContextSourcePrinter(searchContext));
        }

        // Pre-process aggregations as late as possible. In the case of a DFS_Q_T_F
        // request, preProcess is called on the DFS phase, this is why we pre-process them
        // here to make sure it happens during the QUERY phase
        AggregationPhase.preProcess(searchContext);

        addCollectorsAndSearch(searchContext);

        RescorePhase.execute(searchContext);
        SuggestPhase.execute(searchContext);
        AggregationPhase.execute(searchContext);

        if (searchContext.getProfilers() != null) {
            searchContext.queryResult().profileResults(searchContext.getProfilers().buildQueryPhaseResults());
        }
    }

    /**
     * In a package-private method so that it can be tested without having to
     * wire everything (mapperService, etc.)
     */
    static void addCollectorsAndSearch(SearchContext searchContext) throws QueryPhaseExecutionException {
        final ContextIndexSearcher searcher = searchContext.searcher();
        final IndexReader reader = searcher.getIndexReader();
        QuerySearchResult queryResult = searchContext.queryResult();
        queryResult.searchTimedOut(false);
        try {
            queryResult.from(searchContext.from());
            queryResult.size(searchContext.size());
            Query query = searchContext.rewrittenQuery();
            assert query == searcher.rewrite(query); // already rewritten

            final ScrollContext scrollContext = searchContext.scrollContext();
            if (scrollContext != null) {
                if (scrollContext.totalHits == null) {
                    // first round
                    assert scrollContext.lastEmittedDoc == null;
                    // there is not much that we can optimize here since we want to collect all
                    // documents in order to get the total number of hits

                } else {
                    final ScoreDoc after = scrollContext.lastEmittedDoc;
                    if (canEarlyTerminate(reader, searchContext.sort())) {
                        // now this gets interesting: since the search sort is a prefix of the index sort, we can directly
                        // skip to the desired doc
                        if (after != null) {
                            query = new BooleanQuery.Builder().add(query, BooleanClause.Occur.MUST)
                                .add(new SearchAfterSortedDocQuery(searchContext.sort().sort, (FieldDoc) after), BooleanClause.Occur.FILTER)
                                .build();
                        }
                    }
                }
            }

            final TopDocsCollectorManagerFactory topDocsFactory = createTopDocsCollectorFactory(
                searchContext,
                searchContext.parsedPostFilter() != null || searchContext.minimumScore() != null
            );

            CollectorManager<Collector, Void> topDocsCollectorManager = topDocsFactory.collectorManager();
            if (searchContext.getProfilers() != null) {
                Collector topDocsCollector = topDocsCollectorManager.newCollector();
                InternalProfileCollector profileCollector = new InternalProfileCollector(topDocsCollector, topDocsFactory.profilerName);
                topDocsCollectorManager = new InternalProfileCollectorManager(profileCollector);
            }

            Collector topDocsCollector = topDocsCollectorManager.newCollector();
            Collector aggsCollector = null;
            Weight postFilterWeight = null;
            if (searchContext.aggregations() != null) {
                aggsCollector = searchContext.aggregations().getAggsCollectorManager().newCollector();
            }
            if (searchContext.parsedPostFilter() != null) {
                postFilterWeight = searcher.createWeight(
                    searcher.rewrite(searchContext.parsedPostFilter().query()),
                    ScoreMode.COMPLETE_NO_SCORES,
                    1f
                );
            }
            QueryPhaseCollector queryPhaseCollector = new QueryPhaseCollector(
                topDocsCollector,
                postFilterWeight,
                searchContext.terminateAfter(),
                aggsCollector,
                searchContext.minimumScore()
            );

            SingleThreadCollectorManager collectorManager;
            if (searchContext.getProfilers() == null) {
                collectorManager = new SingleThreadCollectorManager(queryPhaseCollector);
            } else {
                InternalProfileCollector profileCollector;
                if (aggsCollector == null) {
                    profileCollector = new InternalProfileCollector(
                        queryPhaseCollector,
                        REASON_SEARCH_QUERY_PHASE,
                        (InternalProfileCollector) topDocsCollector
                    );
                } else {
                    profileCollector = new InternalProfileCollector(
                        queryPhaseCollector,
                        REASON_SEARCH_QUERY_PHASE,
                        (InternalProfileCollector) topDocsCollector,
                        (InternalProfileCollector) aggsCollector
                    );
                }
                collectorManager = new InternalProfileCollectorManager(profileCollector);
            }

            final Runnable timeoutRunnable = getTimeoutCheck(searchContext);
            if (timeoutRunnable != null) {
                searcher.addQueryCancellation(timeoutRunnable);
            }

            try {
                searchWithCollectorManager(searchContext, searcher, query, collectorManager, timeoutRunnable != null);
                if (queryPhaseCollector.isTerminatedAfter()) {
                    queryResult.terminatedEarly(true);
                }
                queryResult.topDocs(topDocsFactory.topDocsAndMaxScore(), topDocsFactory.sortValueFormats);
                ExecutorService executor = searchContext.indexShard().getThreadPool().executor(ThreadPool.Names.SEARCH);
                assert executor instanceof TaskExecutionTimeTrackingEsThreadPoolExecutor
                    || (executor instanceof EsThreadPoolExecutor == false /* in case thread pool is mocked out in tests */)
                    : "SEARCH threadpool should have an executor that exposes EWMA metrics, but is of type " + executor.getClass();
                if (executor instanceof TaskExecutionTimeTrackingEsThreadPoolExecutor rExecutor) {
                    queryResult.nodeQueueSize(rExecutor.getCurrentQueueSize());
                    queryResult.serviceTimeEWMA((long) rExecutor.getTaskExecutionEWMA());
                }
            } finally {
                // Search phase has finished, no longer need to check for timeout
                // otherwise aggregation phase might get cancelled.
                if (timeoutRunnable != null) {
                    searcher.removeQueryCancellation(timeoutRunnable);
                }
            }
        } catch (Exception e) {
            throw new QueryPhaseExecutionException(searchContext.shardTarget(), "Failed to execute main query", e);
        }
    }

    private static void searchWithCollectorManager(
        SearchContext searchContext,
        ContextIndexSearcher searcher,
        Query query,
        CollectorManager<Collector, Void> collectorManager,
        boolean timeoutSet
    ) throws IOException {
        if (searchContext.getProfilers() != null) {
            searchContext.getProfilers()
                .getCurrentQueryProfiler()
                .setCollectorManager(((InternalProfileCollectorManager) collectorManager)::getCollectorTree);
        }
        QuerySearchResult queryResult = searchContext.queryResult();
        try {
            searcher.search(query, collectorManager);
        } catch (TimeExceededException e) {
            assert timeoutSet : "TimeExceededException thrown even though timeout wasn't set";
            if (searchContext.request().allowPartialSearchResults() == false) {
                // Can't rethrow TimeExceededException because not serializable
                throw new QueryPhaseExecutionException(searchContext.shardTarget(), "Time exceeded");
            }
            queryResult.searchTimedOut(true);
        }
        if (searchContext.terminateAfter() != SearchContext.DEFAULT_TERMINATE_AFTER && queryResult.terminatedEarly() == null) {
            queryResult.terminatedEarly(false);
        }
    }

    /**
     * Returns whether collection within the provided <code>reader</code> can be early-terminated if it sorts
     * with <code>sortAndFormats</code>.
     **/
    private static boolean canEarlyTerminate(IndexReader reader, SortAndFormats sortAndFormats) {
        if (sortAndFormats == null || sortAndFormats.sort == null) {
            return false;
        }
        final Sort sort = sortAndFormats.sort;
        for (LeafReaderContext ctx : reader.leaves()) {
            Sort indexSort = ctx.reader().getMetaData().getSort();
            if (indexSort == null || Lucene.canEarlyTerminate(sort, indexSort) == false) {
                return false;
            }
        }
        return true;
    }

    public static Runnable getTimeoutCheck(SearchContext searchContext) {
        boolean timeoutSet = searchContext.scrollContext() == null
            && searchContext.timeout() != null
            && searchContext.timeout().equals(SearchService.NO_TIMEOUT) == false;

        if (timeoutSet) {
            final long startTime = searchContext.getRelativeTimeInMillis();
            final long timeout = searchContext.timeout().millis();
            final long maxTime = startTime + timeout;
            return () -> {
                final long time = searchContext.getRelativeTimeInMillis();
                if (time > maxTime) {
                    throw new TimeExceededException();
                }
            };
        } else {
            return null;
        }
    }

    private static class TimeExceededException extends RuntimeException {

        @Override
        public Throwable fillInStackTrace() {
            // never re-thrown so we can save the expensive stacktrace
            return this;
        }
    }
}
