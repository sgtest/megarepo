/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.spatial.index.mapper;

import org.apache.lucene.index.DocValuesType;
import org.apache.lucene.index.FieldInfo;
import org.apache.lucene.index.LeafReader;
import org.apache.lucene.index.LeafReaderContext;
import org.apache.lucene.search.SortField;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.util.BigArrays;
import org.elasticsearch.index.Index;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.fielddata.IndexFieldData;
import org.elasticsearch.index.fielddata.IndexFieldDataCache;
import org.elasticsearch.index.fielddata.plain.DocValuesIndexFieldData;
import org.elasticsearch.index.mapper.MappedFieldType;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.indices.breaker.CircuitBreakerService;
import org.elasticsearch.search.DocValueFormat;
import org.elasticsearch.search.MultiValueMode;
import org.elasticsearch.search.sort.BucketedSort;
import org.elasticsearch.search.sort.SortOrder;

public abstract class AbstractLatLonShapeDVIndexFieldData extends DocValuesIndexFieldData implements IndexGeoShapeFieldData {
    AbstractLatLonShapeDVIndexFieldData(Index index, String fieldName) {
        super(index, fieldName);
    }

    @Override
    public SortField sortField(@Nullable Object missingValue, MultiValueMode sortMode, XFieldComparatorSource.Nested nested,
            boolean reverse) {
        throw new IllegalArgumentException("can't sort on geo_shape field without using specific sorting feature, like geo_distance");
    }

    public static class LatLonShapeDVIndexFieldData extends AbstractLatLonShapeDVIndexFieldData {
        public LatLonShapeDVIndexFieldData(Index index, String fieldName) {
            super(index, fieldName);
        }

        @Override
        public LeafGeoShapeFieldData load(LeafReaderContext context) {
            LeafReader reader = context.reader();
            FieldInfo info = reader.getFieldInfos().fieldInfo(fieldName);
            if (info != null) {
                checkCompatible(info);
            }
            return new LatLonShapeDVAtomicShapeFieldData(reader, fieldName);
        }

        @Override
        public LeafGeoShapeFieldData loadDirect(LeafReaderContext context) throws Exception {
            return load(context);
        }

        @Override
        public BucketedSort newBucketedSort(BigArrays bigArrays, Object missingValue, MultiValueMode sortMode,
                                            IndexFieldData.XFieldComparatorSource.Nested nested, SortOrder sortOrder, DocValueFormat format,
                                            int bucketSize, BucketedSort.ExtraData extra) {
            throw new IllegalArgumentException("can't sort on geo_shape field without using specific sorting feature, like geo_distance");
        }

        /** helper: checks a fieldinfo and throws exception if its definitely not a LatLonDocValuesField */
        static void checkCompatible(FieldInfo fieldInfo) {
            // dv properties could be "unset", if you e.g. used only StoredField with this same name in the segment.
            if (fieldInfo.getDocValuesType() != DocValuesType.NONE
                && fieldInfo.getDocValuesType() != DocValuesType.BINARY) {
                throw new IllegalArgumentException("field=\"" + fieldInfo.name + "\" was indexed with docValuesType="
                    + fieldInfo.getDocValuesType() + " but this type has docValuesType="
                    + DocValuesType.BINARY + ", is the field really a geo-shape field?");
            }
        }
    }

    public static class Builder implements IndexFieldData.Builder {
        @Override
        public IndexFieldData<?> build(IndexSettings indexSettings, MappedFieldType fieldType, IndexFieldDataCache cache,
                                       CircuitBreakerService breakerService, MapperService mapperService) {
            // ignore breaker
            return new LatLonShapeDVIndexFieldData(indexSettings.getIndex(), fieldType.name());
        }
    }
}
