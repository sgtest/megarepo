/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.spatial.search.aggregations.bucket.geogrid;

import org.elasticsearch.xpack.spatial.index.fielddata.GeoShapeValues;

import java.io.IOException;
import java.util.function.LongConsumer;

/** Sorted numeric doc values for precision 0 */
class AllCellValues extends ByteTrackingSortingNumericDocValues {
    private GeoShapeValues geoValues;

    protected AllCellValues(GeoShapeValues geoValues, GeoGridTiler tiler, LongConsumer circuitBreakerConsumer) {
        super(circuitBreakerConsumer);
        this.geoValues = geoValues;
        resize(1);
        values[0] = tiler.encode(0, 0, 0);
    }

    @Override
    public boolean advanceExact(int docId) throws IOException {
        resize(1);
        return geoValues.advanceExact(docId);
    }
}
