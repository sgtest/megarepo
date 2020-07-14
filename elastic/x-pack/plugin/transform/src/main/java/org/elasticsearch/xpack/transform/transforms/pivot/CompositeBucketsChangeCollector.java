/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.transform.transforms.pivot;

import org.apache.lucene.search.BooleanQuery;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.common.Rounding;
import org.elasticsearch.common.geo.GeoPoint;
import org.elasticsearch.geometry.Rectangle;
import org.elasticsearch.index.query.BoolQueryBuilder;
import org.elasticsearch.index.query.GeoBoundingBoxQueryBuilder;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.index.query.RangeQueryBuilder;
import org.elasticsearch.index.query.TermsQueryBuilder;
import org.elasticsearch.search.aggregations.AggregationBuilder;
import org.elasticsearch.search.aggregations.Aggregations;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregation;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregation.Bucket;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregationBuilder;
import org.elasticsearch.search.aggregations.bucket.geogrid.GeoTileUtils;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.xpack.core.transform.transforms.pivot.DateHistogramGroupSource;
import org.elasticsearch.xpack.core.transform.transforms.pivot.SingleGroupSource;
import org.elasticsearch.xpack.transform.transforms.Function.ChangeCollector;

import java.util.Collection;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Map.Entry;
import java.util.Set;

/**
 * Utility class to collect bucket changes
 */
public class CompositeBucketsChangeCollector implements ChangeCollector {

    private final Map<String, FieldCollector> fieldCollectors;
    private final CompositeAggregationBuilder compositeAggregation;
    private Map<String, Object> afterKey = null;

    /**
     * Collector for collecting changes from 1 group_by field.
     *
     * Every field collector instance is stateful and implements the query logic and result collection,
     * but also stores the changes in their state.
     */
    interface FieldCollector {

        /**
         * Get the maximum page size supported by this field collector.
         *
         * Note: this page size is only about change collection, not the indexer page size.
         *
         * @return the maximum allowed page size, or Integer.MAX_VALUE for unlimited.
         */
        int getMaxPageSize();

        /**
         * Allows the field collector to add aggregations to the changes query.
         *
         * @return aggregations specific for this field collector or null.
         */
        AggregationBuilder aggregateChanges();

        /**
         * Collects the changes from the search response, e.g. stores the terms that have changed.
         *
         * @param buckets buckets from the search result.
         * @return true if changes have been found and got collected, false otherwise.
         */
        boolean collectChanges(Collection<? extends Bucket> buckets);

        /**
         * Apply the collected changes in the query that updates the transform destination.
         *
         * @param lastCheckpointTimestamp the last(complete) checkpoint timestamp
         * @param nextcheckpointTimestamp the next(currently running) checkpoint timestamp.
         * @return a querybuilder instance with added filters to narrow the search
         */
        QueryBuilder filterByChanges(long lastCheckpointTimestamp, long nextcheckpointTimestamp);

        /**
         * Clear the field collector, e.g. the changes to free up memory.
         */
        void clear();
    }

    static class TermsFieldCollector implements FieldCollector {

        private final String sourceFieldName;
        private final String targetFieldName;
        private final Set<String> changedTerms;

        TermsFieldCollector(final String sourceFieldName, final String targetFieldName) {
            this.sourceFieldName = sourceFieldName;
            this.targetFieldName = targetFieldName;
            this.changedTerms = new HashSet<>();
        }

        @Override
        public int getMaxPageSize() {
            // TODO: based on index.max_terms_count, however this is per index, which we don't have access to here,
            // because the page size is limit to 64k anyhow, return 64k
            return 65536;
        }

        @Override
        public boolean collectChanges(Collection<? extends Bucket> buckets) {
            changedTerms.clear();

            for (Bucket b : buckets) {
                Object term = b.getKey().get(targetFieldName);
                if (term != null) {
                    changedTerms.add(term.toString());
                }
            }

            return true;
        }

        @Override
        public QueryBuilder filterByChanges(long lastCheckpointTimestamp, long nextcheckpointTimestamp) {
            if (changedTerms.isEmpty() == false) {
                return new TermsQueryBuilder(sourceFieldName, changedTerms);
            }
            return null;
        }

        @Override
        public void clear() {
            changedTerms.clear();
        }

        @Override
        public AggregationBuilder aggregateChanges() {
            return null;
        }
    }

    static class DateHistogramFieldCollector implements FieldCollector {

        private final String sourceFieldName;
        private final String targetFieldName;
        private final boolean isSynchronizationField;
        private final Rounding.Prepared rounding;

        DateHistogramFieldCollector(
            final String sourceFieldName,
            final String targetFieldName,
            final Rounding.Prepared rounding,
            final boolean isSynchronizationField
        ) {
            this.sourceFieldName = sourceFieldName;
            this.targetFieldName = targetFieldName;
            this.rounding = rounding;
            this.isSynchronizationField = isSynchronizationField;
        }

        @Override
        public int getMaxPageSize() {
            return Integer.MAX_VALUE;
        }

        @Override
        public boolean collectChanges(Collection<? extends Bucket> buckets) {
            // todo: implementation for isSynchronizationField == false
            return false;
        }

        @Override
        public QueryBuilder filterByChanges(long lastCheckpointTimestamp, long nextcheckpointTimestamp) {
            if (isSynchronizationField && lastCheckpointTimestamp > 0) {
                return new RangeQueryBuilder(sourceFieldName).gte(rounding.round(lastCheckpointTimestamp)).format("epoch_millis");
            }

            // todo: implementation for isSynchronizationField == false

            return null;
        }

        @Override
        public void clear() {}

        @Override
        public AggregationBuilder aggregateChanges() {
            return null;
        }
    }

    static class HistogramFieldCollector implements FieldCollector {

        private final String sourceFieldName;
        private final String targetFieldName;

        HistogramFieldCollector(final String sourceFieldName, final String targetFieldName) {
            this.sourceFieldName = sourceFieldName;
            this.targetFieldName = targetFieldName;
        }

        @Override
        public int getMaxPageSize() {
            return Integer.MAX_VALUE;
        }

        @Override
        public boolean collectChanges(Collection<? extends Bucket> buckets) {
            return false;
        }

        @Override
        public QueryBuilder filterByChanges(long lastCheckpointTimestamp, long nextcheckpointTimestamp) {
            return null;
        }

        @Override
        public void clear() {}

        @Override
        public AggregationBuilder aggregateChanges() {
            return null;
        }
    }

    static class GeoTileFieldCollector implements FieldCollector {

        private final String sourceFieldName;
        private final String targetFieldName;
        private final Set<String> changedBuckets;

        GeoTileFieldCollector(final String sourceFieldName, final String targetFieldName) {
            this.sourceFieldName = sourceFieldName;
            this.targetFieldName = targetFieldName;
            this.changedBuckets = new HashSet<>();
        }

        @Override
        public int getMaxPageSize() {
            // this collector is limited by indices.query.bool.max_clause_count, default 1024
            return BooleanQuery.getMaxClauseCount();
        }

        @Override
        public boolean collectChanges(Collection<? extends Bucket> buckets) {
            changedBuckets.clear();

            for (Bucket b : buckets) {
                Object bucket = b.getKey().get(targetFieldName);
                if (bucket != null) {
                    changedBuckets.add(bucket.toString());
                }
            }

            return true;
        }

        @Override
        public QueryBuilder filterByChanges(long lastCheckpointTimestamp, long nextcheckpointTimestamp) {
            if (changedBuckets != null && changedBuckets.isEmpty() == false) {
                BoolQueryBuilder boolQueryBuilder = QueryBuilders.boolQuery();
                changedBuckets.stream().map(GeoTileUtils::toBoundingBox).map(this::toGeoQuery).forEach(boolQueryBuilder::should);
                return boolQueryBuilder;
            }
            return null;
        }

        @Override
        public void clear() {}

        @Override
        public AggregationBuilder aggregateChanges() {
            return null;
        }

        private GeoBoundingBoxQueryBuilder toGeoQuery(Rectangle rectangle) {
            return QueryBuilders.geoBoundingBoxQuery(sourceFieldName)
                .setCorners(
                    new GeoPoint(rectangle.getMaxLat(), rectangle.getMinLon()),
                    new GeoPoint(rectangle.getMinLat(), rectangle.getMaxLon())
                );
        }
    }

    public CompositeBucketsChangeCollector(CompositeAggregationBuilder compositeAggregation, Map<String, FieldCollector> fieldCollectors) {
        this.compositeAggregation = compositeAggregation;
        this.fieldCollectors = fieldCollectors;
    }

    @Override
    public SearchSourceBuilder buildChangesQuery(SearchSourceBuilder sourceBuilder, Map<String, Object> position, int pageSize) {

        sourceBuilder.size(0);
        for (FieldCollector fieldCollector : fieldCollectors.values()) {
            AggregationBuilder aggregationForField = fieldCollector.aggregateChanges();

            if (aggregationForField != null) {
                sourceBuilder.aggregation(aggregationForField);
            }
            pageSize = Math.min(pageSize, fieldCollector.getMaxPageSize());
        }

        CompositeAggregationBuilder changesAgg = this.compositeAggregation;
        changesAgg.size(pageSize).aggregateAfter(position);
        sourceBuilder.aggregation(changesAgg);

        return sourceBuilder;
    }

    @Override
    public QueryBuilder buildFilterQuery(long lastCheckpointTimestamp, long nextcheckpointTimestamp) {
        // shortcut for only 1 element
        if (fieldCollectors.size() == 1) {
            return fieldCollectors.values().iterator().next().filterByChanges(lastCheckpointTimestamp, nextcheckpointTimestamp);
        }

        BoolQueryBuilder filteredQuery = new BoolQueryBuilder();

        for (FieldCollector fieldCollector : fieldCollectors.values()) {
            QueryBuilder filter = fieldCollector.filterByChanges(lastCheckpointTimestamp, nextcheckpointTimestamp);
            if (filter != null) {
                filteredQuery.filter(filter);
            }
        }

        return filteredQuery;
    }

    @Override
    public boolean processSearchResponse(final SearchResponse searchResponse) {
        final Aggregations aggregations = searchResponse.getAggregations();
        if (aggregations == null) {
            return true;
        }

        final CompositeAggregation agg = aggregations.get(compositeAggregation.getName());

        Collection<? extends Bucket> buckets = agg.getBuckets();
        afterKey = agg.afterKey();

        if (buckets.isEmpty()) {
            return true;
        }

        for (FieldCollector fieldCollector : fieldCollectors.values()) {
            fieldCollector.collectChanges(buckets);
        }

        return false;
    }

    @Override
    public void clear() {
        fieldCollectors.forEach((k, c) -> c.clear());
    }

    @Override
    public Map<String, Object> getBucketPosition() {
        return afterKey;
    }

    public static ChangeCollector buildChangeCollector(
        CompositeAggregationBuilder compositeAggregationBuilder,
        Map<String, SingleGroupSource> groups,
        String synchronizationField
    ) {
        Map<String, FieldCollector> fieldCollectors = createFieldCollectors(groups, synchronizationField);
        return new CompositeBucketsChangeCollector(compositeAggregationBuilder, fieldCollectors);
    }

    static Map<String, FieldCollector> createFieldCollectors(Map<String, SingleGroupSource> groups, String synchronizationField) {
        Map<String, FieldCollector> fieldCollectors = new HashMap<>();

        for (Entry<String, SingleGroupSource> entry : groups.entrySet()) {
            switch (entry.getValue().getType()) {
                case TERMS:
                    fieldCollectors.put(
                        entry.getKey(),
                        new CompositeBucketsChangeCollector.TermsFieldCollector(entry.getValue().getField(), entry.getKey())
                    );
                    break;
                case HISTOGRAM:
                    fieldCollectors.put(
                        entry.getKey(),
                        new CompositeBucketsChangeCollector.HistogramFieldCollector(entry.getValue().getField(), entry.getKey())
                    );
                    break;
                case DATE_HISTOGRAM:
                    fieldCollectors.put(
                        entry.getKey(),
                        new CompositeBucketsChangeCollector.DateHistogramFieldCollector(
                            entry.getValue().getField(),
                            entry.getKey(),
                            ((DateHistogramGroupSource) entry.getValue()).getRounding(),
                            entry.getKey().equals(synchronizationField)
                        )
                    );
                    break;
                case GEOTILE_GRID:
                    fieldCollectors.put(
                        entry.getKey(),
                        new CompositeBucketsChangeCollector.GeoTileFieldCollector(entry.getValue().getField(), entry.getKey())
                    );
                    break;
                default:
                    throw new IllegalArgumentException("unknown type");
            }
        }
        return fieldCollectors;
    }

}
