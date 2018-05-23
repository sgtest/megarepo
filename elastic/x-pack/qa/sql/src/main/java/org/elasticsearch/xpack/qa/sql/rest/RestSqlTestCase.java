/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.qa.sql.rest;

import com.fasterxml.jackson.core.io.JsonStringEncoder;
import org.apache.http.HttpEntity;
import org.apache.http.entity.ContentType;
import org.apache.http.entity.StringEntity;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.Response;
import org.elasticsearch.client.ResponseException;
import org.elasticsearch.common.CheckedSupplier;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.io.Streams;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.test.NotEqualMessageBuilder;
import org.elasticsearch.test.rest.ESRestTestCase;
import org.elasticsearch.xpack.qa.sql.ErrorsTestCase;
import org.hamcrest.Matcher;

import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.sql.JDBCType;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;

import static java.util.Collections.emptyList;
import static java.util.Collections.singletonList;
import static java.util.Collections.singletonMap;
import static java.util.Collections.unmodifiableMap;
import static org.hamcrest.Matchers.containsString;

/**
 * Integration test for the rest sql action. The one that speaks json directly to a
 * user rather than to the JDBC driver or CLI.
 */
public abstract class RestSqlTestCase extends ESRestTestCase implements ErrorsTestCase {
    /**
     * Builds that map that is returned in the header for each column.
     */
    public static Map<String, Object> columnInfo(String mode, String name, String type, JDBCType jdbcType, int size) {
        Map<String, Object> column = new HashMap<>();
        column.put("name", name);
        column.put("type", type);
        if ("jdbc".equals(mode)) {
            column.put("jdbc_type", jdbcType.getVendorTypeNumber());
            column.put("display_size", size);
        }
        return unmodifiableMap(column);
    }

    public void testBasicQuery() throws IOException {
        index("{\"test\":\"test\"}",
            "{\"test\":\"test\"}");

        Map<String, Object> expected = new HashMap<>();
        String mode = randomMode();
        expected.put("columns", singletonList(columnInfo(mode, "test", "text", JDBCType.VARCHAR, 0)));
        expected.put("rows", Arrays.asList(singletonList("test"), singletonList("test")));
        assertResponse(expected, runSql(mode, "SELECT * FROM test"));
    }

    public void testNextPage() throws IOException {
        Request request = new Request("POST", "/test/test/_bulk");
        request.addParameter("refresh", "true");
        String mode = randomMode();
        StringBuilder bulk = new StringBuilder();
        for (int i = 0; i < 20; i++) {
            bulk.append("{\"index\":{\"_id\":\"" + i + "\"}}\n");
            bulk.append("{\"text\":\"text" + i + "\", \"number\":" + i + "}\n");
        }
        request.setJsonEntity(bulk.toString());
        client().performRequest(request);

        String sqlRequest =
                  "{\"query\":\""
                + "   SELECT text, number, SQRT(number) AS s, SCORE()"
                + "     FROM test"
                + " ORDER BY number, SCORE()\", "
                + "\"mode\":\"" + mode + "\", "
            + "\"fetch_size\":2}";

        String cursor = null;
        for (int i = 0; i < 20; i += 2) {
            Map<String, Object> response;
            if (i == 0) {
                response = runSql(mode, new StringEntity(sqlRequest, ContentType.APPLICATION_JSON));
            } else {
                response = runSql(mode, new StringEntity("{\"cursor\":\"" + cursor + "\"}",
                        ContentType.APPLICATION_JSON));
            }

            Map<String, Object> expected = new HashMap<>();
            if (i == 0) {
                expected.put("columns", Arrays.asList(
                        columnInfo(mode, "text", "text", JDBCType.VARCHAR, 0),
                        columnInfo(mode, "number", "long", JDBCType.BIGINT, 20),
                        columnInfo(mode, "s", "double", JDBCType.DOUBLE, 25),
                        columnInfo(mode, "SCORE()", "float", JDBCType.REAL, 15)));
            }
            expected.put("rows", Arrays.asList(
                    Arrays.asList("text" + i, i, Math.sqrt(i), 1.0),
                    Arrays.asList("text" + (i + 1), i + 1, Math.sqrt(i + 1), 1.0)));
            cursor = (String) response.remove("cursor");
            assertResponse(expected, response);
            assertNotNull(cursor);
        }
        Map<String, Object> expected = new HashMap<>();
        expected.put("rows", emptyList());
        assertResponse(expected, runSql(mode, new StringEntity("{ \"cursor\":\"" + cursor + "\"}",
                ContentType.APPLICATION_JSON)));
    }

    @AwaitsFix(bugUrl = "Unclear status, https://github.com/elastic/x-pack-elasticsearch/issues/2074")
    public void testTimeZone() throws IOException {
        String mode = randomMode();
        index("{\"test\":\"2017-07-27 00:00:00\"}",
            "{\"test\":\"2017-07-27 01:00:00\"}");

        Map<String, Object> expected = new HashMap<>();
        expected.put("columns", singletonMap("test", singletonMap("type", "text")));
        expected.put("rows", Arrays.asList(singletonMap("test", "test"), singletonMap("test", "test")));
        expected.put("size", 2);

        // Default TimeZone is UTC
        assertResponse(expected, runSql(mode, new StringEntity("{\"query\":\"SELECT DAY_OF_YEAR(test), COUNT(*) FROM test\"}",
                        ContentType.APPLICATION_JSON)));
    }

    public void testScoreWithFieldNamedScore() throws IOException {
        Request request = new Request("POST", "/test/test/_bulk");
        request.addParameter("refresh", "true");
        String mode = randomMode();
        StringBuilder bulk = new StringBuilder();
        bulk.append("{\"index\":{\"_id\":\"1\"}}\n");
        bulk.append("{\"name\":\"test\", \"score\":10}\n");
        request.setJsonEntity(bulk.toString());
        client().performRequest(request);

        Map<String, Object> expected = new HashMap<>();
        expected.put("columns", Arrays.asList(
            columnInfo(mode, "name", "text", JDBCType.VARCHAR, 0),
            columnInfo(mode, "score", "long", JDBCType.BIGINT, 20),
            columnInfo(mode, "SCORE()", "float", JDBCType.REAL, 15)));
        expected.put("rows", singletonList(Arrays.asList(
            "test", 10, 1.0)));

        assertResponse(expected, runSql(mode, "SELECT *, SCORE() FROM test ORDER BY SCORE()"));
        assertResponse(expected, runSql(mode, "SELECT name, \\\"score\\\", SCORE() FROM test ORDER BY SCORE()"));
    }

    public void testSelectWithJoinFails() throws Exception {
        // Normal join not supported
        expectBadRequest(() -> runSql(randomMode(), "SELECT * FROM test JOIN other"),
            containsString("line 1:21: Queries with JOIN are not yet supported"));
        // Neither is a self join
        expectBadRequest(() -> runSql(randomMode(), "SELECT * FROM test JOIN test"),
            containsString("line 1:21: Queries with JOIN are not yet supported"));
        // Nor fancy stuff like CTEs
        expectBadRequest(() -> runSql(randomMode(),
            "    WITH evil"
            + "  AS (SELECT *"
            + "        FROM foo)"
            + "SELECT *"
            + "  FROM test"
            + "  JOIN evil"),
            containsString("line 1:67: Queries with JOIN are not yet supported"));
    }

    public void testSelectDistinctFails() throws Exception {
        index("{\"name\":\"test\"}");
        expectBadRequest(() -> runSql(randomMode(), "SELECT DISTINCT name FROM test"),
            containsString("line 1:8: SELECT DISTINCT is not yet supported"));
    }

    public void testSelectGroupByAllFails() throws Exception {
        index("{\"foo\":1}", "{\"foo\":2}");
        expectBadRequest(() -> runSql(randomMode(), "SELECT foo FROM test GROUP BY ALL foo"),
            containsString("line 1:32: GROUP BY ALL is not supported"));
    }

    public void testSelectWhereExistsFails() throws Exception {
        index("{\"foo\":1}", "{\"foo\":2}");
        expectBadRequest(() -> runSql(randomMode(), "SELECT foo FROM test WHERE EXISTS (SELECT * FROM test t WHERE t.foo = test.foo)"),
            containsString("line 1:28: EXISTS is not yet supported"));
    }


    @Override
    public void testSelectInvalidSql() {
        String mode = randomFrom("jdbc", "plain");
        expectBadRequest(() -> runSql(mode, "SELECT * FRO"), containsString("1:8: Cannot determine columns for *"));
    }

    @Override
    public void testSelectFromMissingIndex() {
        String mode = randomFrom("jdbc", "plain");
        expectBadRequest(() -> runSql(mode, "SELECT * FROM missing"), containsString("1:15: Unknown index [missing]"));
    }

    @Override
    public void testSelectFromIndexWithoutTypes() throws Exception {
        // Create an index without any types
        Request request = new Request("PUT", "/test");
        request.setJsonEntity("{}");
        client().performRequest(request);
        String mode = randomFrom("jdbc", "plain");
        expectBadRequest(() -> runSql(mode, "SELECT * FROM test"),
            containsString("1:15: [test] doesn't have any types so it is incompatible with sql"));
    }

    @Override
    public void testSelectMissingField() throws IOException {
        index("{\"test\":\"test\"}");
        String mode = randomFrom("jdbc", "plain");
        expectBadRequest(() -> runSql(mode, "SELECT foo FROM test"), containsString("1:8: Unknown column [foo]"));
    }

    @Override
    public void testSelectMissingFunction() throws Exception {
        index("{\"foo\":1}");
        expectBadRequest(() -> runSql(randomMode(), "SELECT missing(foo) FROM test"),
                containsString("1:8: Unknown function [missing]"));
    }

    @Override
    public void testSelectProjectScoreInAggContext() throws Exception {
        index("{\"foo\":1}");
        expectBadRequest(() -> runSql(randomMode(),
            "     SELECT foo, SCORE(), COUNT(*)"
            + "     FROM test"
            + " GROUP BY foo"),
                containsString("Cannot use non-grouped column [SCORE()], expected [foo]"));
    }

    @Override
    public void testSelectOrderByScoreInAggContext() throws Exception {
        index("{\"foo\":1}");
        expectBadRequest(() -> runSql(randomMode(),
            "     SELECT foo, COUNT(*)"
            + "     FROM test"
            + " GROUP BY foo"
            + " ORDER BY SCORE()"),
                containsString("Cannot order by non-grouped column [SCORE()], expected [foo]"));
    }

    @Override
    public void testSelectGroupByScore() throws Exception {
        index("{\"foo\":1}");
        expectBadRequest(() -> runSql(randomMode(), "SELECT COUNT(*) FROM test GROUP BY SCORE()"),
                containsString("Cannot use [SCORE()] for grouping"));
    }

    @Override
    public void testSelectScoreSubField() throws Exception {
        index("{\"foo\":1}");
        expectBadRequest(() -> runSql(randomMode(), "SELECT SCORE().bar FROM test"),
            containsString("line 1:15: extraneous input '.' expecting {<EOF>, ','"));
    }

    @Override
    public void testSelectScoreInScalar() throws Exception {
        index("{\"foo\":1}");
        expectBadRequest(() -> runSql(randomMode(), "SELECT SIN(SCORE()) FROM test"),
            containsString("line 1:12: [SCORE()] cannot be an argument to a function"));
    }

    private void expectBadRequest(CheckedSupplier<Map<String, Object>, Exception> code, Matcher<String> errorMessageMatcher) {
        try {
            Map<String, Object> result = code.get();
            fail("expected ResponseException but got " + result);
        } catch (ResponseException e) {
            if (400 != e.getResponse().getStatusLine().getStatusCode()) {
                String body;
                try {
                    body = Streams.copyToString(new InputStreamReader(
                        e.getResponse().getEntity().getContent(), StandardCharsets.UTF_8));
                } catch (IOException bre) {
                    throw new RuntimeException("error reading body after remote sent bad status", bre);
                }
                fail("expected [400] response but get [" + e.getResponse().getStatusLine().getStatusCode() + "] with body:\n" +  body);
            }
            assertThat(e.getMessage(), errorMessageMatcher);
        } catch (Exception e) {
            throw new AssertionError("expected ResponseException but got [" + e.getClass() + "]", e);
        }
    }

    private Map<String, Object> runSql(String mode, String sql) throws IOException {
        return runSql(mode, sql, "");
    }

    private Map<String, Object> runSql(String mode, String sql, String suffix) throws IOException {
        return runSql(mode, new StringEntity("{\"query\":\"" + sql + "\"}", ContentType.APPLICATION_JSON), suffix);
    }

    private Map<String, Object> runSql(String mode, HttpEntity sql) throws IOException {
        return runSql(mode, sql, "");
    }

    private Map<String, Object> runSql(String mode, HttpEntity sql, String suffix) throws IOException {
        Request request = new Request("POST", "/_xpack/sql" + suffix);
        request.addParameter("error_trace", "true");   // Helps with debugging in case something crazy happens on the server.
        request.addParameter("pretty", "true");        // Improves error reporting readability
        if (randomBoolean()) {
            // We default to JSON but we force it randomly for extra coverage
            request.addParameter("format", "json");
        }
        if (false == mode.isEmpty()) {
            request.addParameter("mode", mode);        // JDBC or PLAIN mode
        }
        if (randomBoolean()) {
            request.addHeader("Accept", randomFrom("*/*", "application/json"));
        }
        request.setEntity(sql);
        Response response = client().performRequest(request);
        try (InputStream content = response.getEntity().getContent()) {
            return XContentHelper.convertToMap(JsonXContent.jsonXContent, content, false);
        }
    }

    public void testBasicTranslateQuery() throws IOException {
        index("{\"test\":\"test\"}", "{\"test\":\"test\"}");

        Map<String, Object> response = runSql(randomMode(), "SELECT * FROM test", "/translate/");
        assertEquals(1000, response.get("size"));
        @SuppressWarnings("unchecked")
        Map<String, Object> source = (Map<String, Object>) response.get("_source");
        assertNotNull(source);
        assertEquals(emptyList(), source.get("excludes"));
        assertEquals(singletonList("test"), source.get("includes"));
    }

    public void testBasicQueryWithFilter() throws IOException {
        String mode = randomMode();
        index("{\"test\":\"foo\"}",
            "{\"test\":\"bar\"}");

        Map<String, Object> expected = new HashMap<>();
        expected.put("columns", singletonList(columnInfo(mode, "test", "text", JDBCType.VARCHAR, 0)));
        expected.put("rows", singletonList(singletonList("foo")));
        assertResponse(expected, runSql(mode, new StringEntity("{\"query\":\"SELECT * FROM test\", " +
                                "\"filter\":{\"match\": {\"test\": \"foo\"}}}",
                ContentType.APPLICATION_JSON)));
    }

    public void testBasicQueryWithParameters() throws IOException {
        String mode = randomMode();
        index("{\"test\":\"foo\"}",
                "{\"test\":\"bar\"}");

        Map<String, Object> expected = new HashMap<>();
        expected.put("columns", Arrays.asList(
                columnInfo(mode, "test", "text", JDBCType.VARCHAR, 0),
                columnInfo(mode, "param", "integer", JDBCType.INTEGER, 11)
        ));
        expected.put("rows", singletonList(Arrays.asList("foo", 10)));
        assertResponse(expected, runSql(mode, new StringEntity("{\"query\":\"SELECT test, ? param FROM test WHERE test = ?\", " +
                "\"params\":[{\"type\": \"integer\", \"value\": 10}, {\"type\": \"keyword\", \"value\": \"foo\"}]}",
                ContentType.APPLICATION_JSON)));
    }

    public void testBasicTranslateQueryWithFilter() throws IOException {
        index("{\"test\":\"foo\"}",
            "{\"test\":\"bar\"}");

        Map<String, Object> response = runSql("",
                new StringEntity("{\"query\":\"SELECT * FROM test\", \"filter\":{\"match\": {\"test\": \"foo\"}}}",
                ContentType.APPLICATION_JSON), "/translate/"
        );

        assertEquals(response.get("size"), 1000);
        @SuppressWarnings("unchecked")
        Map<String, Object> source = (Map<String, Object>) response.get("_source");
        assertNotNull(source);
        assertEquals(emptyList(), source.get("excludes"));
        assertEquals(singletonList("test"), source.get("includes"));

        @SuppressWarnings("unchecked")
        Map<String, Object> query = (Map<String, Object>) response.get("query");
        assertNotNull(query);

        @SuppressWarnings("unchecked")
        Map<String, Object> bool = (Map<String, Object>) query.get("bool");
        assertNotNull(bool);

        @SuppressWarnings("unchecked")
        List<Object> filter = (List<Object>) bool.get("filter");
        assertNotNull(filter);

        @SuppressWarnings("unchecked")
        Map<String, Object> map = (Map<String, Object>) filter.get(0);
        assertNotNull(map);

        @SuppressWarnings("unchecked")
        Map<String, Object> matchQ = (Map<String, Object>) map.get("match");

        @SuppressWarnings("unchecked")
        Map<String, Object> matchQuery = (Map<String, Object>) matchQ.get("test");

        assertNotNull(matchQuery);
        assertEquals("foo", matchQuery.get("query"));
    }

    public void testBasicQueryText() throws IOException {
        index("{\"test\":\"test\"}",
            "{\"test\":\"test\"}");

        String expected =
                "     test      \n" +
                "---------------\n" +
                "test           \n" +
                "test           \n";
        Tuple<String, String> response = runSqlAsText("SELECT * FROM test", "text/plain");
        assertEquals(expected, response.v1());
    }

    public void testNextPageText() throws IOException {
        int size = 20;
        String[] docs = new String[size];
        for (int i = 0; i < size; i++) {
            docs[i] = "{\"text\":\"text" + i + "\", \"number\":" + i + "}\n";
        }
        index(docs);

        String request = "{\"query\":\"SELECT text, number, number + 5 AS sum FROM test ORDER BY number\", \"fetch_size\":2}";

        String cursor = null;
        for (int i = 0; i < 20; i += 2) {
            Tuple<String, String> response;
            if (i == 0) {
                response = runSqlAsText("", new StringEntity(request, ContentType.APPLICATION_JSON), "text/plain");
            } else {
                response = runSqlAsText("", new StringEntity("{\"cursor\":\"" + cursor + "\"}", ContentType.APPLICATION_JSON),
                        "text/plain");
            }

            StringBuilder expected = new StringBuilder();
            if (i == 0) {
                expected.append("     text      |    number     |      sum      \n");
                expected.append("---------------+---------------+---------------\n");
            }
            expected.append(String.format(Locale.ROOT, "%-15s|%-15d|%-15d\n", "text" + i, i, i + 5));
            expected.append(String.format(Locale.ROOT, "%-15s|%-15d|%-15d\n", "text" + (i + 1), i + 1, i + 6));
            cursor = response.v2();
            assertEquals(expected.toString(), response.v1());
            assertNotNull(cursor);
        }
        Map<String, Object> expected = new HashMap<>();
        expected.put("rows", emptyList());
        assertResponse(expected, runSql("", new StringEntity("{\"cursor\":\"" + cursor + "\"}", ContentType.APPLICATION_JSON)));

        Map<String, Object> response = runSql("", new StringEntity("{\"cursor\":\"" + cursor + "\"}", ContentType.APPLICATION_JSON),
                "/close");
        assertEquals(true, response.get("succeeded"));

        assertEquals(0, getNumberOfSearchContexts("test"));
    }

    // CSV/TSV tests

    private static String toJson(String value) {
        return "\"" + new String(JsonStringEncoder.getInstance().quoteAsString(value)) + "\"";
    }

    public void testDefaultQueryInCSV() throws IOException {
        index("{\"name\":" + toJson("first") + ", \"number\" : 1 }",
              "{\"name\":" + toJson("second\t") + ", \"number\": 2 }",
              "{\"name\":" + toJson("\"third,\"") + ", \"number\": 3 }");

        String expected =
                "name,number\r\n" +
                "first,1\r\n" +
                "second\t,2\r\n" +
                "\"\"\"third,\"\"\",3\r\n";

        String query = "SELECT * FROM test ORDER BY number";
        Tuple<String, String> response = runSqlAsText(query, "text/csv");
        assertEquals(expected, response.v1());

        response = runSqlAsTextFormat(query, "csv");
        assertEquals(expected, response.v1());
    }

    public void testQueryWithoutHeaderInCSV() throws IOException {
        index("{\"name\":" + toJson("first") + ", \"number\" : 1 }",
              "{\"name\":" + toJson("second\t") + ", \"number\": 2 }",
              "{\"name\":" + toJson("\"third,\"") + ", \"number\": 3 }");

        String expected =
                "first,1\r\n" +
                "second\t,2\r\n" +
                "\"\"\"third,\"\"\",3\r\n";

        String query = "SELECT * FROM test ORDER BY number";
        Tuple<String, String> response = runSqlAsText(query, "text/csv; header=absent");
        assertEquals(expected, response.v1());
    }

    public void testQueryInTSV() throws IOException {
        index("{\"name\":" + toJson("first") + ", \"number\" : 1 }",
              "{\"name\":" + toJson("second\t") + ", \"number\": 2 }",
              "{\"name\":" + toJson("\"third,\"") + ", \"number\": 3 }");

        String expected =
                "name\tnumber\n" +
                "first\t1\n" +
                "second\\t\t2\n" +
                "\"third,\"\t3\n";

        String query = "SELECT * FROM test ORDER BY number";
        Tuple<String, String> response = runSqlAsText(query, "text/tab-separated-values");
        assertEquals(expected, response.v1());
        response = runSqlAsTextFormat(query, "tsv");
        assertEquals(expected, response.v1());
    }

    private Tuple<String, String> runSqlAsText(String sql, String accept) throws IOException {
        return runSqlAsText("", new StringEntity("{\"query\":\"" + sql + "\"}", ContentType.APPLICATION_JSON), accept);
    }

    /**
     * Run SQL as text using the {@code Accept} header to specify the format
     * rather than the {@code format} parameter.
     */
    private Tuple<String, String> runSqlAsText(String suffix, HttpEntity entity, String accept) throws IOException {
        Request request = new Request("POST", "/_xpack/sql" + suffix);
        request.addParameter("error_trace", "true");
        request.setEntity(entity);
        request.addHeader("Accept", accept);
        Response response = client().performRequest(request);
        return new Tuple<>(
                Streams.copyToString(new InputStreamReader(response.getEntity().getContent(), StandardCharsets.UTF_8)),
                response.getHeader("Cursor")
        );
    }

    /**
     * Run SQL as text using the {@code format} parameter to specify the format
     * rather than an {@code Accept} header.
     */
    private Tuple<String, String> runSqlAsTextFormat(String sql, String format) throws IOException {
        Request request = new Request("POST", "/_xpack/sql");
        request.addParameter("error_trace", "true");
        request.addParameter("format", format);
        request.setJsonEntity("{\"query\":\"" + sql + "\"}");

        Response response = client().performRequest(request);
        return new Tuple<>(
                Streams.copyToString(new InputStreamReader(response.getEntity().getContent(), StandardCharsets.UTF_8)),
                response.getHeader("Cursor")
        );
    }

    private void assertResponse(Map<String, Object> expected, Map<String, Object> actual) {
        if (false == expected.equals(actual)) {
            NotEqualMessageBuilder message = new NotEqualMessageBuilder();
            message.compareMaps(actual, expected);
            fail("Response does not match:\n" + message.toString());
        }
    }

    public static int getNumberOfSearchContexts(String index) throws IOException {
        return getOpenContexts(searchStats(), index);
    }

    public static void assertNoSearchContexts() throws IOException {
        Map<String, Object> stats = searchStats();
        @SuppressWarnings("unchecked")
        Map<String, Object> indicesStats = (Map<String, Object>) stats.get("indices");
        for (String index : indicesStats.keySet()) {
            if (index.startsWith(".") == false) { // We are not interested in internal indices
                assertEquals(index + " should have no search contexts", 0, getOpenContexts(stats, index));
            }
        }
    }

    @SuppressWarnings("unchecked")
    private static int getOpenContexts(Map<String, Object> stats, String index) {
        stats = (Map<String, Object>) stats.get("indices");
        stats = (Map<String, Object>) stats.get(index);
        stats = (Map<String, Object>) stats.get("total");
        stats = (Map<String, Object>) stats.get("search");
        return (Integer) stats.get("open_contexts");
    }

    private static Map<String, Object> searchStats() throws IOException {
        Response response = client().performRequest(new Request("GET", "/_stats/search"));
        try (InputStream content = response.getEntity().getContent()) {
            return XContentHelper.convertToMap(JsonXContent.jsonXContent, content, false);
        }
    }

    public static String randomMode() {
        return randomFrom("", "jdbc", "plain");
    }

    private void index(String... docs) throws IOException {
        Request request = new Request("POST", "/test/test/_bulk");
        request.addParameter("refresh", "true");
        StringBuilder bulk = new StringBuilder();
        for (String doc : docs) {
            bulk.append("{\"index\":{}\n");
            bulk.append(doc + "\n");
        }
        request.setJsonEntity(bulk.toString());
        client().performRequest(request);
    }
}
