/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.runtimefields.fielddata;

import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.search.SortField;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.index.fielddata.IndexFieldData;
import org.elasticsearch.index.fielddata.IndexFieldDataCache;
import org.elasticsearch.index.fielddata.IndexGeoPointFieldData;
import org.elasticsearch.index.fielddata.LeafGeoPointFieldData;
import org.elasticsearch.index.fielddata.MultiGeoPointValues;
import org.elasticsearch.index.fielddata.plain.AbstractLeafGeoPointFieldData;
import org.elasticsearch.indices.breaker.CircuitBreakerService;
import org.elasticsearch.search.DocValueFormat;
import org.elasticsearch.search.MultiValueMode;
import org.elasticsearch.search.aggregations.support.CoreValuesSourceType;
import org.elasticsearch.search.aggregations.support.ValuesSourceType;
import org.elasticsearch.search.sort.BucketedSort;
import org.elasticsearch.search.sort.SortOrder;
import org.elasticsearch.xpack.runtimefields.mapper.GeoPointFieldScript;

public class GeoPointScriptFieldData implements IndexGeoPointFieldData {
    public static class Builder implements IndexFieldData.Builder {
        private final String name;
        private final GeoPointFieldScript.LeafFactory leafFactory;

        public Builder(String name, GeoPointFieldScript.LeafFactory leafFactory) {
            this.name = name;
            this.leafFactory = leafFactory;
        }

        @Override
        public GeoPointScriptFieldData build(IndexFieldDataCache cache, CircuitBreakerService breakerService) {
            return new GeoPointScriptFieldData(name, leafFactory);
        }
    }

    private final GeoPointFieldScript.LeafFactory leafFactory;
    private final String name;

    private GeoPointScriptFieldData(String fieldName, GeoPointFieldScript.LeafFactory leafFactory) {
        this.name = fieldName;
        this.leafFactory = leafFactory;
    }

    @Override
    public SortField sortField(Object missingValue, MultiValueMode sortMode, XFieldComparatorSource.Nested nested, boolean reverse) {
        throw new IllegalArgumentException("can't sort on geo_point field without using specific sorting feature, like geo_distance");
    }

    @Override
    public BucketedSort newBucketedSort(
        BigArrays bigArrays,
        Object missingValue,
        MultiValueMode sortMode,
        XFieldComparatorSource.Nested nested,
        SortOrder sortOrder,
        DocValueFormat format,
        int bucketSize,
        BucketedSort.ExtraData extra
    ) {
        throw new IllegalArgumentException("can't sort on geo_point field without using specific sorting feature, like geo_distance");
    }

    @Override
    public String getFieldName() {
        return name;
    }

    @Override
    public ValuesSourceType getValuesSourceType() {
        return CoreValuesSourceType.GEOPOINT;
    }

    @Override
    public LeafGeoPointFieldData load(LeafReaderContext context) {
        GeoPointFieldScript script = leafFactory.newInstance(context);
        return new AbstractLeafGeoPointFieldData() {
            @Override
            public MultiGeoPointValues getGeoPointValues() {
                return new GeoPointScriptDocValues(script);
            }

            @Override
            public long ramBytesUsed() {
                return 0;
            }

            @Override
            public void close() {

            }
        };
    }

    @Override
    public LeafGeoPointFieldData loadDirect(LeafReaderContext context) {
        return load(context);
    }
}
