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
package org.elasticsearch.common.geo;

import org.elasticsearch.common.geo.parsers.ShapeParser;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.hamcrest.ElasticsearchGeoAssertions;
import org.locationtech.jts.geom.Geometry;
import org.locationtech.jts.geom.GeometryFactory;
import org.locationtech.spatial4j.shape.Shape;
import org.locationtech.spatial4j.shape.ShapeCollection;
import org.locationtech.spatial4j.shape.jts.JtsGeometry;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

import static org.elasticsearch.common.geo.builders.ShapeBuilder.SPATIAL_CONTEXT;

/** Base class for all geo parsing tests */
abstract class BaseGeoParsingTestCase extends ESTestCase {
    protected static final GeometryFactory GEOMETRY_FACTORY = SPATIAL_CONTEXT.getGeometryFactory();

    public abstract void testParsePoint() throws IOException;
    public abstract void testParseMultiPoint() throws IOException;
    public abstract void testParseLineString() throws IOException;
    public abstract void testParseMultiLineString() throws IOException;
    public abstract void testParsePolygon() throws IOException;
    public abstract void testParseMultiPolygon() throws IOException;
    public abstract void testParseEnvelope() throws IOException;
    public abstract void testParseGeometryCollection() throws IOException;

    protected void assertValidException(XContentBuilder builder, Class<?> expectedException) throws IOException {
        try (XContentParser parser = createParser(builder)) {
            parser.nextToken();
            ElasticsearchGeoAssertions.assertValidException(parser, expectedException);
        }
    }

    protected void assertGeometryEquals(Object expected, XContentBuilder geoJson, boolean useJTS) throws IOException {
        try (XContentParser parser = createParser(geoJson)) {
            parser.nextToken();
            if (useJTS) {
                ElasticsearchGeoAssertions.assertEquals(expected, ShapeParser.parse(parser).buildS4J());
            } else {
                ElasticsearchGeoAssertions.assertEquals(expected, ShapeParser.parse(parser).buildGeometry());
            }
        }
    }

    protected ShapeCollection<Shape> shapeCollection(Shape... shapes) {
        return new ShapeCollection<>(Arrays.asList(shapes), SPATIAL_CONTEXT);
    }

    protected ShapeCollection<Shape> shapeCollection(Geometry... geoms) {
        List<Shape> shapes = new ArrayList<>(geoms.length);
        for (Geometry geom : geoms) {
            shapes.add(jtsGeom(geom));
        }
        return new ShapeCollection<>(shapes, SPATIAL_CONTEXT);
    }

    protected JtsGeometry jtsGeom(Geometry geom) {
        return new JtsGeometry(geom, SPATIAL_CONTEXT, false, false);
    }

}
