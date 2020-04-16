/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.spatial.index.mapper;

import org.apache.lucene.index.LeafReaderContext;
import org.elasticsearch.index.fielddata.DocValueBits;
import org.elasticsearch.index.fielddata.FieldData;
import org.elasticsearch.index.fielddata.SortedBinaryDocValues;
import org.elasticsearch.search.aggregations.support.ValuesSource;

import java.io.IOException;

public abstract class GeoShapeValuesSource extends ValuesSource {
    public static final GeoShapeValuesSource EMPTY = new GeoShapeValuesSource() {

        @Override
        public MultiGeoShapeValues geoShapeValues(LeafReaderContext context) {
            return MultiGeoShapeValues.EMPTY;
        }

        @Override
        public SortedBinaryDocValues bytesValues(LeafReaderContext context) throws IOException {
            return FieldData.emptySortedBinary();
        }

    };

    abstract MultiGeoShapeValues geoShapeValues(LeafReaderContext context);

    @Override
    public DocValueBits docsWithValue(LeafReaderContext context) throws IOException {
        MultiGeoShapeValues values = geoShapeValues(context);
        return new DocValueBits() {
            @Override
            public boolean advanceExact(int doc) throws IOException {
                return values.advanceExact(doc);
            }
        };
    }

    public static class Fielddata extends GeoShapeValuesSource {

        protected final IndexGeoShapeFieldData indexFieldData;

        public Fielddata(IndexGeoShapeFieldData indexFieldData) {
            this.indexFieldData = indexFieldData;
        }

        @Override
        public SortedBinaryDocValues bytesValues(LeafReaderContext context) {
            return indexFieldData.load(context).getBytesValues();
        }

        public MultiGeoShapeValues geoShapeValues(LeafReaderContext context) {
            return indexFieldData.load(context).getGeoShapeValues();
        }
    }
}
