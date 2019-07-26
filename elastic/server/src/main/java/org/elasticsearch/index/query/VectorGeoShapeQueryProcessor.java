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

package org.elasticsearch.index.query;

import org.apache.lucene.document.LatLonShape;
import org.apache.lucene.geo.Line;
import org.apache.lucene.geo.Polygon;
import org.apache.lucene.search.BooleanClause;
import org.apache.lucene.search.BooleanQuery;
import org.apache.lucene.search.MatchNoDocsQuery;
import org.apache.lucene.search.Query;
import org.elasticsearch.common.geo.GeoShapeType;
import org.elasticsearch.common.geo.ShapeRelation;
import org.elasticsearch.common.geo.SpatialStrategy;
import org.elasticsearch.geo.geometry.Circle;
import org.elasticsearch.geo.geometry.Geometry;
import org.elasticsearch.geo.geometry.GeometryCollection;
import org.elasticsearch.geo.geometry.GeometryVisitor;
import org.elasticsearch.geo.geometry.LinearRing;
import org.elasticsearch.geo.geometry.MultiLine;
import org.elasticsearch.geo.geometry.MultiPoint;
import org.elasticsearch.geo.geometry.MultiPolygon;
import org.elasticsearch.geo.geometry.Point;
import org.elasticsearch.index.mapper.AbstractGeometryFieldMapper;
import org.elasticsearch.index.mapper.GeoShapeFieldMapper;
import org.elasticsearch.index.mapper.GeoShapeIndexer;
import org.elasticsearch.index.mapper.MappedFieldType;

import static org.elasticsearch.index.mapper.GeoShapeFieldMapper.toLucenePolygon;

public class VectorGeoShapeQueryProcessor implements AbstractGeometryFieldMapper.QueryProcessor {

    @Override
    public Query process(Geometry shape, String fieldName, SpatialStrategy strategy, ShapeRelation relation, QueryShardContext context) {
        // CONTAINS queries are not yet supported by VECTOR strategy
        if (relation == ShapeRelation.CONTAINS) {
            throw new QueryShardException(context,
                ShapeRelation.CONTAINS + " query relation not supported for Field [" + fieldName + "]");
        }
        // wrap geoQuery as a ConstantScoreQuery
        return getVectorQueryFromShape(shape, fieldName, relation, context);
    }

    protected Query getVectorQueryFromShape(Geometry queryShape, String fieldName, ShapeRelation relation, QueryShardContext context) {
        GeoShapeIndexer geometryIndexer = new GeoShapeIndexer(true);

        Geometry processedShape = geometryIndexer.prepareForIndexing(queryShape);

        if (processedShape == null) {
            return new MatchNoDocsQuery();
        }
        return queryShape.visit(new ShapeVisitor(context, fieldName, relation));
    }

    private class ShapeVisitor implements GeometryVisitor<Query, RuntimeException> {
        QueryShardContext context;
        MappedFieldType fieldType;
        String fieldName;
        ShapeRelation relation;

        ShapeVisitor(QueryShardContext context, String fieldName, ShapeRelation relation) {
            this.context = context;
            this.fieldType = context.fieldMapper(fieldName);
            this.fieldName = fieldName;
            this.relation = relation;
        }

        @Override
        public Query visit(Circle circle) {
            throw new QueryShardException(context, "Field [" + fieldName + "] found and unknown shape Circle");
        }

        @Override
        public Query visit(GeometryCollection<?> collection) {
            BooleanQuery.Builder bqb = new BooleanQuery.Builder();
            visit(bqb, collection);
            return bqb.build();
        }

        private void visit(BooleanQuery.Builder bqb, GeometryCollection<?> collection) {
            for (Geometry shape : collection) {
                if (shape instanceof MultiPoint) {
                    // Flatten multipoints
                    visit(bqb, (GeometryCollection<?>) shape);
                } else {
                    bqb.add(shape.visit(this), BooleanClause.Occur.SHOULD);
                }
            }
        }

        @Override
        public Query visit(org.elasticsearch.geo.geometry.Line line) {
            validateIsGeoShapeFieldType();
            return LatLonShape.newLineQuery(fieldName, relation.getLuceneRelation(), new Line(line.getLats(), line.getLons()));
        }

        @Override
        public Query visit(LinearRing ring) {
            throw new QueryShardException(context, "Field [" + fieldName + "] found and unsupported shape LinearRing");
        }

        @Override
        public Query visit(MultiLine multiLine) {
            validateIsGeoShapeFieldType();
            Line[] lines = new Line[multiLine.size()];
            for (int i = 0; i < multiLine.size(); i++) {
                lines[i] = new Line(multiLine.get(i).getLats(), multiLine.get(i).getLons());
            }
            return LatLonShape.newLineQuery(fieldName, relation.getLuceneRelation(), lines);
        }

        @Override
        public Query visit(MultiPoint multiPoint) {
            throw new QueryShardException(context, "Field [" + fieldName + "] does not support " + GeoShapeType.MULTIPOINT +
                " queries");
        }

        @Override
        public Query visit(MultiPolygon multiPolygon) {
            Polygon[] polygons = new Polygon[multiPolygon.size()];
            for (int i = 0; i < multiPolygon.size(); i++) {
                polygons[i] = toLucenePolygon(multiPolygon.get(i));
            }
            return LatLonShape.newPolygonQuery(fieldName, relation.getLuceneRelation(), polygons);
        }

        @Override
        public Query visit(Point point) {
            validateIsGeoShapeFieldType();
            return LatLonShape.newBoxQuery(fieldName, relation.getLuceneRelation(),
                point.getLat(), point.getLat(), point.getLon(), point.getLon());
        }

        @Override
        public Query visit(org.elasticsearch.geo.geometry.Polygon polygon) {
            return LatLonShape.newPolygonQuery(fieldName, relation.getLuceneRelation(), toLucenePolygon(polygon));
        }

        @Override
        public Query visit(org.elasticsearch.geo.geometry.Rectangle r) {
            return LatLonShape.newBoxQuery(fieldName, relation.getLuceneRelation(),
                r.getMinLat(), r.getMaxLat(), r.getMinLon(), r.getMaxLon());
        }

        private void validateIsGeoShapeFieldType() {
            if (fieldType instanceof GeoShapeFieldMapper.GeoShapeFieldType == false) {
                throw new QueryShardException(context, "Expected " + GeoShapeFieldMapper.CONTENT_TYPE
                    + " field type for Field [" + fieldName + "] but found " + fieldType.typeName());
            }
        }
    }

}

