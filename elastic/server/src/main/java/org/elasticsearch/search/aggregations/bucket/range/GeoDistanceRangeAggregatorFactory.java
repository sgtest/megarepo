/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.search.aggregations.bucket.range;

import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.index.SortedNumericDocValues;
import org.elasticsearch.common.geo.GeoDistance;
import org.elasticsearch.common.geo.GeoPoint;
import org.elasticsearch.common.geo.GeoUtils;
import org.elasticsearch.common.unit.DistanceUnit;
import org.elasticsearch.index.fielddata.MultiGeoPointValues;
import org.elasticsearch.index.fielddata.SortedBinaryDocValues;
import org.elasticsearch.index.fielddata.SortedNumericDoubleValues;
import org.elasticsearch.search.aggregations.Aggregator;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.elasticsearch.search.aggregations.AggregatorFactory;
import org.elasticsearch.search.aggregations.CardinalityUpperBound;
import org.elasticsearch.search.aggregations.bucket.range.GeoDistanceAggregationBuilder.Range;
import org.elasticsearch.search.aggregations.support.AggregationContext;
import org.elasticsearch.search.aggregations.support.CoreValuesSourceType;
import org.elasticsearch.search.aggregations.support.ValuesSource;
import org.elasticsearch.search.aggregations.support.ValuesSourceAggregatorFactory;
import org.elasticsearch.search.aggregations.support.ValuesSourceConfig;
import org.elasticsearch.search.aggregations.support.ValuesSourceRegistry;
import org.elasticsearch.search.internal.SearchContext;

import java.io.IOException;
import java.util.Map;

public class GeoDistanceRangeAggregatorFactory extends ValuesSourceAggregatorFactory {

    public static void registerAggregators(ValuesSourceRegistry.Builder builder) {
        builder.register(
            GeoDistanceAggregationBuilder.REGISTRY_KEY,
            CoreValuesSourceType.GEOPOINT,
            (
                name,
                factories,
                distanceType,
                origin,
                units,
                valuesSource,
                format,
                rangeFactory,
                ranges,
                keyed,
                context,
                parent,
                cardinality,
                metadata) -> {
                DistanceSource distanceSource = new DistanceSource((ValuesSource.GeoPoint) valuesSource, distanceType, origin, units);
                double averageDocsPerRange = ((double) context.searcher().getIndexReader().maxDoc()) / ranges.length;
                return RangeAggregator.buildWithoutAttemptedToAdaptToFilters(
                    name,
                    factories,
                    distanceSource,
                    format,
                    rangeFactory,
                    ranges,
                    averageDocsPerRange,
                    null, // null here because we didn't try filters at all
                    keyed,
                    context,
                    parent,
                    cardinality,
                    metadata
                );
            },
                true);
    }

    private final InternalRange.Factory<InternalGeoDistance.Bucket, InternalGeoDistance> rangeFactory = InternalGeoDistance.FACTORY;
    private final GeoPoint origin;
    private final Range[] ranges;
    private final DistanceUnit unit;
    private final GeoDistance distanceType;
    private final boolean keyed;

    public GeoDistanceRangeAggregatorFactory(String name, ValuesSourceConfig config, GeoPoint origin,
                                             Range[] ranges, DistanceUnit unit, GeoDistance distanceType, boolean keyed,
                                             AggregationContext context, AggregatorFactory parent,
                                             AggregatorFactories.Builder subFactoriesBuilder,
                                             Map<String, Object> metadata) throws IOException {
        super(name, config, context, parent, subFactoriesBuilder, metadata);
        this.origin = origin;
        this.ranges = ranges;
        this.unit = unit;
        this.distanceType = distanceType;
        this.keyed = keyed;
    }

    @Override
    protected Aggregator createUnmapped(SearchContext searchContext,
                                            Aggregator parent,
                                            Map<String, Object> metadata) throws IOException {
        return new RangeAggregator.Unmapped<>(name, factories, ranges, keyed, config.format(), searchContext, parent,
            rangeFactory, metadata);
    }

    @Override
    protected Aggregator doCreateInternal(
        SearchContext searchContext,
        Aggregator parent,
        CardinalityUpperBound cardinality,
        Map<String, Object> metadata
    ) throws IOException {
        return context.getValuesSourceRegistry()
            .getAggregator(GeoDistanceAggregationBuilder.REGISTRY_KEY, config)
            .build(
                name,
                factories,
                distanceType,
                origin,
                unit,
                config.getValuesSource(),
                config.format(),
                rangeFactory,
                ranges,
                keyed,
                searchContext,
                parent,
                cardinality,
                metadata
            );
    }

    private static class DistanceSource extends ValuesSource.Numeric {

        private final ValuesSource.GeoPoint source;
        private final GeoDistance distanceType;
        private final DistanceUnit units;
        private final org.elasticsearch.common.geo.GeoPoint origin;

        DistanceSource(ValuesSource.GeoPoint source, GeoDistance distanceType,
                org.elasticsearch.common.geo.GeoPoint origin, DistanceUnit units) {
            this.source = source;
            // even if the geo points are unique, there's no guarantee the
            // distances are
            this.distanceType = distanceType;
            this.units = units;
            this.origin = origin;
        }

        @Override
        public boolean isFloatingPoint() {
            return true;
        }

        @Override
        public SortedNumericDocValues longValues(LeafReaderContext ctx) {
            throw new UnsupportedOperationException();
        }

        @Override
        public SortedNumericDoubleValues doubleValues(LeafReaderContext ctx) {
            final MultiGeoPointValues geoValues = source.geoPointValues(ctx);
            return GeoUtils.distanceValues(distanceType, units, geoValues, origin);
        }

        @Override
        public SortedBinaryDocValues bytesValues(LeafReaderContext ctx) {
            throw new UnsupportedOperationException();
        }

    }
}
