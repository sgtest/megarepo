/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.search.aggregations.support;

import org.apache.lucene.analysis.Analyzer;
import org.apache.lucene.search.IndexSearcher;
import org.apache.lucene.search.Query;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.breaker.CircuitBreaker;
import org.elasticsearch.common.breaker.PreallocatedCircuitBreakerService;
import org.elasticsearch.common.lease.Releasable;
import org.elasticsearch.common.lease.Releasables;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.analysis.NamedAnalyzer;
import org.elasticsearch.index.cache.bitset.BitsetFilterCache;
import org.elasticsearch.index.fielddata.IndexFieldData;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.index.mapper.ObjectMapper;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.index.query.Rewriteable;
import org.elasticsearch.index.query.support.NestedScope;
import org.elasticsearch.script.Script;
import org.elasticsearch.script.ScriptContext;
import org.elasticsearch.search.aggregations.Aggregator;
import org.elasticsearch.search.aggregations.MultiBucketConsumerService.MultiBucketConsumer;
import org.elasticsearch.search.internal.SubSearchContext;
import org.elasticsearch.search.lookup.SearchLookup;
import org.elasticsearch.search.profile.aggregation.AggregationProfiler;
import org.elasticsearch.search.profile.aggregation.ProfilingAggregator;
import org.elasticsearch.search.sort.BucketedSort;
import org.elasticsearch.search.sort.SortAndFormats;
import org.elasticsearch.search.sort.SortBuilder;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import java.util.function.Function;
import java.util.function.LongSupplier;
import java.util.function.Supplier;

/**
 * Everything used to build and execute aggregations and the
 * {@link ValuesSource data sources} that power them.
 * <p>
 * In production we always use the {@link ProductionAggregationContext} but
 * this is {@code abstract} so that tests can build it without creating the
 * massing {@link SearchExecutionContext}.
 * <p>
 * {@linkplain AggregationContext}s are {@link Releasable} because they track
 * the {@link Aggregator}s they build and {@link Aggregator#close} them when
 * the request is done. {@linkplain AggregationContext} may also preallocate
 * bytes on the "REQUEST" breaker and is responsible for releasing those bytes.
 */
public abstract class AggregationContext implements Releasable {
    /**
     * The query at the top level of the search in which these aggregations are running.
     */
    public abstract Query query();

    /**
     * Wrap the aggregator for profiling if profiling is enabled.
     */
    public abstract Aggregator profileIfEnabled(Aggregator agg) throws IOException;

    /**
     * Are we profiling the aggregation?
     */
    public abstract boolean profiling();

    /**
     * The time in milliseconds that is shared across all resources involved. Even across shards and nodes.
     */
    public abstract long nowInMillis();

    /**
     * Lookup the context for a field.
     */
    public final FieldContext buildFieldContext(String field) {
        MappedFieldType ft = getFieldType(field);
        if (ft == null) {
            // The field is unmapped
            return null;
        }
        return new FieldContext(field, buildFieldData(ft), ft);
    }

    /**
     * Lookup the context for an already resolved field type.
     */
    public final FieldContext buildFieldContext(MappedFieldType ft) {
        return new FieldContext(ft.name(), buildFieldData(ft), ft);
    }

    /**
     * Build field data.
     */
    protected abstract IndexFieldData<?> buildFieldData(MappedFieldType ft);

    /**
     * Lookup a {@link MappedFieldType} by path.
     */
    public abstract MappedFieldType getFieldType(String path);

    /**
     * Returns true if the field identified by the provided name is mapped, false otherwise
     */
    public abstract boolean isFieldMapped(String field);

    /**
     * Compile a script.
     */
    public abstract <FactoryType> FactoryType compile(Script script, ScriptContext<FactoryType> context);

    /**
     * Fetch the shared {@link SearchLookup}.
     */
    public abstract SearchLookup lookup();

    /**
     * The {@link ValuesSourceRegistry} to resolve {@link Aggregator}s and the like.
     */
    public abstract ValuesSourceRegistry getValuesSourceRegistry();

    /**
     * The {@link AggregationUsageService} used to track which aggregations are
     * actually used.
     */
    public final AggregationUsageService getUsageService() {
        return getValuesSourceRegistry().getUsageService();
    }

    /**
     * Utility to share and track large arrays.
     */
    public abstract BigArrays bigArrays();

    /**
     * The searcher that will execute this query.
     */
    public abstract IndexSearcher searcher();

    /**
     * Build a query.
     */
    public abstract Query buildQuery(QueryBuilder builder) throws IOException;

    /**
     * The settings for the index against which this search is running.
     */
    public abstract IndexSettings getIndexSettings();

    /**
     * Compile a sort.
     */
    public abstract Optional<SortAndFormats> buildSort(List<SortBuilder<?>> sortBuilders) throws IOException;

    /**
     * Find an {@link ObjectMapper}.
     */
    public abstract ObjectMapper getObjectMapper(String path);

    /**
     * Access the nested scope. Stay away from this unless you are dealing with nested.
     */
    public abstract NestedScope nestedScope();

    /**
     * Build a {@linkplain SubSearchContext} to power an aggregation fetching top hits.
     * Try to avoid using this because it pulls in a ton of dependencies.
     */
    public abstract SubSearchContext subSearchContext();

    /**
     * Cause this aggregation to be released when the search is finished.
     */
    public abstract void addReleasable(Aggregator aggregator);

    public abstract MultiBucketConsumer multiBucketConsumer();

    /**
     * Get the filter cache.
     */
    public abstract BitsetFilterCache bitsetFilterCache();
    // TODO it is unclear why we can't just use the IndexSearcher which already caches

    /**
     * Build a collector for sorted values specialized for aggregations.
     */
    public abstract BucketedSort buildBucketedSort(SortBuilder<?> sort, int size, BucketedSort.ExtraData values) throws IOException;

    /**
     * Get a deterministic random seed based for this particular shard.
     */
    public abstract int shardRandomSeed();

    /**
     * How many millis have passed since we started the search?
     */
    public abstract long getRelativeTimeInMillis();

    /**
     * Has the search been cancelled?
     * <p>
     * This'll require a {@code volatile} read.
     */
    public abstract boolean isCancelled();

    /**
     * The circuit breaker used to account for aggs.
     */
    public abstract CircuitBreaker breaker();

    /**
     * Return the index-time analyzer for the current index
     * @param unindexedFieldAnalyzer    a function that builds an analyzer for unindexed fields
     */
    public abstract Analyzer getIndexAnalyzer(Function<String, NamedAnalyzer> unindexedFieldAnalyzer);

    /**
     * Is this request cacheable? Requests that have
     * non-deterministic queries or scripts aren't cachable.
     */
    public abstract boolean isCacheable();

    /**
     * Implementation of {@linkplain AggregationContext} for production usage
     * that wraps our ubiquitous {@link SearchExecutionContext} and anything else
     * specific to aggregations. Unit tests should generally avoid using this
     * because it requires a <strong>huge</strong> portion of a real
     * Elasticsearch node.
     */
    public static class ProductionAggregationContext extends AggregationContext {
        private final SearchExecutionContext context;
        private final PreallocatedCircuitBreakerService preallocatedBreakerService;
        private final BigArrays bigArrays;
        private final Supplier<Query> topLevelQuery;
        private final AggregationProfiler profiler;
        private final MultiBucketConsumer multiBucketConsumer;
        private final Supplier<SubSearchContext> subSearchContextBuilder;
        private final BitsetFilterCache bitsetFilterCache;
        private final int randomSeed;
        private final LongSupplier relativeTimeInMillis;
        private final Supplier<Boolean> isCancelled;

        private final List<Aggregator> releaseMe = new ArrayList<>();

        public ProductionAggregationContext(
            SearchExecutionContext context,
            BigArrays bigArrays,
            long bytesToPreallocate,
            Supplier<Query> topLevelQuery,
            @Nullable AggregationProfiler profiler,
            MultiBucketConsumer multiBucketConsumer,
            Supplier<SubSearchContext> subSearchContextBuilder,
            BitsetFilterCache bitsetFilterCache,
            int randomSeed,
            LongSupplier relativeTimeInMillis,
            Supplier<Boolean> isCancelled
        ) {
            this.context = context;
            if (bytesToPreallocate == 0) {
                /*
                 * Its possible if a bit strange for the aggregations to ask
                 * to preallocate 0 bytes. Mostly this is for testing other
                 * things, but we should honor it and just not preallocate
                 * anything. Setting the breakerService reference to null will
                 * cause us to skip it when we close this context.
                 */
                this.preallocatedBreakerService = null;
                this.bigArrays = bigArrays.withCircuitBreaking();
            } else {
                this.preallocatedBreakerService = new PreallocatedCircuitBreakerService(
                    bigArrays.breakerService(),
                    CircuitBreaker.REQUEST,
                    bytesToPreallocate,
                    "aggregations"
                );
                this.bigArrays = bigArrays.withBreakerService(preallocatedBreakerService).withCircuitBreaking();
            }
            this.topLevelQuery = topLevelQuery;
            this.profiler = profiler;
            this.multiBucketConsumer = multiBucketConsumer;
            this.subSearchContextBuilder = subSearchContextBuilder;
            this.bitsetFilterCache = bitsetFilterCache;
            this.randomSeed = randomSeed;
            this.relativeTimeInMillis = relativeTimeInMillis;
            this.isCancelled = isCancelled;
        }

        @Override
        public Query query() {
            return topLevelQuery.get();
        }

        @Override
        public Aggregator profileIfEnabled(Aggregator agg) throws IOException {
            if (profiler == null) {
                return agg;
            }
            return new ProfilingAggregator(agg, profiler);
        }

        @Override
        public boolean profiling() {
            return profiler != null;
        }

        @Override
        public long nowInMillis() {
            return context.nowInMillis();
        }

        @Override
        protected IndexFieldData<?> buildFieldData(MappedFieldType ft) {
            return context.getForField(ft);
        }

        @Override
        public MappedFieldType getFieldType(String path) {
            return context.getFieldType(path);
        }

        @Override
        public boolean isFieldMapped(String field) {
            return context.isFieldMapped(field);
        }

        @Override
        public <FactoryType> FactoryType compile(Script script, ScriptContext<FactoryType> scriptContext) {
            return context.compile(script, scriptContext);
        }

        @Override
        public SearchLookup lookup() {
            return context.lookup();
        }

        @Override
        public ValuesSourceRegistry getValuesSourceRegistry() {
            return context.getValuesSourceRegistry();
        }

        @Override
        public BigArrays bigArrays() {
            return bigArrays;
        }

        @Override
        public IndexSearcher searcher() {
            return context.searcher();
        }

        @Override
        public Query buildQuery(QueryBuilder builder) throws IOException {
            return Rewriteable.rewrite(builder, context, true).toQuery(context);
        }

        @Override
        public IndexSettings getIndexSettings() {
            return context.getIndexSettings();
        }

        @Override
        public Optional<SortAndFormats> buildSort(List<SortBuilder<?>> sortBuilders) throws IOException {
            return SortBuilder.buildSort(sortBuilders, context);
        }

        @Override
        public ObjectMapper getObjectMapper(String path) {
            return context.getObjectMapper(path);
        }

        @Override
        public NestedScope nestedScope() {
            return context.nestedScope();
        }

        @Override
        public SubSearchContext subSearchContext() {
            return subSearchContextBuilder.get();
        }

        @Override
        public void addReleasable(Aggregator aggregator) {
            releaseMe.add(aggregator);
        }

        @Override
        public MultiBucketConsumer multiBucketConsumer() {
            return multiBucketConsumer;
        }

        @Override
        public BitsetFilterCache bitsetFilterCache() {
            return bitsetFilterCache;
        }

        @Override
        public BucketedSort buildBucketedSort(SortBuilder<?> sort, int bucketSize, BucketedSort.ExtraData extra) throws IOException {
            return sort.buildBucketedSort(context, bigArrays, bucketSize, extra);
        }

        @Override
        public int shardRandomSeed() {
            return randomSeed;
        }

        @Override
        public long getRelativeTimeInMillis() {
            return relativeTimeInMillis.getAsLong();
        }

        @Override
        public boolean isCancelled() {
            return isCancelled.get();
        }

        @Override
        public CircuitBreaker breaker() {
            // preallocatedBreakerService may be null if we haven't preallocated so use the one in bigArrays.
            return bigArrays.breakerService().getBreaker(CircuitBreaker.REQUEST);
        }

        @Override
        public Analyzer getIndexAnalyzer(Function<String, NamedAnalyzer> unindexedFieldAnalyzer) {
            return context.getIndexAnalyzer(unindexedFieldAnalyzer);
        }

        @Override
        public boolean isCacheable() {
            return context.isCacheable();
        }

        @Override
        public void close() {
            /*
             * Add the breakerService to the end of the list so we release it
             * after all the aggregations that allocate bytes on it.
             */
            List<Releasable> releaseMe = new ArrayList<>(this.releaseMe);
            releaseMe.add(preallocatedBreakerService);
            Releasables.close(releaseMe);
        }
    }
}
