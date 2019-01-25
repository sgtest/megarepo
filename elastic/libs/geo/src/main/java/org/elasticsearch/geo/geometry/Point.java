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
 * Represents a Point on the earth's surface in decimal degrees.
 */
public class Point implements Geometry {
    public static final Point EMPTY = new Point();

    private final double lat;
    private final double lon;
    private final boolean empty;

    private Point() {
        lat = 0;
        lon = 0;
        empty = true;
    }

    public Point(double lat, double lon) {
        GeometryUtils.checkLatitude(lat);
        GeometryUtils.checkLongitude(lon);
        this.lat = lat;
        this.lon = lon;
        this.empty = false;
    }

    @Override
    public ShapeType type() {
        return ShapeType.POINT;
    }

    public double getLat() {
        return lat;
    }

    public double getLon() {
        return lon;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;

        Point point = (Point) o;
        if (point.empty != empty) return false;
        if (Double.compare(point.lat, lat) != 0) return false;
        return Double.compare(point.lon, lon) == 0;
    }

    @Override
    public int hashCode() {
        int result;
        long temp;
        temp = Double.doubleToLongBits(lat);
        result = (int) (temp ^ (temp >>> 32));
        temp = Double.doubleToLongBits(lon);
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

    @Override
    public String toString() {
        return "lat=" + lat + ", lon=" + lon;
    }
}
