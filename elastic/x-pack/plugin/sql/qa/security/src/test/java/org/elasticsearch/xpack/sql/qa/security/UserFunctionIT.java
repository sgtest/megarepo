/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.sql.qa.security;

import org.apache.http.HttpEntity;
import org.apache.http.entity.ContentType;
import org.apache.http.entity.StringEntity;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.RequestOptions;
import org.elasticsearch.client.Response;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.test.NotEqualMessageBuilder;
import org.elasticsearch.test.rest.ESRestTestCase;
import org.junit.After;
import org.junit.Before;
import org.junit.Rule;
import org.junit.rules.TestName;

import java.io.IOException;
import java.io.InputStream;
import java.sql.JDBCType;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.elasticsearch.xpack.sql.qa.rest.RestSqlTestCase.columnInfo;
import static org.elasticsearch.xpack.sql.qa.rest.RestSqlTestCase.mode;
import static org.elasticsearch.xpack.sql.qa.rest.RestSqlTestCase.randomMode;
import static org.elasticsearch.xpack.sql.qa.rest.RestSqlTestCase.SQL_QUERY_REST_ENDPOINT;

public class UserFunctionIT extends ESRestTestCase {

    private static final String SQL = "SELECT USER()";
    // role defined in roles.yml
    private static final String MINIMAL_ACCESS_ROLE = "rest_minimal";
    private List<String> users;
    @Rule
    public TestName name = new TestName();
    
    @Override
    protected Settings restClientSettings() {
        return RestSqlIT.securitySettings();
    }
    
    @Override
    protected String getProtocol() {
        return RestSqlIT.SSL_ENABLED ? "https" : "http";
    }
    
    @Before
    private void setUpUsers() throws IOException {
        int usersCount = name.getMethodName().startsWith("testSingle") ? 1 : randomIntBetween(5,  15);
        users = new ArrayList<>(usersCount);
        users.addAll(randomUnique(() -> randomAlphaOfLengthBetween(1, 15), usersCount));
        for (String user : users) {
            createUser(user, MINIMAL_ACCESS_ROLE);
        }
    }

    @After
    private void clearUsers() throws IOException {
        for (String user : users) {
            deleteUser(user);
        }
    }
    
    public void testSingleRandomUser() throws IOException {
        String mode = randomMode().toString();
        String randomUserName = users.get(0);
        
        Map<String, Object> expected = new HashMap<>();
        expected.put("columns", Arrays.asList(
                columnInfo(mode, "USER()", "keyword", JDBCType.VARCHAR, 0)));
        expected.put("rows", Arrays.asList(Arrays.asList(randomUserName)));
        Map<String, Object> actual = runSql(randomUserName, mode, SQL);
        
        assertResponse(expected, actual);
    }
    
    public void testSingleRandomUserWithWhereEvaluatingTrue() throws IOException {
        index("{\"test\":\"doc1\"}",
              "{\"test\":\"doc2\"}",
              "{\"test\":\"doc3\"}");
        String mode = randomMode().toString();
        String randomUserName = users.get(0);

        Map<String, Object> expected = new HashMap<>();
        expected.put("columns", Arrays.asList(
                columnInfo(mode, "USER()", "keyword", JDBCType.VARCHAR, 0)));
        expected.put("rows", Arrays.asList(Arrays.asList(randomUserName),
                                           Arrays.asList(randomUserName),
                                           Arrays.asList(randomUserName)));
        Map<String, Object> actual = runSql(randomUserName, mode, SQL + " FROM test WHERE USER()='" + randomUserName + "' LIMIT 3");
        assertResponse(expected, actual);
    }
    
    @AwaitsFix(bugUrl="https://github.com/elastic/elasticsearch/issues/35980")
    public void testSingleRandomUserWithWhereEvaluatingFalse() throws IOException {
        index("{\"test\":\"doc1\"}",
              "{\"test\":\"doc2\"}",
              "{\"test\":\"doc3\"}");
        String mode = randomMode().toString();
        String randomUserName = users.get(0);

        Map<String, Object> expected = new HashMap<>();
        expected.put("columns", Arrays.asList(
                columnInfo(mode, "USER()", "keyword", JDBCType.VARCHAR, 0)));
        expected.put("rows", Collections.<ArrayList<String>>emptyList());
        String anotherRandomUserName = randomValueOtherThan(randomUserName, () -> randomAlphaOfLengthBetween(1, 15));
        Map<String, Object> actual = runSql(randomUserName, mode, SQL + " FROM test WHERE USER()='" + anotherRandomUserName + "' LIMIT 3");
        assertResponse(expected, actual);
    }
    
    public void testMultipleRandomUsersAccess() throws IOException {
        // run 30 queries and pick randomly each time one of the 5-15 users created previously
        for (int i = 0; i < 30; i++) {
            String mode = randomMode().toString();
            String randomlyPickedUsername = randomFrom(users);
            Map<String, Object> expected = new HashMap<>();

            expected.put("columns", Arrays.asList(
                    columnInfo(mode, "USER()", "keyword", JDBCType.VARCHAR, 0)));
            expected.put("rows", Arrays.asList(Arrays.asList(randomlyPickedUsername)));
            Map<String, Object> actual = runSql(randomlyPickedUsername, mode, SQL);
            
            // expect the user that ran the query to be the same as the one returned by the `USER()` function
            assertResponse(expected, actual);
        }
    }
    
    public void testSingleUserSelectFromIndex() throws IOException {
        index("{\"test\":\"doc1\"}",
              "{\"test\":\"doc2\"}",
              "{\"test\":\"doc3\"}");
        String mode = randomMode().toString();
        String randomUserName = users.get(0);
        
        Map<String, Object> expected = new HashMap<>();
        expected.put("columns", Arrays.asList(
                columnInfo(mode, "USER()", "keyword", JDBCType.VARCHAR, 0)));
        expected.put("rows", Arrays.asList(Arrays.asList(randomUserName),
                                           Arrays.asList(randomUserName),
                                           Arrays.asList(randomUserName)));
        Map<String, Object> actual = runSql(randomUserName, mode, "SELECT USER() FROM test LIMIT 3");
        
        assertResponse(expected, actual);
    }
    
    private void createUser(String name, String role) throws IOException {
        Request request = new Request("PUT", "/_security/user/" + name);
        XContentBuilder user = JsonXContent.contentBuilder().prettyPrint();
        user.startObject(); {
            user.field("password", "testpass");
            user.field("roles", role);
        }
        user.endObject();
        request.setJsonEntity(Strings.toString(user));
        client().performRequest(request);
    }
    
    private void deleteUser(String name) throws IOException {
        Request request = new Request("DELETE", "/_security/user/" + name);
        client().performRequest(request);
    }
    
    private Map<String, Object> runSql(String asUser, String mode, String sql) throws IOException {
        return runSql(asUser, new StringEntity("{\"query\": \"" + sql + "\"" + mode(mode) + "}", ContentType.APPLICATION_JSON));
    }
    
    private Map<String, Object> runSql(String asUser, HttpEntity entity) throws IOException {
        Request request = new Request("POST", SQL_QUERY_REST_ENDPOINT);
        if (asUser != null) {
            RequestOptions.Builder options = request.getOptions().toBuilder();
            options.addHeader("es-security-runas-user", asUser);
            request.setOptions(options);
        }
        request.setEntity(entity);
        return toMap(client().performRequest(request));
    }
    
    private void assertResponse(Map<String, Object> expected, Map<String, Object> actual) {
        if (false == expected.equals(actual)) {
            NotEqualMessageBuilder message = new NotEqualMessageBuilder();
            message.compareMaps(actual, expected);
            fail("Response does not match:\n" + message.toString());
        }
    }
    
    private static Map<String, Object> toMap(Response response) throws IOException {
        try (InputStream content = response.getEntity().getContent()) {
            return XContentHelper.convertToMap(JsonXContent.jsonXContent, content, false);
        }
    }

    private void index(String... docs) throws IOException {
        Request request = new Request("POST", "/test/_bulk");
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
