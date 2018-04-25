/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.qa.sql.jdbc;

import com.carrotsearch.randomizedtesting.annotations.ParametersFactory;

import org.elasticsearch.test.junit.annotations.TestLogging;

import java.util.List;

@TestLogging(JdbcTestUtils.SQL_TRACE)
public abstract class DebugSqlSpec extends SqlSpecTestCase {
    @ParametersFactory(shuffle = false, argumentFormatting = PARAM_FORMATTING)
    public static List<Object[]> readScriptSpec() throws Exception {
        Parser parser = specParser();
        return readScriptSpec("/debug.sql-spec", parser);
    }

    public DebugSqlSpec(String fileName, String groupName, String testName, Integer lineNumber, String query) {
        super(fileName, groupName, testName, lineNumber, query);
    }

    @Override
    protected boolean logEsResultSet() {
        return true;
    }
}