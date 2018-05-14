/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.qa.sql.jdbc;

import org.apache.http.HttpHost;
import org.apache.http.entity.ContentType;
import org.apache.http.entity.StringEntity;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.RestClient;
import org.elasticsearch.common.CheckedBiConsumer;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.SuppressForbidden;
import org.elasticsearch.common.logging.Loggers;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.json.JsonXContent;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import static java.util.Collections.emptyMap;
import static java.util.Collections.singletonMap;

public class DataLoader {

    public static void main(String[] args) throws Exception {
        try (RestClient client = RestClient.builder(new HttpHost("localhost", 9200)).build()) {
            loadDatasetIntoEs(client);
            Loggers.getLogger(DataLoader.class).info("Data loaded");
        }
    }

    protected static void loadDatasetIntoEs(RestClient client) throws Exception {
        loadDatasetIntoEs(client, "test_emp");
        loadDatasetIntoEs(client, "test_emp_copy");
        makeAlias(client, "test_alias", "test_emp", "test_emp_copy");
        makeAlias(client, "test_alias_emp", "test_emp", "test_emp_copy");
    }

    private static void createString(String name, XContentBuilder builder) throws Exception {
        builder.startObject(name).field("type", "text")
            .startObject("fields")
                .startObject("keyword").field("type", "keyword").endObject()
            .endObject()
       .endObject();
    }
    protected static void loadDatasetIntoEs(RestClient client, String index) throws Exception {
        Request request = new Request("PUT", "/" + index);
        XContentBuilder createIndex = JsonXContent.contentBuilder().startObject();
        createIndex.startObject("settings");
        {
            createIndex.field("number_of_shards", 1);
        }
        createIndex.endObject();
        createIndex.startObject("mappings");
        {
            createIndex.startObject("emp");
            {
                createIndex.startObject("properties");
                {
                    createIndex.startObject("emp_no").field("type", "integer").endObject();
                    createString("first_name", createIndex);
                    createString("last_name", createIndex);
                    createIndex.startObject("gender").field("type", "keyword").endObject();
                    createIndex.startObject("birth_date").field("type", "date").endObject();
                    createIndex.startObject("hire_date").field("type", "date").endObject();
                    createIndex.startObject("salary").field("type", "integer").endObject();
                    createIndex.startObject("languages").field("type", "byte").endObject();
                    {
                        createIndex.startObject("dep").field("type", "nested");
                        createIndex.startObject("properties");
                        createIndex.startObject("dep_id").field("type", "keyword").endObject();
                        createString("dep_name", createIndex);
                        createIndex.startObject("from_date").field("type", "date").endObject();
                        createIndex.startObject("to_date").field("type", "date").endObject();
                        createIndex.endObject();
                        createIndex.endObject();
                    }
                }
                createIndex.endObject();
            }
            createIndex.endObject();
        }
        createIndex.endObject().endObject();
        request.setJsonEntity(Strings.toString(createIndex));
        client.performRequest(request);

        Map<String, String> deps = new LinkedHashMap<>();
        csvToLines("departments", (titles, fields) -> deps.put(fields.get(0), fields.get(1)));

        Map<String, List<List<String>>> dep_emp = new LinkedHashMap<>();
        csvToLines("dep_emp", (titles, fields) -> {
            String emp_no = fields.get(0);
            List<List<String>> list = dep_emp.get(emp_no);
            if (list == null) {
                list = new ArrayList<>();
                dep_emp.put(emp_no, list);
            }
            List<String> dep = new ArrayList<>();
            // dep_id
            dep.add(fields.get(1));
            // dep_name (from departments)
            dep.add(deps.get(fields.get(1)));
            // from
            dep.add(fields.get(2));
            // to
            dep.add(fields.get(3));
            list.add(dep);
        });

        request = new Request("POST", "/" + index + "/emp/_bulk");
        request.addParameter("refresh", "true");
        StringBuilder bulk = new StringBuilder();
        csvToLines("employees", (titles, fields) -> {
            bulk.append("{\"index\":{}}\n");
            bulk.append('{');
            String emp_no = fields.get(1);
            for (int f = 0; f < fields.size(); f++) {
                if (f != 0) {
                    bulk.append(',');
                }
                bulk.append('"').append(titles.get(f)).append("\":\"").append(fields.get(f)).append('"');
            }
            // append department
            List<List<String>> list = dep_emp.get(emp_no);
            if (!list.isEmpty()) {
                bulk.append(", \"dep\" : [");
                for (List<String> dp : list) {
                    bulk.append("{");
                    bulk.append("\"dep_id\":\"" + dp.get(0) + "\",");
                    bulk.append("\"dep_name\":\"" + dp.get(1) + "\",");
                    bulk.append("\"from_date\":\"" + dp.get(2) + "\",");
                    bulk.append("\"to_date\":\"" + dp.get(3) + "\"");
                    bulk.append("},");
                }
                // remove last ,
                bulk.setLength(bulk.length() - 1);
                bulk.append("]");
            }

            bulk.append("}\n");
        });
        request.setJsonEntity(bulk.toString());
        client.performRequest(request);
    }

    protected static void makeAlias(RestClient client, String aliasName, String... indices) throws Exception {
        for (String index : indices) {
            client.performRequest(new Request("POST", "/" + index + "/_alias/" + aliasName));
        }
    }

    private static void csvToLines(String name, CheckedBiConsumer<List<String>, List<String>, Exception> consumeLine) throws Exception {
        String location = "/" + name + ".csv";
        URL dataSet = SqlSpecTestCase.class.getResource(location);
        if (dataSet == null) {
            throw new IllegalArgumentException("Can't find [" + location + "]");
        }

        try (BufferedReader reader = new BufferedReader(new InputStreamReader(readFromJarUrl(dataSet), StandardCharsets.UTF_8))) {
            String titlesString = reader.readLine();
            if (titlesString == null) {
                throw new IllegalArgumentException("[" + location + "] must contain at least a title row");
            }
            List<String> titles = Arrays.asList(titlesString.split(","));

            String line;
            while ((line = reader.readLine()) != null) {
                consumeLine.accept(titles, Arrays.asList(line.split(",")));
            }
        }
    }

    @SuppressForbidden(reason = "test reads from jar")
    public static InputStream readFromJarUrl(URL source) throws IOException {
        return source.openStream();
    }
}
