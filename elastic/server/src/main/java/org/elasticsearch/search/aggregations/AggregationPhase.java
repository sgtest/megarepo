/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.search.aggregations;

import org.apache.lucene.search.Collector;
import org.apache.lucene.search.Query;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.lucene.search.Queries;
import org.elasticsearch.search.aggregations.bucket.global.GlobalAggregator;
import org.elasticsearch.search.internal.SearchContext;
import org.elasticsearch.search.profile.query.CollectorResult;
import org.elasticsearch.search.profile.query.InternalProfileCollector;
import org.elasticsearch.search.query.QueryPhaseExecutionException;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * Aggregation phase of a search request, used to collect aggregations
 */
public class AggregationPhase {

    @Inject
    public AggregationPhase() {
    }

    public void preProcess(SearchContext context) {
        if (context.aggregations() != null) {
            List<Aggregator> collectors = new ArrayList<>();
            Aggregator[] aggregators;
            try {
                aggregators = context.aggregations().factories().createTopLevelAggregators();
                for (int i = 0; i < aggregators.length; i++) {
                    if (aggregators[i] instanceof GlobalAggregator == false) {
                        collectors.add(aggregators[i]);
                    }
                }
                context.aggregations().aggregators(aggregators);
                if (collectors.isEmpty() == false) {
                    Collector collector = MultiBucketCollector.wrap(collectors);
                    ((BucketCollector)collector).preCollection();
                    if (context.getProfilers() != null) {
                        collector = new InternalProfileCollector(collector, CollectorResult.REASON_AGGREGATION,
                                // TODO: report on child aggs as well
                                Collections.emptyList());
                    }
                    context.queryCollectors().put(AggregationPhase.class, collector);
                }
            } catch (IOException e) {
                throw new AggregationInitializationException("Could not initialize aggregators", e);
            }
        }
    }

    public void execute(SearchContext context) {
        if (context.aggregations() == null) {
            context.queryResult().aggregations(null);
            return;
        }

        if (context.queryResult().hasAggs()) {
            // no need to compute the aggs twice, they should be computed on a per context basis
            return;
        }

        Aggregator[] aggregators = context.aggregations().aggregators();
        List<Aggregator> globals = new ArrayList<>();
        for (int i = 0; i < aggregators.length; i++) {
            if (aggregators[i] instanceof GlobalAggregator) {
                globals.add(aggregators[i]);
            }
        }

        // optimize the global collector based execution
        if (globals.isEmpty() == false) {
            BucketCollector globalsCollector = MultiBucketCollector.wrap(globals);
            Query query = context.buildFilteredQuery(Queries.newMatchAllQuery());

            try {
                final Collector collector;
                if (context.getProfilers() == null) {
                    collector = globalsCollector;
                } else {
                    InternalProfileCollector profileCollector = new InternalProfileCollector(
                            globalsCollector, CollectorResult.REASON_AGGREGATION_GLOBAL,
                            // TODO: report on sub collectors
                            Collections.emptyList());
                    collector = profileCollector;
                    // start a new profile with this collector
                    context.getProfilers().addQueryProfiler().setCollector(profileCollector);
                }
                globalsCollector.preCollection();
                context.searcher().search(query, collector);
            } catch (Exception e) {
                throw new QueryPhaseExecutionException(context.shardTarget(), "Failed to execute global aggregators", e);
            }
        }

        List<InternalAggregation> aggregations = new ArrayList<>(aggregators.length);
        if (context.aggregations().factories().context() != null) {
            // Rollup can end up here with a null context but not null factories.....
            context.aggregations().factories().context().multiBucketConsumer().reset();
        }
        for (Aggregator aggregator : context.aggregations().aggregators()) {
            try {
                aggregations.add(aggregator.buildTopLevel());
            } catch (IOException e) {
                throw new AggregationExecutionException("Failed to build aggregation [" + aggregator.name() + "]", e);
            }
        }
        context.queryResult().aggregations(InternalAggregations.from(aggregations));

        // disable aggregations so that they don't run on next pages in case of scrolling
        context.aggregations(null);
        context.queryCollectors().remove(AggregationPhase.class);
    }
}
