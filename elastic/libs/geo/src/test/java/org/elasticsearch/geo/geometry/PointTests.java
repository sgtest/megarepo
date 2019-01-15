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

import org.elasticsearch.geo.utils.WellKnownText;

import java.io.IOException;
import java.text.ParseException;

public class PointTests extends BaseGeometryTestCase<Point> {
    @Override
    protected Point createTestInstance() {
        return randomPoint();
    }

    public void testBasicSerialization() throws IOException, ParseException {
        assertEquals("point (20.0 10.0)", WellKnownText.toWKT(new Point(10, 20)));
        assertEquals(new Point(10, 20), WellKnownText.fromWKT("point (20.0 10.0)"));

        assertEquals("point EMPTY", WellKnownText.toWKT(Point.EMPTY));
        assertEquals(Point.EMPTY, WellKnownText.fromWKT("point EMPTY)"));
    }

    public void testInitValidation() {
        IllegalArgumentException ex = expectThrows(IllegalArgumentException.class, () -> new Point(100, 10));
        assertEquals("invalid latitude 100.0; must be between -90.0 and 90.0", ex.getMessage());

        ex = expectThrows(IllegalArgumentException.class, () -> new Point(10, 500));
        assertEquals("invalid longitude 500.0; must be between -180.0 and 180.0", ex.getMessage());
    }
}
