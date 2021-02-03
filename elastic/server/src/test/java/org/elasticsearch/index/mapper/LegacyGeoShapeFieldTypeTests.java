/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.index.mapper;

import org.elasticsearch.Version;
import org.elasticsearch.common.geo.SpatialStrategy;
import org.elasticsearch.index.mapper.LegacyGeoShapeFieldMapper.GeoShapeFieldType;

import java.io.IOException;
import java.util.List;
import java.util.Map;

public class LegacyGeoShapeFieldTypeTests extends FieldTypeTestCase {

    /**
     * Test for {@link LegacyGeoShapeFieldMapper.GeoShapeFieldType#setStrategy(SpatialStrategy)} that checks
     * that {@link LegacyGeoShapeFieldMapper.GeoShapeFieldType#pointsOnly()} gets set as a side effect when using SpatialStrategy.TERM
     */
    public void testSetStrategyName() {
        GeoShapeFieldType fieldType = new GeoShapeFieldType("field");
        assertFalse(fieldType.pointsOnly());
        fieldType.setStrategy(SpatialStrategy.RECURSIVE);
        assertFalse(fieldType.pointsOnly());
        fieldType.setStrategy(SpatialStrategy.TERM);
        assertTrue(fieldType.pointsOnly());
    }

    public void testFetchSourceValue() throws IOException {

        MappedFieldType mapper = new LegacyGeoShapeFieldMapper.Builder("field", Version.CURRENT, false, true)
            .build(new ContentPath()).fieldType();

        Map<String, Object> jsonLineString = Map.of("type", "LineString", "coordinates",
            List.of(List.of(42.0, 27.1), List.of(30.0, 50.0)));
        Map<String, Object> jsonPoint = Map.of("type", "Point", "coordinates", List.of(14.0, 15.0));
        String wktLineString = "LINESTRING (42.0 27.1, 30.0 50.0)";
        String wktPoint = "POINT (14.0 15.0)";

        // Test a single shape in geojson format.
        Object sourceValue = jsonLineString;
        assertEquals(List.of(jsonLineString), fetchSourceValue(mapper, sourceValue, null));
        assertEquals(List.of(wktLineString), fetchSourceValue(mapper, sourceValue, "wkt"));

        // Test a list of shapes in geojson format.
        sourceValue = List.of(jsonLineString, jsonPoint);
        assertEquals(List.of(jsonLineString, jsonPoint), fetchSourceValue(mapper, sourceValue, null));
        assertEquals(List.of(wktLineString, wktPoint), fetchSourceValue(mapper, sourceValue, "wkt"));

        // Test a single shape in wkt format.
        sourceValue = wktLineString;
        assertEquals(List.of(jsonLineString), fetchSourceValue(mapper, sourceValue, null));
        assertEquals(List.of(wktLineString), fetchSourceValue(mapper, sourceValue, "wkt"));

        // Test a list of shapes in wkt format.
        sourceValue = List.of(wktLineString, wktPoint);
        assertEquals(List.of(jsonLineString, jsonPoint), fetchSourceValue(mapper, sourceValue, null));
        assertEquals(List.of(wktLineString, wktPoint), fetchSourceValue(mapper, sourceValue, "wkt"));
    }
}
