/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.querydsl.query;

import org.elasticsearch.search.sort.NestedSortBuilder;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.sql.SqlIllegalArgumentException;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.tree.LocationTests;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Function;
import java.util.function.Supplier;

import static org.elasticsearch.test.EqualsHashCodeTestUtils.checkEqualsAndHashCode;
import static org.hamcrest.Matchers.hasEntry;
import static java.util.Collections.singletonMap;

public class NestedQueryTests extends ESTestCase {
    static Query randomQuery(int depth) {
        List<Supplier<Query>> options = new ArrayList<>();
        options.add(MatchQueryTests::randomMatchQuery);
        if (depth > 0) {
            options.add(() -> randomNestedQuery(depth - 1));
            options.add(() -> BoolQueryTests.randomBoolQuery(depth - 1));
        }
        return randomFrom(options).get();
    }

    static NestedQuery randomNestedQuery(int depth) {
        return new NestedQuery(LocationTests.randomLocation(), randomAlphaOfLength(5), randomFields(), randomQuery(depth));
    }

    private static Map<String, Boolean> randomFields() {
        int size = between(0, 5);
        Map<String, Boolean> fields = new HashMap<>(size);
        while (fields.size() < size) {
            fields.put(randomAlphaOfLength(5), randomBoolean());
        }
        return fields;
    }

    public void testEqualsAndHashCode() {
        checkEqualsAndHashCode(randomNestedQuery(5), NestedQueryTests::copy, NestedQueryTests::mutate);
    }

    private static NestedQuery copy(NestedQuery query) {
        return new NestedQuery(query.location(), query.path(), query.fields(), query.child());
    }

    private static NestedQuery mutate(NestedQuery query) {
        List<Function<NestedQuery, NestedQuery>> options = Arrays.asList(
            q -> new NestedQuery(LocationTests.mutate(q.location()), q.path(), q.fields(), q.child()),
            q -> new NestedQuery(q.location(), randomValueOtherThan(q.path(), () -> randomAlphaOfLength(5)), q.fields(), q.child()),
            q -> new NestedQuery(q.location(), q.path(), randomValueOtherThan(q.fields(), NestedQueryTests::randomFields), q.child()),
            q -> new NestedQuery(q.location(), q.path(), q.fields(), randomValueOtherThan(q.child(), () -> randomQuery(5))));
        return randomFrom(options).apply(query);
    }

    public void testContainsNestedField() {
        NestedQuery q = randomNestedQuery(0);
        for (String field : q.fields().keySet()) {
            assertTrue(q.containsNestedField(q.path(), field));
            assertFalse(q.containsNestedField(randomValueOtherThan(q.path(), () -> randomAlphaOfLength(5)), field));
        }
        assertFalse(q.containsNestedField(q.path(), randomValueOtherThanMany(q.fields()::containsKey, () -> randomAlphaOfLength(5))));
    }

    public void testAddNestedField() {
        NestedQuery q = randomNestedQuery(0);
        for (String field : q.fields().keySet()) {
            // add does nothing if the field is already there
            assertSame(q, q.addNestedField(q.path(), field, randomBoolean()));
            String otherPath = randomValueOtherThan(q.path(), () -> randomAlphaOfLength(5));
            // add does nothing if the path doesn't match
            assertSame(q, q.addNestedField(otherPath, randomAlphaOfLength(5), randomBoolean()));
        }

        // if the field isn't in the list then add rewrites to a query with all the old fields and the new one
        String newField = randomValueOtherThanMany(q.fields()::containsKey, () -> randomAlphaOfLength(5));
        boolean hasDocValues = randomBoolean();
        NestedQuery added = (NestedQuery) q.addNestedField(q.path(), newField, hasDocValues);
        assertNotSame(q, added);
        assertThat(added.fields(), hasEntry(newField, hasDocValues));
        assertTrue(added.containsNestedField(q.path(), newField));
        for (Map.Entry<String, Boolean> field : q.fields().entrySet()) {
            assertThat(added.fields(), hasEntry(field.getKey(), field.getValue()));
            assertTrue(added.containsNestedField(q.path(), field.getKey()));
        }
    }

    public void testEnrichNestedSort() {
        NestedQuery q = randomNestedQuery(0);

        // enrich adds the filter if the path matches
        {
            NestedSortBuilder sort = new NestedSortBuilder(q.path());
            q.enrichNestedSort(sort);
            assertEquals(q.child().asBuilder(), sort.getFilter());
        }

        // but doesn't if it doesn't match
        {
            NestedSortBuilder sort = new NestedSortBuilder(randomValueOtherThan(q.path(), () -> randomAlphaOfLength(5)));
            q.enrichNestedSort(sort);
            assertNull(sort.getFilter());
        }

        // enriching with the same query twice is fine
        {
            NestedSortBuilder sort = new NestedSortBuilder(q.path());
            q.enrichNestedSort(sort);
            assertEquals(q.child().asBuilder(), sort.getFilter());
            q.enrichNestedSort(sort);

            // But enriching using another query is not
            NestedQuery other = new NestedQuery(LocationTests.randomLocation(), q.path(), q.fields(),
                randomValueOtherThan(q.child(), () -> randomQuery(0)));
            Exception e = expectThrows(SqlIllegalArgumentException.class, () -> other.enrichNestedSort(sort));
            assertEquals("nested query should have been grouped in one place", e.getMessage());
        }
    }

    public void testToString() {
        NestedQuery q = new NestedQuery(new Location(1, 1), "a.b", singletonMap("f", true), new MatchAll(new Location(1, 1)));
        assertEquals("NestedQuery@1:2[a.b.{f=true}[MatchAll@1:2[]]]", q.toString());
    }
}
