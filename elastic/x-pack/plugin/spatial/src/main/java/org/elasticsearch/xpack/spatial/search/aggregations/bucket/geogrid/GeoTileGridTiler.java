/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.spatial.search.aggregations.bucket.geogrid;

import org.elasticsearch.geometry.Rectangle;
import org.elasticsearch.search.aggregations.bucket.geogrid.GeoTileUtils;
import org.elasticsearch.xpack.spatial.index.fielddata.GeoRelation;
import org.elasticsearch.xpack.spatial.index.fielddata.GeoShapeValues;

public class GeoTileGridTiler implements GeoGridTiler {

    @Override
    public long encode(double x, double y, int precision) {
        return GeoTileUtils.longEncode(x, y, precision);
    }

    public int advancePointValue(long[] values, double x, double y, int precision, int valuesIdx) {
        values[valuesIdx] = encode(x, y, precision);
        return valuesIdx + 1;
    }

    /**
     * Sets the values of the long[] underlying {@link GeoShapeCellValues}.
     *
     * If the shape resides between <code>GeoTileUtils.NORMALIZED_LATITUDE_MASK</code> and 90 or
     * between <code>GeoTileUtils.NORMALIZED_NEGATIVE_LATITUDE_MASK</code> and -90 degree latitudes, then
     * the shape is not accounted for since geo-tiles are only defined within those bounds.
     *
     * @param values           the bucket values
     * @param geoValue         the input shape
     * @param precision        the tile zoom-level
     *
     * @return the number of tiles set by the shape
     */
    @Override
    public int setValues(GeoShapeCellValues values, GeoShapeValues.GeoShapeValue geoValue, int precision) {
        GeoShapeValues.BoundingBox bounds = geoValue.boundingBox();
        assert bounds.minX() <= bounds.maxX();

        if (precision == 0) {
            values.resizeCell(1);
            values.add(0, GeoTileUtils.longEncodeTiles(0, 0, 0));
            return 1;
        }

        // geo tiles are not defined at the extreme latitudes due to them
        // tiling the world as a square.
        if ((bounds.top > GeoTileUtils.NORMALIZED_LATITUDE_MASK && bounds.bottom > GeoTileUtils.NORMALIZED_LATITUDE_MASK)
            || (bounds.top < GeoTileUtils.NORMALIZED_NEGATIVE_LATITUDE_MASK
            && bounds.bottom < GeoTileUtils.NORMALIZED_NEGATIVE_LATITUDE_MASK)) {
            return 0;
        }

        final double tiles = 1 << precision;
        int minXTile = GeoTileUtils.getXTile(bounds.minX(), (long) tiles);
        int minYTile = GeoTileUtils.getYTile(bounds.maxY(), (long) tiles);
        int maxXTile = GeoTileUtils.getXTile(bounds.maxX(), (long) tiles);
        int maxYTile = GeoTileUtils.getYTile(bounds.minY(), (long) tiles);
        long count = (long) (maxXTile - minXTile + 1) * (maxYTile - minYTile + 1);
        if (count == 1) {
            return setValue(values, geoValue, minXTile, minYTile, precision);
        } else if (count <= precision) {
            return setValuesByBruteForceScan(values, geoValue, precision, minXTile, minYTile, maxXTile, maxYTile);
        } else {
            final long maxtiles = getMaxTilesAtPrecision(precision);
            return setValuesByRasterization(0, 0, 0, values, 0, precision, geoValue, maxtiles);
        }
    }

    protected GeoRelation relateTile(GeoShapeValues.GeoShapeValue geoValue, int xTile, int yTile, int precision) {
        Rectangle rectangle = GeoTileUtils.toBoundingBox(xTile, yTile, precision);
        return geoValue.relate(rectangle);
    }

    /**
     * Sets a singular doc-value for the {@link GeoShapeValues.GeoShapeValue}. To be overriden by {@link BoundedGeoTileGridTiler}
     * to account for {@link org.elasticsearch.common.geo.GeoBoundingBox} conditions
     */
    protected int setValue(GeoShapeCellValues docValues, GeoShapeValues.GeoShapeValue geoValue, int xTile, int yTile, int precision) {
        docValues.resizeCell(1);
        docValues.add(0, GeoTileUtils.longEncodeTiles(precision, xTile, yTile));
        return 1;
    }

    /**
     *
     * @param values the bucket values as longs
     * @param geoValue the shape value
     * @param precision the target precision to split the shape up into
     * @return the number of buckets the geoValue is found in
     */
    protected int setValuesByBruteForceScan(GeoShapeCellValues values, GeoShapeValues.GeoShapeValue geoValue,
                                            int precision, int minXTile, int minYTile, int maxXTile, int maxYTile) {
        int idx = 0;
        for (int i = minXTile; i <= maxXTile; i++) {
            for (int j = minYTile; j <= maxYTile; j++) {
                GeoRelation relation = relateTile(geoValue, i, j, precision);
                if (relation != GeoRelation.QUERY_DISJOINT) {
                    values.resizeCell(idx + 1);
                    values.add(idx++, GeoTileUtils.longEncodeTiles(precision, i, j));
                }
            }
        }
        return idx;
    }

    protected int setValuesByRasterization(int xTile, int yTile, int zTile, GeoShapeCellValues values, int valuesIndex,
                                           int targetPrecision, GeoShapeValues.GeoShapeValue geoValue, long maxtiles) {
        zTile++;
        for (int i = 0; i < 2; i++) {
            for (int j = 0; j < 2; j++) {
                int nextX = 2 * xTile + i;
                int nextY = 2 * yTile + j;
                GeoRelation relation = relateTile(geoValue, nextX, nextY, zTile);
                if (GeoRelation.QUERY_INSIDE == relation) {
                    if (zTile == targetPrecision) {
                        values.resizeCell(getNewSize(valuesIndex, 1));
                        values.add(valuesIndex++, GeoTileUtils.longEncodeTiles(zTile, nextX, nextY));
                    } else {
                        int numTilesAtPrecision = getNumTilesAtPrecision(targetPrecision, zTile, maxtiles);
                        values.resizeCell(getNewSize(valuesIndex, numTilesAtPrecision + 1));
                        valuesIndex = setValuesForFullyContainedTile(nextX, nextY, zTile, values, valuesIndex, targetPrecision);
                    }
                } else if (GeoRelation.QUERY_CROSSES == relation) {
                    if (zTile == targetPrecision) {
                        values.resizeCell(getNewSize(valuesIndex, 1));
                        values.add(valuesIndex++, GeoTileUtils.longEncodeTiles(zTile, nextX, nextY));
                    } else {
                        valuesIndex =
                            setValuesByRasterization(nextX, nextY, zTile, values, valuesIndex, targetPrecision, geoValue, maxtiles);
                    }
                }
            }
        }
        return valuesIndex;
    }

    private int getNewSize(int valuesIndex, int increment) {
        long newSize  = (long) valuesIndex + increment;
        if (newSize > Integer.MAX_VALUE) {
            throw new IllegalArgumentException("Tile aggregation array overflow");
        }
        return (int) newSize;
    }

    private int getNumTilesAtPrecision(int finalPrecision, int currentPrecision, long maxtiles) {
        final long numTilesAtPrecision  = Math.min(1L << (2 * (finalPrecision - currentPrecision)), maxtiles);
        if (numTilesAtPrecision > Integer.MAX_VALUE) {
            throw new IllegalArgumentException("Tile aggregation array overflow");
        }
        return (int) numTilesAtPrecision;
    }

    protected long getMaxTilesAtPrecision(int finalPrecision) {
        return Long.MAX_VALUE;
    }

    protected int setValuesForFullyContainedTile(int xTile, int yTile, int zTile, GeoShapeCellValues values, int valuesIndex,
                                                 int targetPrecision) {
        zTile++;
        for (int i = 0; i < 2; i++) {
            for (int j = 0; j < 2; j++) {
                int nextX = 2 * xTile + i;
                int nextY = 2 * yTile + j;
                if (zTile == targetPrecision) {
                    values.add(valuesIndex++, GeoTileUtils.longEncodeTiles(zTile, nextX, nextY));
                } else {
                    valuesIndex = setValuesForFullyContainedTile(nextX, nextY, zTile, values, valuesIndex, targetPrecision);
                }
            }
        }
        return valuesIndex;
    }

    /**
     * Return the number of tiles contained in the provided bounding box at the given zoom level
     */
    protected static long numTilesFromPrecision(int zoom, double minX, double maxX, double minY, double maxY) {
        final long tiles = 1L << zoom;
        final double xDeltaPrecision = 360.0 / (10 * tiles);
        final double yHalfPrecision = 180.0 / (10 * tiles);
        final int minXTile = GeoTileUtils.getXTile(Math.max(-180, minX - xDeltaPrecision), tiles);
        final int minYTile = GeoTileUtils.getYTile(maxY + yHalfPrecision, tiles);
        final int maxXTile = GeoTileUtils.getXTile(Math.min(180, maxX + xDeltaPrecision), tiles);
        final int maxYTile = GeoTileUtils.getYTile(minY - yHalfPrecision, tiles);
        return (long) (maxXTile - minXTile + 1) * (maxYTile - minYTile + 1);
    }
}
