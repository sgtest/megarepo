/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.upgrades;

import org.elasticsearch.Version;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.Response;
import org.elasticsearch.client.ResponseException;
import org.elasticsearch.common.xcontent.support.XContentMapValues;

import java.io.IOException;
import java.util.Locale;
import java.util.Map;

import static org.elasticsearch.rest.action.search.RestSearchAction.TOTAL_HITS_AS_INT_PARAM;
import static org.hamcrest.Matchers.is;

public class UpgradeWithOldIndexSettingsIT extends AbstractRollingTestCase {

    private static final String INDEX_NAME = "test_index_old_settings";
    private static final String EXPECTED_WARNING = "[index.indexing.slowlog.level] setting was deprecated in Elasticsearch and will "
        + "be removed in a future release! See the breaking changes documentation for the next major version.";

    private static final String EXPECTED_V8_WARNING = "[index.indexing.slowlog.level] setting was deprecated in the previous Elasticsearch"
        + " release and is removed in this release.";

    @SuppressWarnings("unchecked")
    public void testOldIndexSettings() throws Exception {
        switch (CLUSTER_TYPE) {
            case OLD -> {
                Request createTestIndex = new Request("PUT", "/" + INDEX_NAME);
                createTestIndex.setJsonEntity("{\"settings\": {\"index.indexing.slowlog.level\": \"WARN\"}}");
                createTestIndex.setOptions(expectWarnings(EXPECTED_WARNING));
                if (UPGRADE_FROM_VERSION.before(Version.V_8_0_0)) {
                    // create index with settings no longer valid in 8.0
                    client().performRequest(createTestIndex);
                } else {
                    assertTrue(
                        expectThrows(ResponseException.class, () -> client().performRequest(createTestIndex)).getMessage()
                            .contains("unknown setting [index.indexing.slowlog.level]")
                    );

                    Request createTestIndex1 = new Request("PUT", "/" + INDEX_NAME);
                    client().performRequest(createTestIndex1);
                }

                // add some data
                Request bulk = new Request("POST", "/_bulk");
                bulk.addParameter("refresh", "true");
                if (UPGRADE_FROM_VERSION.before(Version.V_8_0_0)) {
                    bulk.setOptions(expectWarnings(EXPECTED_WARNING));
                }
                bulk.setJsonEntity(String.format(Locale.ROOT, """
                    {"index": {"_index": "%s"}}
                    {"f1": "v1", "f2": "v2"}
                    """, INDEX_NAME));
                client().performRequest(bulk);
            }
            case MIXED -> {
                // add some more data
                Request bulk = new Request("POST", "/_bulk");
                bulk.addParameter("refresh", "true");
                if (UPGRADE_FROM_VERSION.before(Version.V_8_0_0)) {
                    bulk.setOptions(expectWarnings(EXPECTED_WARNING));
                }
                bulk.setJsonEntity(String.format(Locale.ROOT, """
                    {"index": {"_index": "%s"}}
                    {"f1": "v3", "f2": "v4"}
                    """, INDEX_NAME));
                client().performRequest(bulk);
            }
            case UPGRADED -> {
                if (UPGRADE_FROM_VERSION.before(Version.V_8_0_0)) {
                    Request createTestIndex = new Request("PUT", "/" + INDEX_NAME + "/_settings");
                    // update index settings should work
                    createTestIndex.setJsonEntity("{\"index.indexing.slowlog.level\": \"INFO\"}");
                    createTestIndex.setOptions(expectWarnings(EXPECTED_V8_WARNING));
                    client().performRequest(createTestIndex);

                    // ensure we were able to change the setting, despite it having no effect
                    Request indexSettingsRequest = new Request("GET", "/" + INDEX_NAME + "/_settings");
                    Map<String, Object> response = entityAsMap(client().performRequest(indexSettingsRequest));

                    var slowLogLevel = (String) (XContentMapValues.extractValue(
                        INDEX_NAME + ".settings.index.indexing.slowlog.level",
                        response
                    ));

                    // check that we can read our old index settings
                    assertThat(slowLogLevel, is("INFO"));
                }
                assertCount(INDEX_NAME, 2);
            }
        }
    }

    private void assertCount(String index, int countAtLeast) throws IOException {
        Request searchTestIndexRequest = new Request("POST", "/" + index + "/_search");
        searchTestIndexRequest.addParameter(TOTAL_HITS_AS_INT_PARAM, "true");
        searchTestIndexRequest.addParameter("filter_path", "hits.total");
        Response searchTestIndexResponse = client().performRequest(searchTestIndexRequest);
        Map<String, Object> response = entityAsMap(searchTestIndexResponse);

        var hitsTotal = (Integer) (XContentMapValues.extractValue("hits.total", response));

        assertTrue(hitsTotal >= countAtLeast);
    }
}
