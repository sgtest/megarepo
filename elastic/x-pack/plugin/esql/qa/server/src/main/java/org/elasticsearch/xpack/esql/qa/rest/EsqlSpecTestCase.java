/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.esql.qa.rest;

import com.carrotsearch.randomizedtesting.annotations.ParametersFactory;

import org.apache.http.HttpEntity;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.ResponseException;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.logging.LogManager;
import org.elasticsearch.logging.Logger;
import org.elasticsearch.test.rest.ESRestTestCase;
import org.elasticsearch.xcontent.XContentType;
import org.elasticsearch.xpack.esql.qa.rest.RestEsqlTestCase.RequestObjectBuilder;
import org.elasticsearch.xpack.ql.CsvSpecReader.CsvTestCase;
import org.elasticsearch.xpack.ql.SpecReader;
import org.junit.After;
import org.junit.AfterClass;
import org.junit.Before;

import java.io.IOException;
import java.net.URL;
import java.util.List;
import java.util.Map;

import static org.elasticsearch.test.MapMatcher.assertMap;
import static org.elasticsearch.test.MapMatcher.matchesMap;
import static org.elasticsearch.xpack.esql.CsvAssert.assertData;
import static org.elasticsearch.xpack.esql.CsvAssert.assertMetadata;
import static org.elasticsearch.xpack.esql.CsvTestUtils.ExpectedResults;
import static org.elasticsearch.xpack.esql.CsvTestUtils.isEnabled;
import static org.elasticsearch.xpack.esql.CsvTestUtils.loadCsvSpecValues;
import static org.elasticsearch.xpack.esql.CsvTestsDataLoader.CSV_DATASET_MAP;
import static org.elasticsearch.xpack.esql.CsvTestsDataLoader.loadDataSetIntoEs;
import static org.elasticsearch.xpack.esql.qa.rest.RestEsqlTestCase.runEsql;
import static org.elasticsearch.xpack.ql.CsvSpecReader.specParser;
import static org.elasticsearch.xpack.ql.TestUtils.classpathResources;

public abstract class EsqlSpecTestCase extends ESRestTestCase {

    private static final Logger LOGGER = LogManager.getLogger(EsqlSpecTestCase.class);
    private final String fileName;
    private final String groupName;
    private final String testName;
    private final Integer lineNumber;
    private final CsvTestCase testCase;

    @ParametersFactory(argumentFormatting = "%2$s.%3$s")
    public static List<Object[]> readScriptSpec() throws Exception {
        List<URL> urls = classpathResources("/*.csv-spec");
        assertTrue("Not enough specs found " + urls, urls.size() > 0);
        return SpecReader.readScriptSpec(urls, specParser());
    }

    public EsqlSpecTestCase(String fileName, String groupName, String testName, Integer lineNumber, CsvTestCase testCase) {
        this.fileName = fileName;
        this.groupName = groupName;
        this.testName = testName;
        this.lineNumber = lineNumber;
        this.testCase = testCase;
    }

    @Before
    public void setup() throws IOException {
        if (indexExists(CSV_DATASET_MAP.keySet().iterator().next()) == false) {
            loadDataSetIntoEs(client());
        }
    }

    @AfterClass
    public static void wipeTestData() throws IOException {
        try {
            adminClient().performRequest(new Request("DELETE", "/*"));
        } catch (ResponseException e) {
            // 404 here just means we had no indexes
            if (e.getResponse().getStatusLine().getStatusCode() != 404) {
                throw e;
            }
        }
    }

    public boolean logResults() {
        return false;
    }

    public final void test() throws Throwable {
        try {
            assumeTrue("Test " + testName + " is not enabled", isEnabled(testName));
            doTest();
        } catch (Exception e) {
            throw reworkException(e);
        }
    }

    protected final void doTest() throws Throwable {
        RequestObjectBuilder builder = new RequestObjectBuilder(randomFrom(XContentType.values()));
        Map<String, Object> answer = runEsql(builder.query(testCase.query).build(), testCase.expectedWarnings);
        var expectedColumnsWithValues = loadCsvSpecValues(testCase.expectedResults);

        var metadata = answer.get("columns");
        assertNotNull(metadata);
        @SuppressWarnings("unchecked")
        var actualColumns = (List<Map<String, String>>) metadata;

        Logger logger = logResults() ? LOGGER : null;
        var values = answer.get("values");
        assertNotNull(values);
        @SuppressWarnings("unchecked")
        List<List<Object>> actualValues = (List<List<Object>>) values;

        assertResults(expectedColumnsWithValues, actualColumns, actualValues, testCase.ignoreOrder, logger);
    }

    protected void assertResults(
        ExpectedResults expected,
        List<Map<String, String>> actualColumns,
        List<List<Object>> actualValues,
        boolean ignoreOrder,
        Logger logger
    ) {
        assertMetadata(expected, actualColumns, logger);
        assertData(expected, actualValues, testCase.ignoreOrder, logger, value -> value == null ? "null" : value.toString());
    }

    private Throwable reworkException(Throwable th) {
        StackTraceElement[] stackTrace = th.getStackTrace();
        StackTraceElement[] redone = new StackTraceElement[stackTrace.length + 1];
        System.arraycopy(stackTrace, 0, redone, 1, stackTrace.length);
        redone[0] = new StackTraceElement(getClass().getName(), groupName + "." + testName, fileName, lineNumber);

        th.setStackTrace(redone);
        return th;
    }

    @Override
    protected boolean preserveClusterUponCompletion() {
        return true;
    }

    @Before
    @After
    public void assertRequestBreakerEmptyAfterTests() throws Exception {
        assertRequestBreakerEmpty();
    }

    public static void assertRequestBreakerEmpty() throws Exception {
        assertBusy(() -> {
            HttpEntity entity = adminClient().performRequest(new Request("GET", "/_nodes/stats")).getEntity();
            Map<?, ?> stats = XContentHelper.convertToMap(XContentType.JSON.xContent(), entity.getContent(), false);
            Map<?, ?> nodes = (Map<?, ?>) stats.get("nodes");
            for (Object n : nodes.values()) {
                Map<?, ?> node = (Map<?, ?>) n;
                Map<?, ?> breakers = (Map<?, ?>) node.get("breakers");
                Map<?, ?> request = (Map<?, ?>) breakers.get("request");
                assertMap(request, matchesMap().extraOk().entry("estimated_size_in_bytes", 0).entry("estimated_size", "0b"));
            }
        });
    }
}
