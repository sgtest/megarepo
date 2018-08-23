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

import org.apache.lucene.geo.Rectangle;
import org.elasticsearch.test.ESTestCase;

/**
 * Tests for {@link org.elasticsearch.common.geo.GeoHashUtils}
 */
public class GeoHashTests extends ESTestCase {
    public void testGeohashAsLongRoutines() {
        final GeoPoint expected = new GeoPoint();
        final GeoPoint actual = new GeoPoint();
        //Ensure that for all points at all supported levels of precision
        // that the long encoding of a geohash is compatible with its
        // String based counterpart
        for (double lat=-90;lat<90;lat++)
        {
            for (double lng=-180;lng<180;lng++)
            {
                for(int p=1;p<=12;p++)
                {
                    long geoAsLong = GeoHashUtils.longEncode(lng, lat, p);

                    // string encode from geohashlong encoded location
                    String geohashFromLong = GeoHashUtils.stringEncode(geoAsLong);

                    // string encode from full res lat lon
                    String geohash = GeoHashUtils.stringEncode(lng, lat, p);

                    // ensure both strings are the same
                    assertEquals(geohash, geohashFromLong);

                    // decode from the full-res geohash string
                    expected.resetFromGeoHash(geohash);
                    // decode from the geohash encoded long
                    actual.resetFromGeoHash(geoAsLong);

                    assertEquals(expected, actual);
                }
            }
        }
    }

    public void testBboxFromHash() {
        String hash = randomGeohash(1, 12);
        int level = hash.length();
        Rectangle bbox = GeoHashUtils.bbox(hash);
        // check that the length is as expected
        double expectedLonDiff = 360.0 / (Math.pow(8.0, (level + 1) / 2) * Math.pow(4.0, level / 2));
        double expectedLatDiff = 180.0 / (Math.pow(4.0, (level + 1) / 2) * Math.pow(8.0, level / 2));
        assertEquals(expectedLonDiff, bbox.maxLon - bbox.minLon, 0.00001);
        assertEquals(expectedLatDiff, bbox.maxLat - bbox.minLat, 0.00001);
        assertEquals(hash, GeoHashUtils.stringEncode(bbox.minLon, bbox.minLat, level));
    }

    public void testGeohashExtremes() {
        assertEquals("000000000000", GeoHashUtils.stringEncode(-180, -90));
        assertEquals("800000000000", GeoHashUtils.stringEncode(-180, 0));
        assertEquals("bpbpbpbpbpbp", GeoHashUtils.stringEncode(-180, 90));
        assertEquals("h00000000000", GeoHashUtils.stringEncode(0, -90));
        assertEquals("s00000000000", GeoHashUtils.stringEncode(0, 0));
        assertEquals("upbpbpbpbpbp", GeoHashUtils.stringEncode(0, 90));
        assertEquals("pbpbpbpbpbpb", GeoHashUtils.stringEncode(180, -90));
        assertEquals("xbpbpbpbpbpb", GeoHashUtils.stringEncode(180, 0));
        assertEquals("zzzzzzzzzzzz", GeoHashUtils.stringEncode(180, 90));
    }

    public void testLongGeohashes() {
        for (int i = 0; i < 100000; i++) {
            String geohash = randomGeohash(12, 12);
            GeoPoint expected = GeoPoint.fromGeohash(geohash);
            // Adding some random geohash characters at the end
            String extendedGeohash = geohash + randomGeohash(1, 10);
            GeoPoint actual = GeoPoint.fromGeohash(extendedGeohash);
            assertEquals("Additional data points above 12 should be ignored [" + extendedGeohash + "]" , expected, actual);

            Rectangle expectedBbox = GeoHashUtils.bbox(geohash);
            Rectangle actualBbox = GeoHashUtils.bbox(extendedGeohash);
            assertEquals("Additional data points above 12 should be ignored [" + extendedGeohash + "]" , expectedBbox, actualBbox);
        }
    }

    public void testNorthPoleBoundingBox() {
        Rectangle bbox = GeoHashUtils.bbox("zzbxfpgzupbx"); // Bounding box with maximum precision touching north pole
        assertEquals(90.0, bbox.maxLat, 0.0000001); // Should be 90 degrees
    }

    public void testInvalidGeohashes() {
        IllegalArgumentException ex;

        ex = expectThrows(IllegalArgumentException.class, () -> GeoHashUtils.mortonEncode("55.5"));
        assertEquals("unsupported symbol [.] in geohash [55.5]", ex.getMessage());

        ex = expectThrows(IllegalArgumentException.class, () -> GeoHashUtils.mortonEncode(""));
        assertEquals("empty geohash", ex.getMessage());
    }

}
