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

package org.elasticsearch.geo.geometry;

/**
 * Represents a lat/lon rectangle in decimal degrees.
 */
public class Rectangle implements Geometry {
    public static final Rectangle EMPTY = new Rectangle();
    /**
     * maximum longitude value (in degrees)
     */
    private final double minLat;
    /**
     * minimum longitude value (in degrees)
     */
    private final double minLon;
    /**
     * maximum latitude value (in degrees)
     */
    private final double maxLat;
    /**
     * minimum latitude value (in degrees)
     */
    private final double maxLon;

    private final boolean empty;

    private Rectangle() {
        minLat = 0;
        minLon = 0;
        maxLat = 0;
        maxLon = 0;
        empty = true;
    }

    /**
     * Constructs a bounding box by first validating the provided latitude and longitude coordinates
     */
    public Rectangle(double minLat, double maxLat, double minLon, double maxLon) {
        GeometryUtils.checkLatitude(minLat);
        GeometryUtils.checkLatitude(maxLat);
        GeometryUtils.checkLongitude(minLon);
        GeometryUtils.checkLongitude(maxLon);
        this.minLon = minLon;
        this.maxLon = maxLon;
        this.minLat = minLat;
        this.maxLat = maxLat;
        empty = false;
        if (maxLat < minLat) {
            throw new IllegalArgumentException("max lat cannot be less than min lat");
        }
    }

    public double getWidth() {
        if (crossesDateline()) {
            return GeometryUtils.MAX_LON_INCL - minLon + maxLon - GeometryUtils.MIN_LON_INCL;
        }
        return maxLon - minLon;
    }

    public double getHeight() {
        return maxLat - minLat;
    }

    public double getMinLat() {
        return minLat;
    }

    public double getMinLon() {
        return minLon;
    }

    public double getMaxLat() {
        return maxLat;
    }

    public double getMaxLon() {
        return maxLon;
    }

    @Override
    public ShapeType type() {
        return ShapeType.ENVELOPE;
    }

    @Override
    public String toString() {
        StringBuilder b = new StringBuilder();
        b.append("Rectangle(lat=");
        b.append(minLat);
        b.append(" TO ");
        b.append(maxLat);
        b.append(" lon=");
        b.append(minLon);
        b.append(" TO ");
        b.append(maxLon);
        if (maxLon < minLon) {
            b.append(" [crosses dateline!]");
        }
        b.append(")");

        return b.toString();
    }

    /**
     * Returns true if this bounding box crosses the dateline
     */
    public boolean crossesDateline() {
        return maxLon < minLon;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;

        Rectangle rectangle = (Rectangle) o;

        if (Double.compare(rectangle.minLat, minLat) != 0) return false;
        if (Double.compare(rectangle.minLon, minLon) != 0) return false;
        if (Double.compare(rectangle.maxLat, maxLat) != 0) return false;
        return Double.compare(rectangle.maxLon, maxLon) == 0;

    }

    @Override
    public int hashCode() {
        int result;
        long temp;
        temp = Double.doubleToLongBits(minLat);
        result = (int) (temp ^ (temp >>> 32));
        temp = Double.doubleToLongBits(minLon);
        result = 31 * result + (int) (temp ^ (temp >>> 32));
        temp = Double.doubleToLongBits(maxLat);
        result = 31 * result + (int) (temp ^ (temp >>> 32));
        temp = Double.doubleToLongBits(maxLon);
        result = 31 * result + (int) (temp ^ (temp >>> 32));
        return result;
    }

    @Override
    public <T> T visit(GeometryVisitor<T> visitor) {
        return visitor.visit(this);
    }

    @Override
    public boolean isEmpty() {
        return empty;
    }
}
