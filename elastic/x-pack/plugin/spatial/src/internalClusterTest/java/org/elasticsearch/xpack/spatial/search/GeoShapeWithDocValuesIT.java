/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.spatial.search;

import org.elasticsearch.Version;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.unit.DistanceUnit;
import org.elasticsearch.geometry.LinearRing;
import org.elasticsearch.geometry.Polygon;
import org.elasticsearch.index.mapper.MapperParsingException;
import org.elasticsearch.percolator.PercolateQueryBuilder;
import org.elasticsearch.percolator.PercolatorPlugin;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.search.geo.GeoShapeIntegTestCase;
import org.elasticsearch.search.sort.SortOrder;
import org.elasticsearch.test.VersionUtils;
import org.elasticsearch.xcontent.XContentBuilder;
import org.elasticsearch.xcontent.XContentType;
import org.elasticsearch.xpack.spatial.LocalStateSpatialPlugin;

import java.io.IOException;
import java.util.Collection;
import java.util.List;

import static org.elasticsearch.index.query.QueryBuilders.geoBoundingBoxQuery;
import static org.elasticsearch.index.query.QueryBuilders.geoDistanceQuery;
import static org.elasticsearch.index.query.QueryBuilders.geoShapeQuery;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertHitCount;
import static org.elasticsearch.xcontent.XContentFactory.jsonBuilder;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;

public class GeoShapeWithDocValuesIT extends GeoShapeIntegTestCase {

    @Override
    protected Collection<Class<? extends Plugin>> nodePlugins() {
        return List.of(LocalStateSpatialPlugin.class, PercolatorPlugin.class);
    }

    @Override
    protected void getGeoShapeMapping(XContentBuilder b) throws IOException {
        b.field("type", "geo_shape");
    }

    @Override
    protected Version randomSupportedVersion() {
        return VersionUtils.randomIndexCompatibleVersion(random());
    }

    @Override
    protected boolean allowExpensiveQueries() {
        return true;
    }

    public void testMappingUpdate() {
        // create index
        Version version = randomSupportedVersion();
        assertAcked(
            client().admin()
                .indices()
                .prepareCreate("test")
                .setSettings(settings(version).build())
                .setMapping("shape", "type=geo_shape")
                .get()
        );
        ensureGreen();

        String update = """
            {
              "properties": {
                "shape": {
                  "type": "geo_shape",
                  "strategy": "recursive"
                }
              }
            }""";

        if (version.before(Version.V_8_0_0)) {
            IllegalArgumentException e = expectThrows(
                IllegalArgumentException.class,
                () -> client().admin().indices().preparePutMapping("test").setSource(update, XContentType.JSON).get()
            );
            assertThat(
                e.getMessage(),
                containsString("mapper [shape] of type [geo_shape] cannot change strategy from [BKD] to [recursive]")
            );
        } else {
            MapperParsingException e = expectThrows(
                MapperParsingException.class,
                () -> client().admin().indices().preparePutMapping("test").setSource(update, XContentType.JSON).get()
            );
            assertThat(
                e.getMessage(),
                containsString("using deprecated parameters [strategy] in mapper [shape] of type [geo_shape] is no longer allowed")
            );
        }
    }

    public void testPercolatorGeoQueries() throws Exception {
        assertAcked(
            client().admin()
                .indices()
                .prepareCreate("test")
                .setMapping("id", "type=keyword", "field1", "type=geo_shape", "query", "type=percolator")
        );

        client().prepareIndex("test")
            .setId("1")
            .setSource(
                jsonBuilder().startObject()
                    .field("query", geoDistanceQuery("field1").point(52.18, 4.38).distance(50, DistanceUnit.KILOMETERS))
                    .field("id", "1")
                    .endObject()
            )
            .get();

        client().prepareIndex("test")
            .setId("2")
            .setSource(
                jsonBuilder().startObject()
                    .field("query", geoBoundingBoxQuery("field1").setCorners(52.3, 4.4, 52.1, 4.6))
                    .field("id", "2")
                    .endObject()
            )
            .get();

        client().prepareIndex("test")
            .setId("3")
            .setSource(
                jsonBuilder().startObject()
                    .field(
                        "query",
                        geoShapeQuery(
                            "field1",
                            new Polygon(new LinearRing(new double[] { 4.4, 4.5, 4.6, 4.4 }, new double[] { 52.1, 52.3, 52.1, 52.1 }))
                        )
                    )
                    .field("id", "3")
                    .endObject()
            )
            .get();
        refresh();

        BytesReference source = BytesReference.bytes(jsonBuilder().startObject().field("field1", "POINT(4.51 52.20)").endObject());
        SearchResponse response = client().prepareSearch()
            .setQuery(new PercolateQueryBuilder("query", source, XContentType.JSON))
            .addSort("id", SortOrder.ASC)
            .get();
        assertHitCount(response, 3);
        assertThat(response.getHits().getAt(0).getId(), equalTo("1"));
        assertThat(response.getHits().getAt(1).getId(), equalTo("2"));
        assertThat(response.getHits().getAt(2).getId(), equalTo("3"));
    }
}
