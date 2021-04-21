/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.spatial.index.fielddata;

import org.elasticsearch.index.mapper.GeoShapeIndexer;
import org.elasticsearch.geometry.Geometry;
import org.elasticsearch.geometry.Rectangle;
import org.elasticsearch.geometry.utils.GeographyValidator;
import org.elasticsearch.geometry.utils.WellKnownText;
import org.elasticsearch.search.aggregations.support.ValuesSourceType;
import org.elasticsearch.xpack.spatial.index.mapper.BinaryGeoShapeDocValuesField;
import org.elasticsearch.xpack.spatial.search.aggregations.support.GeoShapeValuesSourceType;

import java.io.IOException;
import java.text.ParseException;

/**
 * A stateful lightweight per document geo values.
 * To iterate over values in a document use the following pattern:
 * <pre>
 *   MultiGeoValues values = ..;
 *   // for each docID
 *   if (values.advanceExact(docId)) {
 *     GeoValue value = values.value()
 *     final int numValues = values.count();
 *     // process value
 *   }
 * </pre>
 *
 * There is just one value for one document.
 */
public abstract class GeoShapeValues {

    public static GeoShapeValues EMPTY = new GeoShapeValues() {
        private GeoShapeValuesSourceType DEFAULT_VALUES_SOURCE_TYPE = GeoShapeValuesSourceType.instance();
        @Override
        public boolean advanceExact(int doc) {
            return false;
        }

        @Override
        public ValuesSourceType valuesSourceType() {
            return DEFAULT_VALUES_SOURCE_TYPE;
        }

        @Override
        public GeoShapeValue value() {
            throw new UnsupportedOperationException();
        }
    };

    /**
     * Creates a new {@link GeoShapeValues} instance
     */
    protected GeoShapeValues() {
    }

    /**
     * Advance this instance to the given document id
     * @return true if there is a value for this document
     */
    public abstract boolean advanceExact(int doc) throws IOException;


    public abstract ValuesSourceType valuesSourceType();

    /**
     * Return the value associated with the current document.
     *
     * Note: the returned {@link GeoShapeValue} might be shared across invocations.
     *
     * @return the value for the current docID set to {@link #advanceExact(int)}.
     */
    public abstract GeoShapeValue value() throws IOException;

    /** thin wrapper around a {@link GeometryDocValueReader} which encodes / decodes values using
     * the Geo decoder */
    public static class GeoShapeValue {
        private static final WellKnownText MISSING_GEOMETRY_PARSER = new WellKnownText(true, new GeographyValidator(true));

        private final GeometryDocValueReader reader;
        private final BoundingBox boundingBox;
        private final Tile2DVisitor tile2DVisitor;

        public GeoShapeValue(GeometryDocValueReader reader)  {
            this.reader = reader;
            this.boundingBox = new BoundingBox();
            tile2DVisitor = new Tile2DVisitor();
        }

        public BoundingBox boundingBox() {
            boundingBox.reset(reader.getExtent(), CoordinateEncoder.GEO);
            return boundingBox;
        }

        public GeoRelation relate(Rectangle rectangle) {
            int minX = CoordinateEncoder.GEO.encodeX(rectangle.getMinX());
            int maxX = CoordinateEncoder.GEO.encodeX(rectangle.getMaxX());
            int minY = CoordinateEncoder.GEO.encodeY(rectangle.getMinY());
            int maxY = CoordinateEncoder.GEO.encodeY(rectangle.getMaxY());
            tile2DVisitor.reset(minX, minY, maxX, maxY);
            reader.visit(tile2DVisitor);
            return tile2DVisitor.relation();
        }

        public DimensionalShapeType dimensionalShapeType() {
            return reader.getDimensionalShapeType();
        }

        public double weight() {
            return reader.getSumCentroidWeight();
        }

        /**
         * @return the latitude of the centroid of the shape
         */
        public double lat() {
            return CoordinateEncoder.GEO.decodeY(reader.getCentroidY());
        }

        /**
         * @return the longitude of the centroid of the shape
         */
        public double lon() {
            return CoordinateEncoder.GEO.decodeX(reader.getCentroidX());
        }

        public static GeoShapeValue missing(String missing) {
            try {
                final GeoShapeIndexer indexer = new GeoShapeIndexer(true, "missing");
                final Geometry geometry = indexer.prepareForIndexing(MISSING_GEOMETRY_PARSER.fromWKT(missing));
                final BinaryGeoShapeDocValuesField field = new BinaryGeoShapeDocValuesField("missing");
                field.add(indexer.indexShape(geometry), geometry);
                final GeometryDocValueReader reader = new GeometryDocValueReader();
                reader.reset(field.binaryValue());
                return new GeoShapeValue(reader);
            } catch (IOException | ParseException e) {
                throw new IllegalArgumentException("Can't apply missing value [" + missing + "]", e);
            }
        }
    }

    public static class BoundingBox {
        public double top;
        public double bottom;
        public double negLeft;
        public double negRight;
        public double posLeft;
        public double posRight;

        private BoundingBox() {
        }

        private void reset(Extent extent, CoordinateEncoder coordinateEncoder) {
            this.top = coordinateEncoder.decodeY(extent.top);
            this.bottom = coordinateEncoder.decodeY(extent.bottom);

            if (extent.negLeft == Integer.MAX_VALUE && extent.negRight == Integer.MIN_VALUE) {
                this.negLeft = Double.POSITIVE_INFINITY;
                this.negRight = Double.NEGATIVE_INFINITY;
            } else {
                this.negLeft = coordinateEncoder.decodeX(extent.negLeft);
                this.negRight = coordinateEncoder.decodeX(extent.negRight);
            }

            if (extent.posLeft == Integer.MAX_VALUE && extent.posRight == Integer.MIN_VALUE) {
                this.posLeft = Double.POSITIVE_INFINITY;
                this.posRight = Double.NEGATIVE_INFINITY;
            } else {
                this.posLeft = coordinateEncoder.decodeX(extent.posLeft);
                this.posRight = coordinateEncoder.decodeX(extent.posRight);
            }
        }

        /**
         * @return the minimum y-coordinate of the extent
         */
        public double minY() {
            return bottom;
        }

        /**
         * @return the maximum y-coordinate of the extent
         */
        public double maxY() {
            return top;
        }

        /**
         * @return the absolute minimum x-coordinate of the extent, whether it is positive or negative.
         */
        public double minX() {
            return Math.min(negLeft, posLeft);
        }

        /**
         * @return the absolute maximum x-coordinate of the extent, whether it is positive or negative.
         */
        public double maxX() {
            return Math.max(negRight, posRight);
        }
    }
}
