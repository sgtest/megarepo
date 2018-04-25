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
package org.elasticsearch.index.mapper;

import org.elasticsearch.common.geo.SpatialStrategy;
import org.elasticsearch.common.geo.builders.ShapeBuilder;
import org.elasticsearch.index.mapper.GeoShapeFieldMapper.GeoShapeFieldType;
import org.junit.Before;

import java.io.IOException;

public class GeoShapeFieldTypeTests extends FieldTypeTestCase {
    @Override
    protected MappedFieldType createDefaultFieldType() {
        return new GeoShapeFieldMapper.GeoShapeFieldType();
    }

    @Before
    public void setupProperties() {
        addModifier(new Modifier("tree", false) {
            @Override
            public void modify(MappedFieldType ft) {
                ((GeoShapeFieldMapper.GeoShapeFieldType)ft).setTree("quadtree");
            }
        });
        addModifier(new Modifier("strategy", false) {
            @Override
            public void modify(MappedFieldType ft) {
                ((GeoShapeFieldMapper.GeoShapeFieldType)ft).setStrategyName("term");
            }
        });
        addModifier(new Modifier("tree_levels", false) {
            @Override
            public void modify(MappedFieldType ft) {
                ((GeoShapeFieldMapper.GeoShapeFieldType)ft).setTreeLevels(10);
            }
        });
        addModifier(new Modifier("precision", false) {
            @Override
            public void modify(MappedFieldType ft) {
                ((GeoShapeFieldMapper.GeoShapeFieldType)ft).setPrecisionInMeters(20);
            }
        });
        addModifier(new Modifier("distance_error_pct", true) {
            @Override
            public void modify(MappedFieldType ft) {
                ((GeoShapeFieldMapper.GeoShapeFieldType)ft).setDefaultDistanceErrorPct(0.5);
            }
        });
        addModifier(new Modifier("orientation", true) {
            @Override
            public void modify(MappedFieldType ft) {
                ((GeoShapeFieldMapper.GeoShapeFieldType)ft).setOrientation(ShapeBuilder.Orientation.LEFT);
            }
        });
    }

    /**
     * Test for {@link GeoShapeFieldType#setStrategyName(String)} that checks that {@link GeoShapeFieldType#pointsOnly()}
     * gets set as a side effect when using SpatialStrategy.TERM
     */
    public void testSetStrategyName() throws IOException {
        GeoShapeFieldType fieldType = new GeoShapeFieldMapper.GeoShapeFieldType();
        assertFalse(fieldType.pointsOnly());
        fieldType.setStrategyName(SpatialStrategy.RECURSIVE.getStrategyName());
        assertFalse(fieldType.pointsOnly());
        fieldType.setStrategyName(SpatialStrategy.TERM.getStrategyName());
        assertTrue(fieldType.pointsOnly());
    }
}
