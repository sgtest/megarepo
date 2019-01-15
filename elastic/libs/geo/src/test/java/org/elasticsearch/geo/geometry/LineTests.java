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

public class LineTests extends BaseGeometryTestCase<Line> {
    @Override
    protected Line createTestInstance() {
        return randomLine();
    }

    public void testBasicSerialization() throws IOException, ParseException {
        assertEquals("linestring (3.0 1.0, 4.0 2.0)", WellKnownText.toWKT(new Line(new double[]{1, 2}, new double[]{3, 4})));
        assertEquals(new Line(new double[]{1, 2}, new double[]{3, 4}), WellKnownText.fromWKT("linestring (3 1, 4 2)"));

        assertEquals("linestring EMPTY", WellKnownText.toWKT(Line.EMPTY));
        assertEquals(Line.EMPTY, WellKnownText.fromWKT("linestring EMPTY)"));
    }

    public void testInitValidation() {
        IllegalArgumentException ex = expectThrows(IllegalArgumentException.class, () -> new Line(new double[]{1}, new double[]{3}));
        assertEquals("at least two points in the line is required", ex.getMessage());

        ex = expectThrows(IllegalArgumentException.class, () -> new Line(new double[]{1, 2, 3, 1}, new double[]{3, 4, 500, 3}));
        assertEquals("invalid longitude 500.0; must be between -180.0 and 180.0", ex.getMessage());

        ex = expectThrows(IllegalArgumentException.class, () -> new Line(new double[]{1, 100, 3, 1}, new double[]{3, 4, 5, 3}));
        assertEquals("invalid latitude 100.0; must be between -90.0 and 90.0", ex.getMessage());
    }
}
