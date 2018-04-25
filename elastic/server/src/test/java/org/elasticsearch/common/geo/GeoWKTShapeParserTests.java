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

import org.locationtech.jts.geom.Coordinate;
import org.locationtech.jts.geom.LineString;
import org.locationtech.jts.geom.LinearRing;
import org.locationtech.jts.geom.MultiLineString;
import org.locationtech.jts.geom.Point;
import org.locationtech.jts.geom.Polygon;
import org.apache.lucene.geo.GeoTestUtil;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.ElasticsearchParseException;
import org.elasticsearch.Version;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.geo.builders.CoordinatesBuilder;
import org.elasticsearch.common.geo.builders.EnvelopeBuilder;
import org.elasticsearch.common.geo.builders.GeometryCollectionBuilder;
import org.elasticsearch.common.geo.builders.LineStringBuilder;
import org.elasticsearch.common.geo.builders.MultiLineStringBuilder;
import org.elasticsearch.common.geo.builders.MultiPointBuilder;
import org.elasticsearch.common.geo.builders.MultiPolygonBuilder;
import org.elasticsearch.common.geo.builders.PointBuilder;
import org.elasticsearch.common.geo.builders.PolygonBuilder;
import org.elasticsearch.common.geo.builders.ShapeBuilder;
import org.elasticsearch.common.geo.parsers.GeoWKTParser;
import org.elasticsearch.common.geo.parsers.ShapeParser;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.index.mapper.ContentPath;
import org.elasticsearch.index.mapper.GeoShapeFieldMapper;
import org.elasticsearch.index.mapper.Mapper;
import org.elasticsearch.test.geo.RandomShapeGenerator;
import org.locationtech.spatial4j.exception.InvalidShapeException;
import org.locationtech.spatial4j.shape.Rectangle;
import org.locationtech.spatial4j.shape.Shape;
import org.locationtech.spatial4j.shape.ShapeCollection;
import org.locationtech.spatial4j.shape.jts.JtsPoint;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;

import static org.elasticsearch.common.geo.builders.ShapeBuilder.SPATIAL_CONTEXT;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.hasToString;

/**
 * Tests for {@code GeoWKTShapeParser}
 */
public class GeoWKTShapeParserTests extends BaseGeoParsingTestCase {

    private static XContentBuilder toWKTContent(ShapeBuilder builder, boolean generateMalformed)
            throws IOException {
        String wkt = builder.toWKT();
        if (generateMalformed) {
            // malformed - extra paren
            // TODO generate more malformed WKT
            wkt += GeoWKTParser.RPAREN;
        }
        if (randomBoolean()) {
            // test comments
            wkt = "# " + wkt + "\n" + wkt;
        }
        return XContentFactory.jsonBuilder().value(wkt);
    }

    private void assertExpected(Shape expected, ShapeBuilder builder) throws IOException {
        XContentBuilder xContentBuilder = toWKTContent(builder, false);
        assertGeometryEquals(expected, xContentBuilder);
    }

    private void assertMalformed(ShapeBuilder builder) throws IOException {
        XContentBuilder xContentBuilder = toWKTContent(builder, true);
        assertValidException(xContentBuilder, ElasticsearchParseException.class);
    }

    @Override
    public void testParsePoint() throws IOException {
        GeoPoint p = RandomShapeGenerator.randomPoint(random());
        Coordinate c = new Coordinate(p.lon(), p.lat());
        Point expected = GEOMETRY_FACTORY.createPoint(c);
        assertExpected(new JtsPoint(expected, SPATIAL_CONTEXT), new PointBuilder().coordinate(c));
        assertMalformed(new PointBuilder().coordinate(c));
    }

    @Override
    public void testParseMultiPoint() throws IOException {
        int numPoints = randomIntBetween(2, 100);
        List<Coordinate> coordinates = new ArrayList<>(numPoints);
        Shape[] shapes = new Shape[numPoints];
        GeoPoint p = new GeoPoint();
        for (int i = 0; i < numPoints; ++i) {
            p.reset(GeoTestUtil.nextLatitude(), GeoTestUtil.nextLongitude());
            coordinates.add(new Coordinate(p.lon(), p.lat()));
            shapes[i] = SPATIAL_CONTEXT.makePoint(p.lon(), p.lat());
        }
        ShapeCollection expected = shapeCollection(shapes);
        assertExpected(expected, new MultiPointBuilder(coordinates));
        assertMalformed(new MultiPointBuilder(coordinates));
    }

    private List<Coordinate> randomLineStringCoords() {
        int numPoints = randomIntBetween(2, 100);
        List<Coordinate> coordinates = new ArrayList<>(numPoints);
        GeoPoint p;
        for (int i = 0; i < numPoints; ++i) {
            p = RandomShapeGenerator.randomPointIn(random(), -90d, -90d, 90d, 90d);
            coordinates.add(new Coordinate(p.lon(), p.lat()));
        }
        return coordinates;
    }

    @Override
    public void testParseLineString() throws IOException {
        List<Coordinate> coordinates = randomLineStringCoords();
        LineString expected = GEOMETRY_FACTORY.createLineString(coordinates.toArray(new Coordinate[coordinates.size()]));
        assertExpected(jtsGeom(expected), new LineStringBuilder(coordinates));
    }

    @Override
    public void testParseMultiLineString() throws IOException {
        int numLineStrings = randomIntBetween(2, 8);
        List<LineString> lineStrings = new ArrayList<>(numLineStrings);
        MultiLineStringBuilder builder = new MultiLineStringBuilder();
        for (int j = 0; j < numLineStrings; ++j) {
            List<Coordinate> lsc = randomLineStringCoords();
            Coordinate [] coords = lsc.toArray(new Coordinate[lsc.size()]);
            lineStrings.add(GEOMETRY_FACTORY.createLineString(coords));
            builder.linestring(new LineStringBuilder(lsc));
        }
        MultiLineString expected = GEOMETRY_FACTORY.createMultiLineString(
            lineStrings.toArray(new LineString[lineStrings.size()]));
        assertExpected(jtsGeom(expected), builder);
        assertMalformed(builder);
    }

    @Override
    public void testParsePolygon() throws IOException {
        PolygonBuilder builder = PolygonBuilder.class.cast(
            RandomShapeGenerator.createShape(random(), RandomShapeGenerator.ShapeType.POLYGON));
        Coordinate[] coords = builder.coordinates()[0][0];
        LinearRing shell = GEOMETRY_FACTORY.createLinearRing(coords);
        Polygon expected = GEOMETRY_FACTORY.createPolygon(shell, null);
        assertExpected(jtsGeom(expected), builder);
        assertMalformed(builder);
    }

    @Override
    public void testParseMultiPolygon() throws IOException {
        int numPolys = randomIntBetween(2, 8);
        MultiPolygonBuilder builder = new MultiPolygonBuilder();
        PolygonBuilder pb;
        Coordinate[] coordinates;
        Polygon[] shapes = new Polygon[numPolys];
        LinearRing shell;
        for (int i = 0; i < numPolys; ++i) {
            pb = PolygonBuilder.class.cast(RandomShapeGenerator.createShape(random(), RandomShapeGenerator.ShapeType.POLYGON));
            builder.polygon(pb);
            coordinates = pb.coordinates()[0][0];
            shell = GEOMETRY_FACTORY.createLinearRing(coordinates);
            shapes[i] = GEOMETRY_FACTORY.createPolygon(shell, null);
        }
        Shape expected = shapeCollection(shapes);
        assertExpected(expected, builder);
        assertMalformed(builder);
    }

    public void testParsePolygonWithHole() throws IOException {
        // add 3d point to test ISSUE #10501
        List<Coordinate> shellCoordinates = new ArrayList<>();
        shellCoordinates.add(new Coordinate(100, 0));
        shellCoordinates.add(new Coordinate(101, 0));
        shellCoordinates.add(new Coordinate(101, 1));
        shellCoordinates.add(new Coordinate(100, 1));
        shellCoordinates.add(new Coordinate(100, 0));

        List<Coordinate> holeCoordinates = new ArrayList<>();
        holeCoordinates.add(new Coordinate(100.2, 0.2));
        holeCoordinates.add(new Coordinate(100.8, 0.2));
        holeCoordinates.add(new Coordinate(100.8, 0.8));
        holeCoordinates.add(new Coordinate(100.2, 0.8));
        holeCoordinates.add(new Coordinate(100.2, 0.2));

        PolygonBuilder polygonWithHole = new PolygonBuilder(new CoordinatesBuilder().coordinates(shellCoordinates));
        polygonWithHole.hole(new LineStringBuilder(holeCoordinates));

        LinearRing shell = GEOMETRY_FACTORY.createLinearRing(
            shellCoordinates.toArray(new Coordinate[shellCoordinates.size()]));
        LinearRing[] holes = new LinearRing[1];
        holes[0] = GEOMETRY_FACTORY.createLinearRing(
            holeCoordinates.toArray(new Coordinate[holeCoordinates.size()]));
        Polygon expected = GEOMETRY_FACTORY.createPolygon(shell, holes);

        assertExpected(jtsGeom(expected), polygonWithHole);
        assertMalformed(polygonWithHole);
    }

    public void testParseMixedDimensionPolyWithHole() throws IOException {
        List<Coordinate> shellCoordinates = new ArrayList<>();
        shellCoordinates.add(new Coordinate(100, 0));
        shellCoordinates.add(new Coordinate(101, 0));
        shellCoordinates.add(new Coordinate(101, 1));
        shellCoordinates.add(new Coordinate(100, 1));
        shellCoordinates.add(new Coordinate(100, 0));

        // add 3d point to test ISSUE #10501
        List<Coordinate> holeCoordinates = new ArrayList<>();
        holeCoordinates.add(new Coordinate(100.2, 0.2, 15.0));
        holeCoordinates.add(new Coordinate(100.8, 0.2));
        holeCoordinates.add(new Coordinate(100.8, 0.8));
        holeCoordinates.add(new Coordinate(100.2, 0.8, 10.0));
        holeCoordinates.add(new Coordinate(100.2, 0.2));

        PolygonBuilder builder = new PolygonBuilder(new CoordinatesBuilder().coordinates(shellCoordinates));
        builder.hole(new LineStringBuilder(holeCoordinates));

        XContentBuilder xContentBuilder = XContentFactory.jsonBuilder().value(builder.toWKT());
        XContentParser parser = createParser(xContentBuilder);
        parser.nextToken();

        Settings indexSettings = Settings.builder()
            .put(IndexMetaData.SETTING_VERSION_CREATED, Version.V_6_3_0)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .put(IndexMetaData.SETTING_INDEX_UUID, UUIDs.randomBase64UUID()).build();

        Mapper.BuilderContext mockBuilderContext = new Mapper.BuilderContext(indexSettings, new ContentPath());
        final GeoShapeFieldMapper mapperBuilder = new GeoShapeFieldMapper.Builder("test").ignoreZValue(false).build(mockBuilderContext);

        // test store z disabled
        ElasticsearchParseException e = expectThrows(ElasticsearchParseException.class,
            () -> ShapeParser.parse(parser, mapperBuilder));
        assertThat(e, hasToString(containsString("but [ignore_z_value] parameter is [false]")));
    }

    public void testParseMixedDimensionPolyWithHoleStoredZ() throws IOException {
        List<Coordinate> shellCoordinates = new ArrayList<>();
        shellCoordinates.add(new Coordinate(100, 0));
        shellCoordinates.add(new Coordinate(101, 0));
        shellCoordinates.add(new Coordinate(101, 1));
        shellCoordinates.add(new Coordinate(100, 1));
        shellCoordinates.add(new Coordinate(100, 0));

        // add 3d point to test ISSUE #10501
        List<Coordinate> holeCoordinates = new ArrayList<>();
        holeCoordinates.add(new Coordinate(100.2, 0.2, 15.0));
        holeCoordinates.add(new Coordinate(100.8, 0.2));
        holeCoordinates.add(new Coordinate(100.8, 0.8));
        holeCoordinates.add(new Coordinate(100.2, 0.8, 10.0));
        holeCoordinates.add(new Coordinate(100.2, 0.2));

        PolygonBuilder builder = new PolygonBuilder(new CoordinatesBuilder().coordinates(shellCoordinates));
        builder.hole(new LineStringBuilder(holeCoordinates));

        XContentBuilder xContentBuilder = XContentFactory.jsonBuilder().value(builder.toWKT());
        XContentParser parser = createParser(xContentBuilder);
        parser.nextToken();

        Settings indexSettings = Settings.builder()
            .put(IndexMetaData.SETTING_VERSION_CREATED, Version.V_6_3_0)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .put(IndexMetaData.SETTING_INDEX_UUID, UUIDs.randomBase64UUID()).build();

        Mapper.BuilderContext mockBuilderContext = new Mapper.BuilderContext(indexSettings, new ContentPath());
        final GeoShapeFieldMapper mapperBuilder = new GeoShapeFieldMapper.Builder("test").ignoreZValue(true).build(mockBuilderContext);

        // test store z disabled
        ElasticsearchException e = expectThrows(ElasticsearchException.class,
            () -> ShapeParser.parse(parser, mapperBuilder));
        assertThat(e, hasToString(containsString("unable to add coordinate to CoordinateBuilder: coordinate dimensions do not match")));
    }

    public void testParsePolyWithStoredZ() throws IOException {
        List<Coordinate> shellCoordinates = new ArrayList<>();
        shellCoordinates.add(new Coordinate(100, 0, 0));
        shellCoordinates.add(new Coordinate(101, 0, 0));
        shellCoordinates.add(new Coordinate(101, 1, 0));
        shellCoordinates.add(new Coordinate(100, 1, 5));
        shellCoordinates.add(new Coordinate(100, 0, 5));

        PolygonBuilder builder = new PolygonBuilder(new CoordinatesBuilder().coordinates(shellCoordinates));

        XContentBuilder xContentBuilder = XContentFactory.jsonBuilder().value(builder.toWKT());
        XContentParser parser = createParser(xContentBuilder);
        parser.nextToken();

        Settings indexSettings = Settings.builder()
            .put(IndexMetaData.SETTING_VERSION_CREATED, Version.V_6_3_0)
            .put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0)
            .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1)
            .put(IndexMetaData.SETTING_INDEX_UUID, UUIDs.randomBase64UUID()).build();

        Mapper.BuilderContext mockBuilderContext = new Mapper.BuilderContext(indexSettings, new ContentPath());
        final GeoShapeFieldMapper mapperBuilder = new GeoShapeFieldMapper.Builder("test").ignoreZValue(true).build(mockBuilderContext);

        ShapeBuilder shapeBuilder = ShapeParser.parse(parser, mapperBuilder);
        assertEquals(shapeBuilder.numDimensions(), 3);
    }

    public void testParseSelfCrossingPolygon() throws IOException {
        // test self crossing ccw poly not crossing dateline
        List<Coordinate> shellCoordinates = new ArrayList<>();
        shellCoordinates.add(new Coordinate(176, 15));
        shellCoordinates.add(new Coordinate(-177, 10));
        shellCoordinates.add(new Coordinate(-177, -10));
        shellCoordinates.add(new Coordinate(176, -15));
        shellCoordinates.add(new Coordinate(-177, 15));
        shellCoordinates.add(new Coordinate(172, 0));
        shellCoordinates.add(new Coordinate(176, 15));

        PolygonBuilder poly = new PolygonBuilder(new CoordinatesBuilder().coordinates(shellCoordinates));
        XContentBuilder builder = XContentFactory.jsonBuilder().value(poly.toWKT());
        assertValidException(builder, InvalidShapeException.class);
    }

    public void testMalformedWKT() throws IOException {
        // malformed points in a polygon is a common typo
        String malformedWKT = "POLYGON ((100, 5) (100, 10) (90, 10), (90, 5), (100, 5)";
        XContentBuilder builder = XContentFactory.jsonBuilder().value(malformedWKT);
        assertValidException(builder, ElasticsearchParseException.class);
    }

    @Override
    public void testParseEnvelope() throws IOException {
        org.apache.lucene.geo.Rectangle r = GeoTestUtil.nextBox();
        EnvelopeBuilder builder = new EnvelopeBuilder(new Coordinate(r.minLon, r.maxLat), new Coordinate(r.maxLon, r.minLat));
        Rectangle expected = SPATIAL_CONTEXT.makeRectangle(r.minLon, r.maxLon, r.minLat, r.maxLat);
        assertExpected(expected, builder);
        assertMalformed(builder);
    }

    public void testInvalidGeometryType() throws IOException {
        XContentBuilder builder = XContentFactory.jsonBuilder().value("UnknownType (-1 -2)");
        assertValidException(builder, IllegalArgumentException.class);
    }

    @Override
    public void testParseGeometryCollection() throws IOException {
        if (rarely()) {
            // assert empty shape collection
            GeometryCollectionBuilder builder = new GeometryCollectionBuilder();
            Shape[] expected = new Shape[0];
            assertEquals(shapeCollection(expected).isEmpty(), builder.build().isEmpty());
        } else {
            GeometryCollectionBuilder gcb = RandomShapeGenerator.createGeometryCollection(random());
            assertExpected(gcb.build(), gcb);
        }
    }

    public void testUnexpectedShapeException() throws IOException {
        XContentBuilder builder = toWKTContent(new PointBuilder(-1, 2), false);
        XContentParser parser = createParser(builder);
        parser.nextToken();
        ElasticsearchParseException e = expectThrows(ElasticsearchParseException.class,
            () -> GeoWKTParser.parseExpectedType(parser, GeoShapeType.POLYGON));
        assertThat(e, hasToString(containsString("Expected geometry type [polygon] but found [point]")));
    }
}
