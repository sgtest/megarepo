/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.application.connector;

import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.search.SearchModule;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xcontent.ToXContent;
import org.elasticsearch.xcontent.XContentParser;
import org.elasticsearch.xcontent.XContentType;
import org.junit.Before;

import java.io.IOException;
import java.util.List;

import static java.util.Collections.emptyList;
import static org.elasticsearch.common.xcontent.XContentHelper.toXContent;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertToXContentEquivalent;
import static org.hamcrest.CoreMatchers.equalTo;

public class ConnectorTests extends ESTestCase {

    private NamedWriteableRegistry namedWriteableRegistry;

    @Before
    public void registerNamedObjects() {
        SearchModule searchModule = new SearchModule(Settings.EMPTY, emptyList());

        List<NamedWriteableRegistry.Entry> namedWriteables = searchModule.getNamedWriteables();
        namedWriteableRegistry = new NamedWriteableRegistry(namedWriteables);
    }

    public final void testRandomSerialization() throws IOException {
        for (int runs = 0; runs < 10; runs++) {
            Connector testInstance = ConnectorTestUtils.getRandomConnector();
            assertTransportSerialization(testInstance);
        }
    }

    public void testToXContent() throws IOException {
        String content = XContentHelper.stripWhitespace("""
            {
                "api_key_id": "test",
                "connector_id": "test-connector",
                "custom_scheduling": {
                    "schedule-key": {
                        "configuration_overrides": {
                            "domain_allowlist": [
                                "https://example.com"
                            ],
                            "max_crawl_depth": 1,
                            "seed_urls": [
                                "https://example.com/blog",
                                "https://example.com/info"
                            ],
                            "sitemap_discovery_disabled": true,
                            "sitemap_urls": [
                                "https://example.com/sitemap.xml"
                            ]
                        },
                        "enabled": true,
                        "interval": "0 0 12 * * ?",
                        "last_synced": null,
                        "name": "My Schedule"
                    }
                },
                "configuration": {},
                "description": "test-connector",
                "features": {
                    "document_level_security": {
                        "enabled": true
                    },
                    "filtering_advanced_config": true,
                    "sync_rules": {
                        "advanced": {
                            "enabled": false
                        },
                        "basic": {
                            "enabled": true
                        }
                    }
                },
                "filtering": [
                    {
                        "active": {
                            "advanced_snippet": {
                                "created_at": "2023-11-09T15:13:08.231Z",
                                "updated_at": "2023-11-09T15:13:08.231Z",
                                "value": {}
                            },
                            "rules": [
                                {
                                    "created_at": "2023-11-09T15:13:08.231Z",
                                    "field": "_",
                                    "id": "DEFAULT",
                                    "order": 0,
                                    "policy": "include",
                                    "rule": "regex",
                                    "updated_at": "2023-11-09T15:13:08.231Z",
                                    "value": ".*"
                                }
                            ],
                            "validation": {
                                "errors": [],
                                "state": "valid"
                            }
                        },
                        "domain": "DEFAULT",
                        "draft": {
                            "advanced_snippet": {
                                "created_at": "2023-11-09T15:13:08.231Z",
                                "updated_at": "2023-11-09T15:13:08.231Z",
                                "value": {}
                            },
                            "rules": [
                                {
                                    "created_at": "2023-11-09T15:13:08.231Z",
                                    "field": "_",
                                    "id": "DEFAULT",
                                    "order": 0,
                                    "policy": "include",
                                    "rule": "regex",
                                    "updated_at": "2023-11-09T15:13:08.231Z",
                                    "value": ".*"
                                }
                            ],
                            "validation": {
                                "errors": [],
                                "state": "valid"
                            }
                        }
                    }
                ],
                "index_name": "search-test",
                "is_native": true,
                "language": "polish",
                "last_access_control_sync_error": "some error",
                "last_access_control_sync_scheduled_at": "2023-11-09T15:13:08.231Z",
                "last_access_control_sync_status": "pending",
                "last_deleted_document_count": 42,
                "last_incremental_sync_scheduled_at": "2023-11-09T15:13:08.231Z",
                "last_indexed_document_count": 42,
                "last_seen": "2023-11-09T15:13:08.231Z",
                "last_sync_error": "some error",
                "last_sync_scheduled_at": "2024-11-09T15:13:08.231Z",
                "last_sync_status": "completed",
                "last_synced": "2024-11-09T15:13:08.231Z",
                "name": "test-name",
                "pipeline": {
                    "extract_binary_content": true,
                    "name": "ent-search-generic-ingestion",
                    "reduce_whitespace": true,
                    "run_ml_inference": false
                },
                "scheduling": {
                    "access_control": {
                        "enabled": false,
                        "interval": "0 0 0 * * ?"
                    },
                    "full": {
                        "enabled": false,
                        "interval": "0 0 0 * * ?"
                    },
                    "incremental": {
                        "enabled": false,
                        "interval": "0 0 0 * * ?"
                    }
                },
                "service_type": "google_drive",
                "status": "needs_configuration",
                "sync_now": false
            }""");

        Connector connector = Connector.fromXContentBytes(new BytesArray(content), XContentType.JSON);
        boolean humanReadable = true;
        BytesReference originalBytes = toShuffledXContent(connector, XContentType.JSON, ToXContent.EMPTY_PARAMS, humanReadable);
        Connector parsed;
        try (XContentParser parser = createParser(XContentType.JSON.xContent(), originalBytes)) {
            parsed = Connector.fromXContent(parser);
        }
        assertToXContentEquivalent(originalBytes, toXContent(parsed, XContentType.JSON, humanReadable), XContentType.JSON);
    }

    private void assertTransportSerialization(Connector testInstance) throws IOException {
        Connector deserializedInstance = copyInstance(testInstance);
        assertNotSame(testInstance, deserializedInstance);
        assertThat(testInstance, equalTo(deserializedInstance));
    }

    private Connector copyInstance(Connector instance) throws IOException {
        return copyWriteable(instance, namedWriteableRegistry, Connector::new);
    }
}
