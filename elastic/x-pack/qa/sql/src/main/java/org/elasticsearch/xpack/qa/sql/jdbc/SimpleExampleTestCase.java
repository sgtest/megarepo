/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.qa.sql.jdbc;

import java.sql.Connection;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Statement;

public class SimpleExampleTestCase extends JdbcIntegrationTestCase {
    public void testSimpleExample() throws Exception {
        index("library", builder -> {
            builder.field("name", "Don Quixote");
            builder.field("page_count", 1072);
        });
        try (Connection connection = esJdbc()) {
            // tag::simple_example
            try (Statement statement = connection.createStatement();
                    ResultSet results = statement.executeQuery(
                        "SELECT name, page_count FROM library ORDER BY page_count DESC LIMIT 1")) {
                assertTrue(results.next());
                assertEquals("Don Quixote", results.getString(1));
                assertEquals(1072, results.getInt(2));
                SQLException e = expectThrows(SQLException.class, () -> results.getInt(1));
                assertTrue(e.getMessage(), e.getMessage().contains("unable to convert column 1 to an int"));
                assertFalse(results.next());
            }
            // end::simple_example
        }
    }
}
