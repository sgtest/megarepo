/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.search.geo;

import org.elasticsearch.action.get.GetResponse;
import org.elasticsearch.action.search.SearchAction;
import org.elasticsearch.action.search.SearchPhaseExecutionException;
import org.elasticsearch.action.search.SearchRequestBuilder;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.geo.GeoShapeType;
import org.elasticsearch.common.geo.ShapeRelation;
import org.elasticsearch.common.geo.builders.CircleBuilder;
import org.elasticsearch.common.geo.builders.CoordinatesBuilder;
import org.elasticsearch.common.geo.builders.EnvelopeBuilder;
import org.elasticsearch.common.geo.builders.GeometryCollectionBuilder;
import org.elasticsearch.common.geo.builders.LineStringBuilder;
import org.elasticsearch.common.geo.builders.MultiLineStringBuilder;
import org.elasticsearch.common.geo.builders.MultiPointBuilder;
import org.elasticsearch.common.geo.builders.MultiPolygonBuilder;
import org.elasticsearch.common.geo.builders.PointBuilder;
import org.elasticsearch.common.geo.builders.PolygonBuilder;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.geometry.Geometry;
import org.elasticsearch.geometry.Line;
import org.elasticsearch.geometry.LinearRing;
import org.elasticsearch.geometry.MultiLine;
import org.elasticsearch.geometry.MultiPoint;
import org.elasticsearch.geometry.Point;
import org.elasticsearch.geometry.Rectangle;
import org.elasticsearch.index.query.GeoShapeQueryBuilder;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.search.SearchHits;
import org.elasticsearch.test.ESSingleNodeTestCase;
import org.elasticsearch.test.TestGeoShapeFieldMapperPlugin;
import org.locationtech.jts.geom.Coordinate;

import java.util.Collection;
import java.util.Collections;

import static org.elasticsearch.action.support.WriteRequest.RefreshPolicy.IMMEDIATE;
import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertSearchResponse;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.not;
import static org.hamcrest.Matchers.nullValue;

public abstract class GeoQueryTests extends ESSingleNodeTestCase {
    @Override
    protected Collection<Class<? extends Plugin>> getPlugins() {
        return Collections.singleton(TestGeoShapeFieldMapperPlugin.class);
    }

    protected abstract XContentBuilder createDefaultMapping() throws Exception;

    static String defaultGeoFieldName = "geo";
    static String defaultIndexName = "test";

    public void testNullShape() throws Exception {
        String mapping = Strings.toString(createDefaultMapping());
        client().admin().indices().prepareCreate(defaultIndexName).setMapping(mapping).get();
        ensureGreen();

        client().prepareIndex(defaultIndexName)
            .setId("aNullshape")
            .setSource("{\"geo\": null}", XContentType.JSON)
            .setRefreshPolicy(IMMEDIATE).get();
        GetResponse result = client().prepareGet(defaultIndexName, "aNullshape").get();
        assertThat(result.getField("location"), nullValue());
    };

    public void testIndexPointsFilterRectangle() throws Exception {
        String mapping = Strings.toString(createDefaultMapping());
        client().admin().indices().prepareCreate(defaultIndexName).setMapping(mapping).get();
        ensureGreen();

        client().prepareIndex(defaultIndexName).setId("1").setSource(jsonBuilder()
            .startObject()
              .field("name", "Document 1")
              .field(defaultGeoFieldName, "POINT(-30 -30)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        client().prepareIndex(defaultIndexName).setId("2").setSource(jsonBuilder()
            .startObject()
              .field("name", "Document 2")
              .field(defaultGeoFieldName, "POINT(-45 -50)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        EnvelopeBuilder shape = new EnvelopeBuilder(new Coordinate(-45, 45), new Coordinate(45, -45));
        GeometryCollectionBuilder builder = new GeometryCollectionBuilder().shape(shape);
        Geometry geometry = builder.buildGeometry().get(0);
        SearchResponse searchResponse = client().prepareSearch(defaultIndexName)
            .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, geometry)
                .relation(ShapeRelation.INTERSECTS))
            .get();

        assertSearchResponse(searchResponse);
        assertThat(searchResponse.getHits().getTotalHits().value, equalTo(1L));
        assertThat(searchResponse.getHits().getHits().length, equalTo(1));
        assertThat(searchResponse.getHits().getAt(0).getId(), equalTo("1"));

        // default query, without specifying relation (expect intersects)
        searchResponse = client().prepareSearch(defaultIndexName)
            .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, geometry))
            .get();

        assertSearchResponse(searchResponse);
        assertThat(searchResponse.getHits().getTotalHits().value, equalTo(1L));
        assertThat(searchResponse.getHits().getHits().length, equalTo(1));
        assertThat(searchResponse.getHits().getAt(0).getId(), equalTo("1"));
    }

    public void testIndexPointsCircle() throws Exception {
        String mapping = Strings.toString(createDefaultMapping());
        client().admin().indices().prepareCreate(defaultIndexName).setMapping(mapping).get();
        ensureGreen();

        client().prepareIndex(defaultIndexName).setId("1").setSource(jsonBuilder()
            .startObject()
            .field("name", "Document 1")
            .field(defaultGeoFieldName, "POINT(-30 -30)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        client().prepareIndex(defaultIndexName).setId("2").setSource(jsonBuilder()
            .startObject()
            .field("name", "Document 2")
            .field(defaultGeoFieldName, "POINT(-45 -50)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        CircleBuilder shape = new CircleBuilder().center(new Coordinate(-30, -30)).radius("100m");
        GeometryCollectionBuilder builder = new GeometryCollectionBuilder().shape(shape);
        Geometry geometry = builder.buildGeometry().get(0);

        try {
            client().prepareSearch(defaultIndexName)
                .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, geometry)
                    .relation(ShapeRelation.INTERSECTS))
                .get();
        } catch (
            Exception e) {
            assertThat(e.getCause().getMessage(),
                containsString("failed to create query: "
                    + GeoShapeType.CIRCLE + " geometry is not supported"));
        }
    }

    public void testIndexPointsPolygon() throws Exception {
        String mapping = Strings.toString(createDefaultMapping());
        client().admin().indices().prepareCreate(defaultIndexName).setMapping(mapping).get();
        ensureGreen();

        client().prepareIndex(defaultIndexName).setId("1").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(-30 -30)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        client().prepareIndex(defaultIndexName).setId("2").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(-45 -50)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        CoordinatesBuilder cb = new CoordinatesBuilder();
        cb.coordinate(new Coordinate(-35, -35))
            .coordinate(new Coordinate(-35, -25))
            .coordinate(new Coordinate(-25, -25))
            .coordinate(new Coordinate(-25, -35))
            .coordinate(new Coordinate(-35, -35));
        PolygonBuilder shape = new PolygonBuilder(cb);
        GeometryCollectionBuilder builder = new GeometryCollectionBuilder().shape(shape);
        Geometry geometry = builder.buildGeometry();
        SearchResponse searchResponse = client().prepareSearch(defaultIndexName)
            .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, geometry)
                .relation(ShapeRelation.INTERSECTS))
            .get();

        assertSearchResponse(searchResponse);
        SearchHits searchHits = searchResponse.getHits();
        assertThat(searchHits.getTotalHits().value, equalTo(1L));
        assertThat(searchHits.getAt(0).getId(), equalTo("1"));
    }

    public void testIndexPointsMultiPolygon() throws Exception {
        String mapping = Strings.toString(createDefaultMapping());
        client().admin().indices().prepareCreate(defaultIndexName).setMapping(mapping).get();
        ensureGreen();

        client().prepareIndex(defaultIndexName).setId("1").setSource(jsonBuilder()
            .startObject()
            .field("name", "Document 1")
            .field(defaultGeoFieldName, "POINT(-30 -30)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        client().prepareIndex(defaultIndexName).setId("2").setSource(jsonBuilder()
            .startObject()
            .field("name", "Document 2")
            .field(defaultGeoFieldName, "POINT(-40 -40)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        client().prepareIndex(defaultIndexName).setId("3").setSource(jsonBuilder()
            .startObject()
            .field("name", "Document 3")
            .field(defaultGeoFieldName, "POINT(-50 -50)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        CoordinatesBuilder encloseDocument1Cb = new CoordinatesBuilder();
        encloseDocument1Cb.coordinate(new Coordinate(-35, -35))
            .coordinate(new Coordinate(-35, -25))
            .coordinate(new Coordinate(-25, -25))
            .coordinate(new Coordinate(-25, -35))
            .coordinate(new Coordinate(-35, -35));
        PolygonBuilder encloseDocument1Shape = new PolygonBuilder(encloseDocument1Cb);

        CoordinatesBuilder encloseDocument2Cb = new CoordinatesBuilder();
        encloseDocument2Cb.coordinate(new Coordinate(-55, -55))
            .coordinate(new Coordinate(-55, -45))
            .coordinate(new Coordinate(-45, -45))
            .coordinate(new Coordinate(-45, -55))
            .coordinate(new Coordinate(-55, -55));
        PolygonBuilder encloseDocument2Shape = new PolygonBuilder(encloseDocument2Cb);

        MultiPolygonBuilder mp = new MultiPolygonBuilder();
        mp.polygon(encloseDocument1Shape).polygon(encloseDocument2Shape);

        GeometryCollectionBuilder builder = new GeometryCollectionBuilder().shape(mp);
        Geometry geometry = builder.buildGeometry();
        {
            SearchResponse searchResponse = client().prepareSearch(defaultIndexName)
                .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, geometry)
                    .relation(ShapeRelation.INTERSECTS))
                .get();

            assertSearchResponse(searchResponse);
            assertThat(searchResponse.getHits().getTotalHits().value, equalTo(2L));
            assertThat(searchResponse.getHits().getHits().length, equalTo(2));
            assertThat(searchResponse.getHits().getAt(0).getId(), not(equalTo("2")));
            assertThat(searchResponse.getHits().getAt(1).getId(), not(equalTo("2")));
        }
        {
            SearchResponse searchResponse = client().prepareSearch(defaultIndexName)
                .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, geometry)
                    .relation(ShapeRelation.WITHIN))
                .get();

            assertSearchResponse(searchResponse);
            assertThat(searchResponse.getHits().getTotalHits().value, equalTo(2L));
            assertThat(searchResponse.getHits().getHits().length, equalTo(2));
            assertThat(searchResponse.getHits().getAt(0).getId(), not(equalTo("2")));
            assertThat(searchResponse.getHits().getAt(1).getId(), not(equalTo("2")));
        }
        {
            SearchResponse searchResponse = client().prepareSearch(defaultIndexName)
                .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, geometry)
                    .relation(ShapeRelation.DISJOINT))
                .get();

            assertSearchResponse(searchResponse);
            assertThat(searchResponse.getHits().getTotalHits().value, equalTo(1L));
            assertThat(searchResponse.getHits().getHits().length, equalTo(1));
            assertThat(searchResponse.getHits().getAt(0).getId(), equalTo("2"));
        }
        {
            SearchResponse searchResponse = client().prepareSearch(defaultIndexName)
                .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, geometry)
                    .relation(ShapeRelation.CONTAINS))
                .get();

            assertSearchResponse(searchResponse);
            assertThat(searchResponse.getHits().getTotalHits().value, equalTo(0L));
            assertThat(searchResponse.getHits().getHits().length, equalTo(0));
        }
    }

    public void testIndexPointsRectangle() throws Exception {
        String mapping = Strings.toString(createDefaultMapping());
        client().admin().indices().prepareCreate(defaultIndexName).setMapping(mapping).get();
        ensureGreen();

        client().prepareIndex(defaultIndexName).setId("1").setSource(jsonBuilder()
            .startObject()
            .field("name", "Document 1")
            .field(defaultGeoFieldName, "POINT(-30 -30)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        client().prepareIndex(defaultIndexName).setId("2").setSource(jsonBuilder()
            .startObject()
            .field("name", "Document 2")
            .field(defaultGeoFieldName, "POINT(-45 -50)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        Rectangle rectangle = new Rectangle(-50, -40, -45, -55);

        SearchResponse searchResponse = client().prepareSearch(defaultIndexName)
            .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, rectangle)
                .relation(ShapeRelation.INTERSECTS))
            .get();

        assertSearchResponse(searchResponse);
        assertThat(searchResponse.getHits().getTotalHits().value, equalTo(1L));
        assertThat(searchResponse.getHits().getHits().length, equalTo(1));
        assertThat(searchResponse.getHits().getAt(0).getId(), equalTo("2"));
    }

    public void testIndexPointsIndexedRectangle() throws Exception {
        String mapping = Strings.toString(createDefaultMapping());
        client().admin().indices().prepareCreate(defaultIndexName).setMapping(mapping).get();
        ensureGreen();

        client().prepareIndex(defaultIndexName).setId("point1").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(-30 -30)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        client().prepareIndex(defaultIndexName).setId("point2").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(-45 -50)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        String indexedShapeIndex = "indexed_query_shapes";
        String indexedShapePath = "shape";
        String queryShapesMapping = Strings.toString(XContentFactory.jsonBuilder().startObject()
            .startObject("properties").startObject(indexedShapePath)
            .field("type", "geo_shape")
            .endObject()
            .endObject()
            .endObject());
        client().admin().indices().prepareCreate(indexedShapeIndex).setMapping(queryShapesMapping).get();
        ensureGreen();

        client().prepareIndex(indexedShapeIndex).setId("shape1").setSource(jsonBuilder()
            .startObject()
            .field(indexedShapePath, "BBOX(-50, -40, -45, -55)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        client().prepareIndex(indexedShapeIndex).setId("shape2").setSource(jsonBuilder()
            .startObject()
            .field(indexedShapePath, "BBOX(-60, -50, -50, -60)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        SearchResponse searchResponse = client().prepareSearch(defaultIndexName)
            .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, "shape1")
                .relation(ShapeRelation.INTERSECTS)
                .indexedShapeIndex(indexedShapeIndex)
                .indexedShapePath(indexedShapePath))
            .get();

        assertSearchResponse(searchResponse);
        assertThat(searchResponse.getHits().getTotalHits().value, equalTo(1L));
        assertThat(searchResponse.getHits().getHits().length, equalTo(1));
        assertThat(searchResponse.getHits().getAt(0).getId(), equalTo("point2"));

        searchResponse = client().prepareSearch(defaultIndexName)
            .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, "shape2")
                .relation(ShapeRelation.INTERSECTS)
                .indexedShapeIndex(indexedShapeIndex)
                .indexedShapePath(indexedShapePath))
            .get();
        assertSearchResponse(searchResponse);
        assertThat(searchResponse.getHits().getTotalHits().value, equalTo(0L));
    }

    public void testRectangleSpanningDateline() throws Exception {
        XContentBuilder mapping = createDefaultMapping();
        client().admin().indices().prepareCreate("test").setMapping(mapping).get();
        ensureGreen();

        client().prepareIndex(defaultIndexName).setId("1").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(-169 0)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        client().prepareIndex(defaultIndexName).setId("2").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(-179 0)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        client().prepareIndex(defaultIndexName).setId("3").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(171 0)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        Rectangle rectangle = new Rectangle(
            169, -178, 1, -1);

        GeoShapeQueryBuilder geoShapeQueryBuilder = QueryBuilders.geoShapeQuery("geo", rectangle);
        SearchResponse response = client().prepareSearch("test").setQuery(geoShapeQueryBuilder).get();
        SearchHits searchHits = response.getHits();
        assertEquals(2, searchHits.getTotalHits().value);
        assertNotEquals("1", searchHits.getAt(0).getId());
        assertNotEquals("1", searchHits.getAt(1).getId());
    }

    public void testPolygonSpanningDateline() throws Exception {
        XContentBuilder mapping = createDefaultMapping();
        client().admin().indices().prepareCreate("test").setMapping(mapping).get();
        ensureGreen();

        client().prepareIndex(defaultIndexName).setId("1").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(-169 7)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        client().prepareIndex(defaultIndexName).setId("2").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(-179 7)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        client().prepareIndex(defaultIndexName).setId("3").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(179 7)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        client().prepareIndex(defaultIndexName).setId("4").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(171 7)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        PolygonBuilder polygon = new PolygonBuilder(new CoordinatesBuilder()
                    .coordinate(-177, 10)
                    .coordinate(177, 10)
                    .coordinate(177, 5)
                    .coordinate(-177, 5)
                    .coordinate(-177, 10));

        GeoShapeQueryBuilder geoShapeQueryBuilder = QueryBuilders.geoShapeQuery("geo", polygon.buildGeometry());
        geoShapeQueryBuilder.relation(ShapeRelation.INTERSECTS);
        SearchResponse response = client().prepareSearch("test").setQuery(geoShapeQueryBuilder).get();
        SearchHits searchHits = response.getHits();
        assertEquals(2, searchHits.getTotalHits().value);
        assertNotEquals("1", searchHits.getAt(0).getId());
        assertNotEquals("4", searchHits.getAt(0).getId());
        assertNotEquals("1", searchHits.getAt(1).getId());
        assertNotEquals("4", searchHits.getAt(1).getId());
    }

    public void testMultiPolygonSpanningDateline() throws Exception {
        XContentBuilder mapping = createDefaultMapping();
        client().admin().indices().prepareCreate("test").setMapping(mapping).get();
        ensureGreen();

        client().prepareIndex(defaultIndexName).setId("1").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(-169 7)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        client().prepareIndex(defaultIndexName).setId("2").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(-179 7)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        client().prepareIndex(defaultIndexName).setId("3").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(171 7)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        MultiPolygonBuilder multiPolygon = new MultiPolygonBuilder()
            .polygon(new PolygonBuilder(new CoordinatesBuilder()
                .coordinate(-167, 10)
                .coordinate(-171, 10)
                .coordinate(171, 5)
                .coordinate(-167, 5)
                .coordinate(-167, 10)))
            .polygon(new PolygonBuilder(new CoordinatesBuilder()
                .coordinate(-177, 10)
                .coordinate(177, 10)
                .coordinate(177, 5)
                .coordinate(-177, 5)
                .coordinate(-177, 10)));

        GeoShapeQueryBuilder geoShapeQueryBuilder = QueryBuilders.geoShapeQuery(
            "geo",
            multiPolygon.buildGeometry());
        geoShapeQueryBuilder.relation(ShapeRelation.INTERSECTS);
        SearchResponse response = client().prepareSearch("test").setQuery(geoShapeQueryBuilder).get();
        SearchHits searchHits = response.getHits();
        assertEquals(2, searchHits.getTotalHits().value);
        assertNotEquals("3", searchHits.getAt(0).getId());
        assertNotEquals("3", searchHits.getAt(1).getId());
    }

    public void testWithInQueryLine() throws Exception {
        String mapping = Strings.toString(createDefaultMapping());
        client().admin().indices().prepareCreate("test").setMapping(mapping).get();
        ensureGreen();

        Line line = new Line(new double[]{-25, -25}, new double[]{-35, -35});

        try {
            client().prepareSearch("test")
                .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, line).relation(ShapeRelation.WITHIN)).get();
        } catch (
            SearchPhaseExecutionException e) {
            assertThat(e.getCause().getMessage(),
                containsString("Field [geo] found an unsupported shape Line"));
        }
    }

    public void testQueryWithinMultiLine() throws Exception {
        String mapping = Strings.toString(createDefaultMapping());
        client().admin().indices().prepareCreate("test").setMapping(mapping).get();
        ensureGreen();

        CoordinatesBuilder coords1 = new CoordinatesBuilder()
            .coordinate(-35,-35)
            .coordinate(-25,-25);
        CoordinatesBuilder coords2 = new CoordinatesBuilder()
            .coordinate(-15,-15)
            .coordinate(-5,-5);
        LineStringBuilder lsb1 = new LineStringBuilder(coords1);
        LineStringBuilder lsb2 = new LineStringBuilder(coords2);
        MultiLineStringBuilder mlb = new MultiLineStringBuilder().linestring(lsb1).linestring(lsb2);
        MultiLine multiline = (MultiLine) mlb.buildGeometry();

        GeoShapeQueryBuilder builder = QueryBuilders.geoShapeQuery(defaultGeoFieldName, multiline).relation(ShapeRelation.WITHIN);
        SearchRequestBuilder searchRequestBuilder = client().prepareSearch("test").setQuery(builder);
        SearchPhaseExecutionException e = expectThrows(SearchPhaseExecutionException.class, searchRequestBuilder::get);
        assertThat(e.getCause().getMessage(),
            containsString("Field [" + defaultGeoFieldName + "] found an unsupported shape Line"));
    }

    public void testQueryLinearRing() throws Exception {
        String mapping = Strings.toString(createDefaultMapping());
        client().admin().indices().prepareCreate("test").setMapping(mapping).get();
        ensureGreen();

        LinearRing linearRing = new LinearRing(new double[]{-25, -35, -25}, new double[]{-25, -35, -25});

        // LinearRing extends Line implements Geometry: expose the build process
        GeoShapeQueryBuilder queryBuilder = new GeoShapeQueryBuilder(defaultGeoFieldName, linearRing);
        SearchRequestBuilder searchRequestBuilder = new SearchRequestBuilder(client(), SearchAction.INSTANCE);
        searchRequestBuilder.setQuery(queryBuilder);
        searchRequestBuilder.setIndices("test");
        SearchPhaseExecutionException e = expectThrows(SearchPhaseExecutionException.class, searchRequestBuilder::get);
        assertThat(e.getCause().getMessage(),
            containsString("Field [" + defaultGeoFieldName + "] found an unsupported shape LinearRing"));
    }

    public void testQueryPoint() throws Exception {
        String mapping = Strings.toString(createDefaultMapping());
        client().admin().indices().prepareCreate("test").setMapping(mapping).get();
        ensureGreen();

        client().prepareIndex("test").setId("1").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(-35 -25)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        PointBuilder pb = new PointBuilder().coordinate(-35, -25);
        Point point = pb.buildGeometry();
        {
            SearchResponse response = client().prepareSearch("test")
                .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, point)).get();
            SearchHits searchHits = response.getHits();
            assertEquals(1, searchHits.getTotalHits().value);
        }
        {
            SearchResponse response = client().prepareSearch("test")
                .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, point).relation(ShapeRelation.WITHIN)).get();
            SearchHits searchHits = response.getHits();
            assertEquals(1, searchHits.getTotalHits().value);
        }
        {
            SearchResponse response = client().prepareSearch("test")
                .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, point).relation(ShapeRelation.CONTAINS)).get();
            SearchHits searchHits = response.getHits();
            assertEquals(1, searchHits.getTotalHits().value);
        }
        {
            SearchResponse response = client().prepareSearch("test")
                .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, point).relation(ShapeRelation.DISJOINT)).get();
            SearchHits searchHits = response.getHits();
            assertEquals(0, searchHits.getTotalHits().value);
        }
    }

    public void testQueryMultiPoint() throws Exception {
        String mapping = Strings.toString(createDefaultMapping());
        client().admin().indices().prepareCreate("test").setMapping(mapping).get();
        ensureGreen();

        client().prepareIndex("test").setId("1").setSource(jsonBuilder()
            .startObject()
            .field(defaultGeoFieldName, "POINT(-35 -25)")
            .endObject()).setRefreshPolicy(IMMEDIATE).get();

        MultiPointBuilder mpb = new MultiPointBuilder().coordinate(-35,-25).coordinate(-15,-5);
        MultiPoint multiPoint = mpb.buildGeometry();

        {
            SearchResponse response = client().prepareSearch("test")
                .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, multiPoint)).get();
            SearchHits searchHits = response.getHits();
            assertEquals(1, searchHits.getTotalHits().value);
        }
        {
            SearchResponse response = client().prepareSearch("test")
                .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, multiPoint).relation(ShapeRelation.WITHIN)).get();
            SearchHits searchHits = response.getHits();
            assertEquals(1, searchHits.getTotalHits().value);
        }
        {
            SearchResponse response = client().prepareSearch("test")
                .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, multiPoint).relation(ShapeRelation.CONTAINS)).get();
            SearchHits searchHits = response.getHits();
            assertEquals(0, searchHits.getTotalHits().value);
        }
        {
            SearchResponse response = client().prepareSearch("test")
                .setQuery(QueryBuilders.geoShapeQuery(defaultGeoFieldName, multiPoint).relation(ShapeRelation.DISJOINT)).get();
            SearchHits searchHits = response.getHits();
            assertEquals(0, searchHits.getTotalHits().value);
        }
    }
}
