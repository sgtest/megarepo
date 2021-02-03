/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.test.eql;

import static org.hamcrest.Matchers.instanceOf;
import static org.junit.Assert.assertThat;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.net.URL;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Map.Entry;

import org.apache.http.HttpHost;
import org.apache.logging.log4j.LogManager;
import org.elasticsearch.action.bulk.BulkRequest;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.client.RequestOptions;
import org.elasticsearch.client.RestClient;
import org.elasticsearch.client.RestHighLevelClient;
import org.elasticsearch.client.indices.CreateIndexRequest;
import org.elasticsearch.cluster.ClusterModule;
import org.elasticsearch.common.CheckedBiFunction;
import org.elasticsearch.common.xcontent.LoggingDeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContent;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.test.rest.ESRestTestCase;
import org.elasticsearch.xpack.ql.TestUtils;

/**
 * Loads EQL dataset into ES.
 *
 * Checks for predefined indices:
 * - endgame-140 - for existing data
 * - extra       - additional data
 *
 * While the loader could be made generic, the queries are bound to each index and generalizing that would make things way too complicated.
 */
public class DataLoader {
    public static final String TEST_INDEX = "endgame-140";
    public static final String TEST_EXTRA_INDEX = "extra";
    public static final String DATE_NANOS_INDEX = "eql_date_nanos";

    private static final Map<String, String[]> replacementPatterns = Collections.unmodifiableMap(getReplacementPatterns());

    private static final long FILETIME_EPOCH_DIFF = 11644473600000L;
    private static final long FILETIME_ONE_MILLISECOND = 10 * 1000;

    // runs as java main
    private static boolean main = false;

    private static Map<String, String[]> getReplacementPatterns() {
        final Map<String, String[]> map = new HashMap<>(1);
        map.put("[runtime_random_keyword_type]", new String[] {"keyword", "wildcard"});
        return map;
    }

    public static void main(String[] args) throws IOException {
        main = true;
        try (RestClient client = RestClient.builder(new HttpHost("localhost", 9200)).build()) {
            loadDatasetIntoEs(new RestHighLevelClient(
                client,
                ignore -> {
                },
                List.of()) {
            }, DataLoader::createParser);
        }
    }

    public static void loadDatasetIntoEs(RestHighLevelClient client,
        CheckedBiFunction<XContent, InputStream, XContentParser, IOException> p) throws IOException {

        //
        // Main Index
        //
        load(client, TEST_INDEX, true, p);
        //
        // Aux Index
        //
        load(client, TEST_EXTRA_INDEX, false, p);
        //
        // Date_Nanos index
        //
        // The data for this index are identical to the endgame-140.data with only the values for @timestamp changed.
        // There are mixed values with and without nanos precision so that the filtering is properly tested for both cases.
        load(client, DATE_NANOS_INDEX, false, p);
    }

    private static void load(RestHighLevelClient client, String indexName, boolean winFileTime,
                             CheckedBiFunction<XContent, InputStream, XContentParser, IOException> p) throws IOException {
        String name = "/data/" + indexName + ".mapping";
        URL mapping = DataLoader.class.getResource(name);
        if (mapping == null) {
            throw new IllegalArgumentException("Cannot find resource " + name);
        }
        name = "/data/" + indexName + ".data";
        URL data = DataLoader.class.getResource(name);
        if (data == null) {
            throw new IllegalArgumentException("Cannot find resource " + name);
        }
        createTestIndex(client, indexName, readMapping(mapping));
        loadData(client, indexName, winFileTime, data, p);
    }

    private static void createTestIndex(RestHighLevelClient client, String indexName, String mapping) throws IOException {
        CreateIndexRequest request = new CreateIndexRequest(indexName);
        if (mapping != null) {
            request.mapping(mapping, XContentType.JSON);
        }
        client.indices().create(request, RequestOptions.DEFAULT);
    }

    /**
     * Reads the mapping file, ignoring comments and replacing placeholders for random types.
     */
    private static String readMapping(URL resource) throws IOException {
        try (BufferedReader reader = TestUtils.reader(resource)) {
            StringBuilder b = new StringBuilder();
            String line;
            while ((line = reader.readLine()) != null) {
                if (line.startsWith("#") == false) {
                    for (Entry<String, String[]> entry : replacementPatterns.entrySet()) {
                        line = line.replace(entry.getKey(), randomOf(entry.getValue()));
                    }
                    b.append(line);
                }
            }
            return b.toString();
        }
    }

    private static CharSequence randomOf(String...values) {
        return main ? values[0] : ESRestTestCase.randomFrom(values);
    }

    @SuppressWarnings("unchecked")
    private static void loadData(RestHighLevelClient client, String indexName, boolean winfileTime, URL resource,
                                 CheckedBiFunction<XContent, InputStream, XContentParser, IOException> p)
        throws IOException {
        BulkRequest bulk = new BulkRequest();
        bulk.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);

        try (XContentParser parser = p.apply(JsonXContent.jsonXContent, TestUtils.inputStream(resource))) {
            List<Object> list = parser.list();
            for (Object item : list) {
                assertThat(item, instanceOf(Map.class));
                Map<String, Object> entry = (Map<String, Object>) item;
                if (winfileTime) {
                    transformDataset(entry);
                }
                bulk.add(new IndexRequest(indexName).source(entry, XContentType.JSON));
            }
        }

        if (bulk.numberOfActions() > 0) {
            BulkResponse bulkResponse = client.bulk(bulk, RequestOptions.DEFAULT);
            if (bulkResponse.hasFailures()) {
                LogManager.getLogger(DataLoader.class).info("Data loading FAILED");
            } else {
                LogManager.getLogger(DataLoader.class).info("Data loading OK");
            }
        }
    }

    private static void transformDataset(Map<String, Object> entry) {
        Object object = entry.get("timestamp");
        assertThat(object, instanceOf(Long.class));
        Long ts = (Long) object;
        // currently this is windows filetime
        entry.put("@timestamp", winFileTimeToUnix(ts));
    }

    public static long winFileTimeToUnix(final long filetime) {
        long ts = (filetime / FILETIME_ONE_MILLISECOND);
        return ts - FILETIME_EPOCH_DIFF;
    }

    private static XContentParser createParser(XContent xContent, InputStream data) throws IOException {
        NamedXContentRegistry contentRegistry = new NamedXContentRegistry(ClusterModule.getNamedXWriteables());
        return xContent.createParser(contentRegistry, LoggingDeprecationHandler.INSTANCE, data);
    }
}
