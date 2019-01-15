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
import java.util.Arrays;
import java.util.Collections;

public class GeometryCollectionTests extends BaseGeometryTestCase<GeometryCollection<Geometry>> {
    @Override
    protected GeometryCollection<Geometry> createTestInstance() {
        return randomGeometryCollection();
    }

    public void testBasicSerialization() throws IOException, ParseException {
        assertEquals("geometrycollection (point (20.0 10.0),point EMPTY)",
            WellKnownText.toWKT(new GeometryCollection<Geometry>(Arrays.asList(new Point(10, 20), Point.EMPTY))));

        assertEquals(new GeometryCollection<Geometry>(Arrays.asList(new Point(10, 20), Point.EMPTY)),
            WellKnownText.fromWKT("geometrycollection (point (20.0 10.0),point EMPTY)"));

        assertEquals("geometrycollection EMPTY", WellKnownText.toWKT(GeometryCollection.EMPTY));
        assertEquals(GeometryCollection.EMPTY, WellKnownText.fromWKT("geometrycollection EMPTY)"));
    }

    @SuppressWarnings("ConstantConditions")
    public void testInitValidation() {
        IllegalArgumentException ex = expectThrows(IllegalArgumentException.class, () -> new GeometryCollection<>(Collections.emptyList()));
        assertEquals("the list of shapes cannot be null or empty", ex.getMessage());

        ex = expectThrows(IllegalArgumentException.class, () -> new GeometryCollection<>(null));
        assertEquals("the list of shapes cannot be null or empty", ex.getMessage());
    }
}
