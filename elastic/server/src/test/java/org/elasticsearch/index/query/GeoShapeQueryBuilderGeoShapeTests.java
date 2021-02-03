/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.index.query;

import org.elasticsearch.Version;
import org.elasticsearch.common.geo.ShapeRelation;
import org.elasticsearch.common.geo.builders.ShapeBuilder;
import org.elasticsearch.test.geo.RandomShapeGenerator;

public class GeoShapeQueryBuilderGeoShapeTests extends GeoShapeQueryBuilderTests {

    protected String fieldName() {
        return GEO_SHAPE_FIELD_NAME;
    }

    protected GeoShapeQueryBuilder doCreateTestQueryBuilder(boolean indexedShape) {
        RandomShapeGenerator.ShapeType shapeType = randomFrom(
            RandomShapeGenerator.ShapeType.POINT,
            RandomShapeGenerator.ShapeType.MULTIPOINT,
            RandomShapeGenerator.ShapeType.LINESTRING,
            RandomShapeGenerator.ShapeType.MULTILINESTRING,
            RandomShapeGenerator.ShapeType.POLYGON);
        ShapeBuilder<?, ?, ?> shape = RandomShapeGenerator.createShapeWithin(random(), null, shapeType);
        GeoShapeQueryBuilder builder;
        clearShapeFields();
        if (indexedShape == false) {
            builder = new GeoShapeQueryBuilder(fieldName(), shape);
        } else {
            indexedShapeToReturn = shape;
            indexedShapeId = randomAlphaOfLengthBetween(3, 20);
            builder = new GeoShapeQueryBuilder(fieldName(), indexedShapeId);
            if (randomBoolean()) {
                indexedShapeIndex = randomAlphaOfLengthBetween(3, 20);
                builder.indexedShapeIndex(indexedShapeIndex);
            }
            if (randomBoolean()) {
                indexedShapePath = randomAlphaOfLengthBetween(3, 20);
                builder.indexedShapePath(indexedShapePath);
            }
            if (randomBoolean()) {
                indexedShapeRouting = randomAlphaOfLengthBetween(3, 20);
                builder.indexedShapeRouting(indexedShapeRouting);
            }
        }
        if (randomBoolean()) {
            SearchExecutionContext context = createSearchExecutionContext();
            if (context.indexVersionCreated().onOrAfter(Version.V_7_5_0)) { // CONTAINS is only supported from version 7.5
                if (shapeType == RandomShapeGenerator.ShapeType.LINESTRING || shapeType == RandomShapeGenerator.ShapeType.MULTILINESTRING) {
                    builder.relation(randomFrom(ShapeRelation.DISJOINT, ShapeRelation.INTERSECTS, ShapeRelation.CONTAINS));
                } else {
                    builder.relation(randomFrom(ShapeRelation.DISJOINT, ShapeRelation.INTERSECTS,
                        ShapeRelation.WITHIN, ShapeRelation.CONTAINS));
                }
            } else {
                if (shapeType == RandomShapeGenerator.ShapeType.LINESTRING || shapeType == RandomShapeGenerator.ShapeType.MULTILINESTRING) {
                    builder.relation(randomFrom(ShapeRelation.DISJOINT, ShapeRelation.INTERSECTS));
                } else {
                    builder.relation(randomFrom(ShapeRelation.DISJOINT, ShapeRelation.INTERSECTS, ShapeRelation.WITHIN));
                }
            }
        }

        if (randomBoolean()) {
            builder.ignoreUnmapped(randomBoolean());
        }
        return builder;
    }
}
