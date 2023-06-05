/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.synonyms;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.DocWriteRequest;
import org.elasticsearch.action.bulk.BulkItemResponse;
import org.elasticsearch.action.bulk.BulkRequestBuilder;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.client.internal.OriginSettingClient;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.cluster.routing.Preference;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.index.reindex.DeleteByQueryAction;
import org.elasticsearch.index.reindex.DeleteByQueryRequest;
import org.elasticsearch.indices.SystemIndexDescriptor;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.xcontent.XContentBuilder;
import org.elasticsearch.xcontent.XContentFactory;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.Arrays;
import java.util.List;
import java.util.Objects;
import java.util.stream.Collectors;

import static org.elasticsearch.index.mapper.MapperService.SINGLE_MAPPING_NAME;
import static org.elasticsearch.xcontent.XContentFactory.jsonBuilder;

/**
 * Manages synonyms performing operations on the system index
 */
public class SynonymsManagementAPIService {
    public static final String SYNONYMS_INDEX_NAME_PATTERN = ".synonyms-*";
    public static final String SYNONYMS_INDEX_CONCRETE_NAME = ".synonyms-1";
    public static final String SYNONYMS_ALIAS_NAME = ".synonyms";

    public static final String SYNONYMS_FEATURE_NAME = "synonyms";
    public static final String SYNONYMS_SET_FIELD = "synonyms_set";
    public static final String SYNONYMS_FIELD = "synonyms";
    public static final String SYNONYM_RULE_ID_SEPARATOR = "|";

    private final Client client;

    public static final String SYNONYMS_ORIGIN = "synonyms";
    public static final SystemIndexDescriptor SYNONYMS_DESCRIPTOR = SystemIndexDescriptor.builder()
        .setIndexPattern(SYNONYMS_INDEX_NAME_PATTERN)
        .setDescription("Synonyms index for synonyms managed through APIs")
        .setPrimaryIndex(SYNONYMS_INDEX_CONCRETE_NAME)
        .setAliasName(SYNONYMS_ALIAS_NAME)
        .setMappings(mappings())
        .setSettings(settings())
        .setVersionMetaKey("version")
        .setOrigin(SYNONYMS_ORIGIN)
        .build();

    public SynonymsManagementAPIService(Client client) {
        this.client = new OriginSettingClient(client, SYNONYMS_ORIGIN);
    }

    private static XContentBuilder mappings() {
        try {
            XContentBuilder builder = jsonBuilder();
            builder.startObject();
            {
                builder.startObject(SINGLE_MAPPING_NAME);
                {
                    builder.startObject("_meta");
                    {
                        builder.field("version", Version.CURRENT.toString());
                    }
                    builder.endObject();
                    builder.field("dynamic", "strict");
                    builder.startObject("properties");
                    {
                        builder.startObject(SYNONYMS_FIELD);
                        {
                            builder.field("type", "match_only_text");
                        }
                        builder.endObject();
                        builder.startObject(SYNONYMS_SET_FIELD);
                        {
                            builder.field("type", "keyword");
                        }
                        builder.endObject();
                    }
                    builder.endObject();
                }
                builder.endObject();
            }
            builder.endObject();
            return builder;
        } catch (IOException e) {
            throw new UncheckedIOException("Failed to build mappings for " + SYNONYMS_INDEX_CONCRETE_NAME, e);
        }
    }

    public void getSynonymsSet(String resourceName, int from, int size, ActionListener<SynonymsSetResult> listener) {
        client.prepareSearch(SYNONYMS_ALIAS_NAME)
            .setQuery(QueryBuilders.termQuery(SYNONYMS_SET_FIELD, resourceName))
            .setFrom(from)
            .setSize(size)
            .setPreference(Preference.LOCAL.type())
            .setTrackTotalHits(true)
            .execute(listener.delegateFailure((searchResponseListener, searchResponse) -> {
                final long totalSynonymRules = searchResponse.getHits().getTotalHits().value;
                if (totalSynonymRules == 0) {
                    listener.onFailure(new ResourceNotFoundException("Synonym set [" + resourceName + "] not found"));
                    return;
                }
                final SynonymRule[] synonymRules = Arrays.stream(searchResponse.getHits().getHits())
                    .map(SynonymsManagementAPIService::hitToSynonymRule)
                    .toArray(SynonymRule[]::new);
                listener.onResponse(new SynonymsSetResult(totalSynonymRules, synonymRules));
            }));
    }

    private static SynonymRule hitToSynonymRule(SearchHit hit) {
        return new SynonymRule(
            externalSynonymRuleId(hit.getId()),
            (String) hit.getSourceAsMap().get(SynonymRule.SYNONYMS_FIELD.getPreferredName())
        );
    }

    // Retrieves the external synonym rule ID from the internal one for displaying to users
    private static String externalSynonymRuleId(String internalId) {
        int index = internalId.indexOf(SYNONYM_RULE_ID_SEPARATOR);
        if (index == -1) {
            throw new IllegalStateException("Synonym Rule ID [" + internalId + "] is incorrect");
        }
        return internalId.substring(index + 1);
    }

    // Retrieves the internal synonym rule ID to store it in the index. As the same synonym rule ID
    // can be used in different synonym sets, we prefix the ID with the synonym set to avoid collisions
    private static String internalSynonymRuleId(String resourceName, SynonymRule synonymRule) {
        String synonymRuleId = synonymRule.id();
        if (synonymRuleId == null) {
            synonymRuleId = UUIDs.base64UUID();
        }
        final String id = resourceName + SYNONYM_RULE_ID_SEPARATOR + synonymRuleId;
        return id;
    }

    public void putSynonymsSet(String resourceName, SynonymRule[] synonymsSet, ActionListener<UpdateSynonymsResult> listener) {

        // TODO Add synonym rules validation

        // Delete synonyms set if it existed previously. Avoid catching an index not found error by ignoring unavailable indices
        DeleteByQueryRequest dbqRequest = new DeleteByQueryRequest(SYNONYMS_ALIAS_NAME).setQuery(
            QueryBuilders.termQuery(SYNONYMS_SET_FIELD, resourceName)
        ).setIndicesOptions(IndicesOptions.fromOptions(true, true, false, false));

        client.execute(
            DeleteByQueryAction.INSTANCE,
            dbqRequest,
            listener.delegateFailure((deleteByQueryResponseListener, bulkByScrollResponse) -> {
                boolean created = bulkByScrollResponse.getDeleted() == 0;
                final List<BulkItemResponse.Failure> bulkFailures = bulkByScrollResponse.getBulkFailures();
                if (bulkFailures.isEmpty() == false) {
                    listener.onFailure(
                        new ElasticsearchException(
                            "Error updating synonyms: "
                                + bulkFailures.stream().map(BulkItemResponse.Failure::getMessage).collect(Collectors.joining("\n"))
                        )
                    );
                }

                // Insert as bulk requests
                BulkRequestBuilder bulkRequestBuilder = client.prepareBulk();
                try {
                    for (SynonymRule synonymRule : synonymsSet) {
                        try (XContentBuilder builder = XContentFactory.jsonBuilder()) {
                            builder.startObject();
                            {
                                builder.field(SYNONYMS_FIELD, synonymRule.synonyms());
                                builder.field(SYNONYMS_SET_FIELD, resourceName);
                            }
                            builder.endObject();

                            final IndexRequest indexRequest = new IndexRequest(SYNONYMS_ALIAS_NAME).opType(DocWriteRequest.OpType.INDEX)
                                .source(builder);
                            indexRequest.id(internalSynonymRuleId(resourceName, synonymRule));
                            bulkRequestBuilder.add(indexRequest);
                        }
                    }
                } catch (IOException ex) {
                    listener.onFailure(ex);
                }

                bulkRequestBuilder.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE)
                    .execute(deleteByQueryResponseListener.delegateFailure((bulkResponseListener, bulkResponse) -> {
                        if (bulkResponse.hasFailures() == false) {
                            UpdateSynonymsResult result = created ? UpdateSynonymsResult.CREATED : UpdateSynonymsResult.UPDATED;
                            bulkResponseListener.onResponse(result);
                        } else {
                            bulkResponseListener.onFailure(
                                new ElasticsearchException("Couldn't update synonyms: " + bulkResponse.buildFailureMessage())
                            );
                        }
                    }));
            })
        );
    }

    static Settings settings() {
        return Settings.builder()
            .put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, 1)
            .put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, 0)
            .put(IndexMetadata.SETTING_AUTO_EXPAND_REPLICAS, "0-all")
            .build();
    }

    public enum UpdateSynonymsResult {
        CREATED,
        UPDATED
    }

    public record SynonymsSetResult(long totalSynonymRules, SynonymRule[] synonymRules) {
        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            SynonymsSetResult that = (SynonymsSetResult) o;
            return totalSynonymRules == that.totalSynonymRules && Arrays.equals(synonymRules, that.synonymRules);
        }

        @Override
        public int hashCode() {
            int result = Objects.hash(totalSynonymRules);
            result = 31 * result + Arrays.hashCode(synonymRules);
            return result;
        }
    }

}
