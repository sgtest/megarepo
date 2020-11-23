/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.spatial.search.aggregations;

import org.apache.lucene.geo.GeoEncodingUtils;
import org.apache.lucene.index.LeafReaderContext;
import org.elasticsearch.common.geo.GeoPoint;
import org.elasticsearch.common.lease.Releasable;
import org.elasticsearch.common.lease.Releasables;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.common.util.DoubleArray;
import org.elasticsearch.common.util.LongArray;
import org.elasticsearch.index.fielddata.MultiGeoPointValues;
import org.elasticsearch.index.fielddata.SortedNumericDoubleValues;
import org.elasticsearch.search.DocValueFormat;
import org.elasticsearch.search.aggregations.AggregationExecutionException;
import org.elasticsearch.xpack.spatial.search.aggregations.support.GeoLineMultiValuesSource;
import org.elasticsearch.search.sort.BucketedSort;
import org.elasticsearch.search.sort.SortOrder;
import org.elasticsearch.xpack.core.common.search.aggregations.MissingHelper;

import java.io.IOException;

import static org.elasticsearch.xpack.spatial.search.aggregations.GeoLineAggregationBuilder.SORT_FIELD;

/**
 * A bigArrays sorter of both a geo_line's sort-values and points.
 *
 * This class accumulates geo_points within buckets and heapifies the
 * bucket based on whether there are too many items in the bucket that
 * need to be dropped based on their sort value.
 */
public class GeoLineBucketedSort extends BucketedSort.ForDoubles {
    private final GeoLineMultiValuesSource valuesSources;

    public GeoLineBucketedSort(BigArrays bigArrays, SortOrder sortOrder, DocValueFormat format, int bucketSize,
                               GeoLineMultiValuesSource valuesSources, GeoLineBucketedSort.Extra extra) {
        super(bigArrays, sortOrder, format, bucketSize, extra);
        this.valuesSources = valuesSources;
    }

    public long sizeOf(long bucket) {
        int bucketSize = getBucketSize();
        long rootIndex = bucket * bucketSize;
        if (rootIndex >= values().size()) {
            // We've never seen this bucket.
            return 0;
        }
        long start = inHeapMode(bucket) ? rootIndex : (rootIndex + getNextGatherOffset(rootIndex) + 1);
        long end = rootIndex + bucketSize;
        return end - start;
    }

    /**
     * @param bucket the bucket ordinal
     * @return the array of sort-values for the specific bucket. This array may not necessarily be heapified already, so no ordering is
     *         guaranteed.
     */
    public double[] getSortValues(long bucket) {
        int bucketSize = getBucketSize();
        long rootIndex = bucket * bucketSize;
        if (rootIndex >= values().size()) {
            // We've never seen this bucket.
            return new double[]{};
        }
        long start = inHeapMode(bucket) ? rootIndex : (rootIndex + getNextGatherOffset(rootIndex) + 1);
        long end = rootIndex + bucketSize;
        double[] result = new double[(int)(end - start)];
        int i = 0;
        for (long index = start; index < end; index++) {
            double timestampValue = ((DoubleArray)values()).get(index);
            result[i++] = timestampValue;
        }
        return result;
    }

    /**
     * @param bucket the bucket ordinal
     * @return the array of points, ordered by the their respective sort-value for the specific bucket.
     */
    public long[] getPoints(long bucket) {
        int bucketSize = getBucketSize();
        long rootIndex = bucket * bucketSize;
        if (rootIndex >= values().size()) {
            // We've never seen this bucket.
            return new long[]{};
        }
        long start = inHeapMode(bucket) ? rootIndex : (rootIndex + getNextGatherOffset(rootIndex) + 1);
        long end = rootIndex + bucketSize;
        long[] result = new long[(int)(end - start)];
        int i = 0;
        for (long index = start; index < end; index++) {
            long geoPointValue = ((Extra) extra).values.get(index);
            result[i++] = geoPointValue;
        }
        return result;
    }

    @Override
    public BucketedSort.Leaf forLeaf(LeafReaderContext ctx) throws IOException {
        return new BucketedSort.ForDoubles.Leaf(ctx) {
            private final SortedNumericDoubleValues docSortValues = valuesSources.getNumericField(SORT_FIELD.getPreferredName(), ctx);
            private double docValue;

            @Override
            protected boolean advanceExact(int doc) throws IOException {
                if (docSortValues.advanceExact(doc)) {
                    if (docSortValues.docValueCount() > 1) {
                        throw new AggregationExecutionException("Encountered more than one sort value for a " +
                            "single document. Use a script to combine multiple sort-values-per-doc into a single value.");
                    }

                    // There should always be one weight if advanceExact lands us here, either
                    // a real weight or a `missing` weight
                    assert docSortValues.docValueCount() == 1;
                    docValue = docSortValues.nextValue();
                    return true;
                } else {
                    docValue = Long.MIN_VALUE;
                }
                return false;
            }

            @Override
            protected double docValue() {
                return docValue;
            }
        };
    }

    /**
     * An {@link BucketedSort.ExtraData} representing the geo-point for a document
     * within a bucket.
     */
    static class Extra implements BucketedSort.ExtraData, Releasable {

        private final BigArrays bigArrays;
        private final GeoLineMultiValuesSource valuesSources;
        private LongArray values;
        private final MissingHelper empty;

        Extra(BigArrays bigArrays, GeoLineMultiValuesSource valuesSources) {
            this.bigArrays = bigArrays;
            this.valuesSources = valuesSources;
            this.values = bigArrays.newLongArray(1, false);
            this.empty = new MissingHelper(bigArrays);
        }

        @Override
        public void swap(long lhs, long rhs) {
            long tmp = values.get(lhs);
            values.set(lhs, values.get(rhs));
            values.set(rhs, tmp);
            empty.swap(lhs, rhs);
        }

        @Override
        public Loader loader(LeafReaderContext ctx) throws IOException {
            final MultiGeoPointValues docGeoPointValues = valuesSources
                .getGeoPointField(GeoLineAggregationBuilder.POINT_FIELD.getPreferredName(), ctx);
            return (index, doc) -> {
                if (false == docGeoPointValues.advanceExact(doc)) {
                    empty.markMissing(index);
                    return;
                }

                if (docGeoPointValues.docValueCount() > 1) {
                    throw new AggregationExecutionException("Encountered more than one geo_point value for a " +
                        "single document. Use a script to combine multiple geo_point-values-per-doc into a single value.");
                }

                if (index > values.size()) {
                    values = bigArrays.grow(values, index + 1);
                }

                final GeoPoint point = docGeoPointValues.nextValue();
                int encodedLat = GeoEncodingUtils.encodeLatitude(point.lat());
                int encodedLon = GeoEncodingUtils.encodeLongitude(point.lon());
                long lonLat = (((long) encodedLon) << 32) | encodedLat & 0xffffffffL;

                values.set(index, lonLat);
                empty.markNotMissing(index);
            };
        }

        @Override
        public void close() {
            Releasables.close(values, empty);
        }
    }
}
