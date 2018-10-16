/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.ccr.action;

import org.elasticsearch.Version;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.metadata.IndexMetaData.State;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.index.MapperTestUtils;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.ccr.Ccr;
import org.elasticsearch.xpack.ccr.CcrSettings;
import org.elasticsearch.xpack.ccr.IndexFollowingIT;
import org.elasticsearch.xpack.core.ccr.action.ResumeFollowAction;

import java.io.IOException;
import java.util.HashMap;
import java.util.Map;

import static java.util.Collections.emptyMap;
import static java.util.Collections.singletonMap;
import static org.elasticsearch.xpack.ccr.action.TransportResumeFollowAction.validate;
import static org.hamcrest.Matchers.equalTo;

public class TransportResumeFollowActionTests extends ESTestCase {

    public void testValidation() throws IOException {
        final Map<String, String> customMetaData = new HashMap<>();
        customMetaData.put(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_SHARD_HISTORY_UUIDS, "uuid");
        customMetaData.put(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_UUID_KEY, "_na_");

        ResumeFollowAction.Request request = IndexFollowingIT.resumeFollow("index1", "index2");
        String[] UUIDs = new String[]{"uuid"};
        {
            // should fail, because leader index does not exist
            Exception e = expectThrows(IllegalArgumentException.class, () -> validate(request, null, null, null, null));
            assertThat(e.getMessage(), equalTo("leader index [leader_cluster:index1] does not exist"));
        }
        {
            // should fail, because follow index does not exist
            IndexMetaData leaderIMD = createIMD("index1", 5, Settings.EMPTY, emptyMap());
            Exception e = expectThrows(IllegalArgumentException.class,
                () -> validate(request, leaderIMD, null, null, null));
            assertThat(e.getMessage(), equalTo("follow index [index2] does not exist"));
        }
        {
            IndexMetaData leaderIMD = createIMD("index1", 5, Settings.EMPTY, null);
            IndexMetaData followIMD = createIMD("index2", 5, Settings.EMPTY, null);
            Exception e = expectThrows(IllegalArgumentException.class,
                () -> validate(request, leaderIMD, followIMD, UUIDs, null));
            assertThat(e.getMessage(), equalTo("follow index [index2] does not have ccr metadata"));
        }
        {
            // should fail because the recorded leader index uuid is not equal to the leader actual index
            IndexMetaData leaderIMD = createIMD("index1", 5, Settings.EMPTY, null);
            IndexMetaData followIMD = createIMD("index2", 5, Settings.EMPTY,
                singletonMap(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_UUID_KEY, "another-value"));
            Exception e = expectThrows(IllegalArgumentException.class,
                () -> validate(request, leaderIMD, followIMD, UUIDs, null));
            assertThat(e.getMessage(), equalTo("follow index [index2] should reference [_na_] as leader index but " +
                "instead reference [another-value] as leader index"));
        }
        {
            // should fail because the recorded leader index history uuid is not equal to the leader actual index history uuid:
            IndexMetaData leaderIMD = createIMD("index1", 5, Settings.EMPTY, null);
            Map<String, String> anotherCustomMetaData = new HashMap<>();
            anotherCustomMetaData.put(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_UUID_KEY, "_na_");
            anotherCustomMetaData.put(Ccr.CCR_CUSTOM_METADATA_LEADER_INDEX_SHARD_HISTORY_UUIDS, "another-uuid");
            IndexMetaData followIMD = createIMD("index2", 5, Settings.EMPTY, anotherCustomMetaData);
            Exception e = expectThrows(IllegalArgumentException.class,
                () -> validate(request, leaderIMD, followIMD, UUIDs, null));
            assertThat(e.getMessage(), equalTo("leader shard [index2][0] should reference [another-uuid] as history uuid but " +
                "instead reference [uuid] as history uuid"));
        }
        {
            // should fail because leader index does not have soft deletes enabled
            IndexMetaData leaderIMD = createIMD("index1", 5, Settings.EMPTY, null);
            IndexMetaData followIMD = createIMD("index2", 5, Settings.EMPTY, customMetaData);
            Exception e = expectThrows(IllegalArgumentException.class, () -> validate(request, leaderIMD, followIMD, UUIDs, null));
            assertThat(e.getMessage(), equalTo("leader index [leader_cluster:index1] does not have soft deletes enabled"));
        }
        {
            // should fail because the number of primary shards between leader and follow index are not equal
            IndexMetaData leaderIMD = createIMD("index1", 5, Settings.builder()
                .put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true").build(), null);
            IndexMetaData followIMD = createIMD("index2", 4, Settings.EMPTY, customMetaData);
            Exception e = expectThrows(IllegalArgumentException.class, () -> validate(request, leaderIMD, followIMD, UUIDs, null));
            assertThat(e.getMessage(),
                equalTo("leader index primary shards [5] does not match with the number of shards of the follow index [4]"));
        }
        {
            // should fail, because leader index is closed
            IndexMetaData leaderIMD = createIMD("index1", State.CLOSE, "{}", 5, Settings.builder()
                .put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true").build(), null);
            IndexMetaData followIMD = createIMD("index2", State.OPEN, "{}", 5, Settings.builder()
                .put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true").build(), customMetaData);
            Exception e = expectThrows(IllegalArgumentException.class, () -> validate(request, leaderIMD, followIMD, UUIDs, null));
            assertThat(e.getMessage(), equalTo("leader and follow index must be open"));
        }
        {
            // should fail, because index.xpack.ccr.following_index setting has not been enabled in leader index
            IndexMetaData leaderIMD = createIMD("index1", 1,
                Settings.builder().put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true").build(), null);
            IndexMetaData followIMD = createIMD("index2", 1, Settings.EMPTY, customMetaData);
            MapperService mapperService = MapperTestUtils.newMapperService(xContentRegistry(), createTempDir(), Settings.EMPTY, "index2");
            mapperService.updateMapping(null, followIMD);
            Exception e = expectThrows(IllegalArgumentException.class,
                () -> validate(request, leaderIMD, followIMD, UUIDs, mapperService));
            assertThat(e.getMessage(), equalTo("the following index [index2] is not ready to follow; " +
                "the setting [index.xpack.ccr.following_index] must be enabled."));
        }
        {
            // should fail, because leader has a field with the same name mapped as keyword and follower as text
            IndexMetaData leaderIMD = createIMD("index1", State.OPEN, "{\"properties\": {\"field\": {\"type\": \"keyword\"}}}", 5,
                Settings.builder().put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true").build(), null);
            IndexMetaData followIMD = createIMD("index2", State.OPEN, "{\"properties\": {\"field\": {\"type\": \"text\"}}}", 5,
                Settings.builder().put(CcrSettings.CCR_FOLLOWING_INDEX_SETTING.getKey(), true).build(), customMetaData);
            MapperService mapperService = MapperTestUtils.newMapperService(xContentRegistry(), createTempDir(), Settings.EMPTY, "index2");
            mapperService.updateMapping(null, followIMD);
            Exception e = expectThrows(IllegalArgumentException.class, () -> validate(request, leaderIMD, followIMD, UUIDs, mapperService));
            assertThat(e.getMessage(), equalTo("mapper [field] of different type, current_type [text], merged_type [keyword]"));
        }
        {
            // should fail because of non whitelisted settings not the same between leader and follow index
            String mapping = "{\"properties\": {\"field\": {\"type\": \"text\", \"analyzer\": \"my_analyzer\"}}}";
            IndexMetaData leaderIMD = createIMD("index1", State.OPEN, mapping, 5, Settings.builder()
                .put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true")
                .put("index.analysis.analyzer.my_analyzer.type", "custom")
                .put("index.analysis.analyzer.my_analyzer.tokenizer", "whitespace").build(), null);
            IndexMetaData followIMD = createIMD("index2", State.OPEN, mapping, 5, Settings.builder()
                .put(CcrSettings.CCR_FOLLOWING_INDEX_SETTING.getKey(), true)
                .put("index.analysis.analyzer.my_analyzer.type", "custom")
                .put("index.analysis.analyzer.my_analyzer.tokenizer", "standard").build(), customMetaData);
            Exception e = expectThrows(IllegalArgumentException.class, () -> validate(request, leaderIMD, followIMD, UUIDs, null));
            assertThat(e.getMessage(), equalTo("the leader and follower index settings must be identical"));
        }
        {
            // should fail because the following index does not have the following_index settings
            IndexMetaData leaderIMD = createIMD("index1", 5,
                Settings.builder().put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true").build(), null);
            Settings followingIndexSettings = randomBoolean() ? Settings.EMPTY :
                Settings.builder().put(CcrSettings.CCR_FOLLOWING_INDEX_SETTING.getKey(), false).build();
            IndexMetaData followIMD = createIMD("index2", 5, followingIndexSettings, customMetaData);
            MapperService mapperService = MapperTestUtils.newMapperService(xContentRegistry(), createTempDir(),
                followingIndexSettings, "index2");
            mapperService.updateMapping(null, followIMD);
            IllegalArgumentException error =
                    expectThrows(IllegalArgumentException.class, () -> validate(request, leaderIMD, followIMD, UUIDs, mapperService));
            assertThat(error.getMessage(), equalTo("the following index [index2] is not ready to follow; " +
                "the setting [index.xpack.ccr.following_index] must be enabled."));
        }
        {
            // should succeed
            IndexMetaData leaderIMD = createIMD("index1", 5, Settings.builder()
                .put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true").build(), null);
            IndexMetaData followIMD = createIMD("index2", 5, Settings.builder()
                .put(CcrSettings.CCR_FOLLOWING_INDEX_SETTING.getKey(), true).build(), customMetaData);
            MapperService mapperService = MapperTestUtils.newMapperService(xContentRegistry(), createTempDir(), Settings.EMPTY, "index2");
            mapperService.updateMapping(null, followIMD);
            validate(request, leaderIMD, followIMD, UUIDs, mapperService);
        }
        {
            // should succeed, index settings are identical
            String mapping = "{\"properties\": {\"field\": {\"type\": \"text\", \"analyzer\": \"my_analyzer\"}}}";
            IndexMetaData leaderIMD = createIMD("index1", State.OPEN, mapping, 5, Settings.builder()
                .put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true")
                .put("index.analysis.analyzer.my_analyzer.type", "custom")
                .put("index.analysis.analyzer.my_analyzer.tokenizer", "standard").build(), null);
            IndexMetaData followIMD = createIMD("index2", State.OPEN, mapping, 5, Settings.builder()
                .put(CcrSettings.CCR_FOLLOWING_INDEX_SETTING.getKey(), true)
                .put("index.analysis.analyzer.my_analyzer.type", "custom")
                .put("index.analysis.analyzer.my_analyzer.tokenizer", "standard").build(), customMetaData);
            MapperService mapperService = MapperTestUtils.newMapperService(xContentRegistry(), createTempDir(),
                followIMD.getSettings(), "index2");
            mapperService.updateMapping(null, followIMD);
            validate(request, leaderIMD, followIMD, UUIDs, mapperService);
        }
        {
            // should succeed despite whitelisted settings being different
            String mapping = "{\"properties\": {\"field\": {\"type\": \"text\", \"analyzer\": \"my_analyzer\"}}}";
            IndexMetaData leaderIMD = createIMD("index1", State.OPEN, mapping, 5, Settings.builder()
                .put(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true")
                .put(IndexSettings.INDEX_REFRESH_INTERVAL_SETTING.getKey(), "1s")
                .put("index.analysis.analyzer.my_analyzer.type", "custom")
                .put("index.analysis.analyzer.my_analyzer.tokenizer", "standard").build(), null);
            IndexMetaData followIMD = createIMD("index2", State.OPEN, mapping, 5, Settings.builder()
                .put(CcrSettings.CCR_FOLLOWING_INDEX_SETTING.getKey(), true)
                .put(IndexSettings.INDEX_REFRESH_INTERVAL_SETTING.getKey(), "10s")
                .put("index.analysis.analyzer.my_analyzer.type", "custom")
                .put("index.analysis.analyzer.my_analyzer.tokenizer", "standard").build(), customMetaData);
            MapperService mapperService = MapperTestUtils.newMapperService(xContentRegistry(), createTempDir(),
                followIMD.getSettings(), "index2");
            mapperService.updateMapping(null, followIMD);
            validate(request, leaderIMD, followIMD, UUIDs, mapperService);
        }
    }

    private static IndexMetaData createIMD(String index,
                                           int numberOfShards,
                                           Settings settings,
                                           Map<String, String> custom) throws IOException {
        return createIMD(index, State.OPEN, "{\"properties\": {}}", numberOfShards, settings, custom);
    }

    private static IndexMetaData createIMD(String index,
                                           State state,
                                           String mapping,
                                           int numberOfShards,
                                           Settings settings,
                                           Map<String, String> custom) throws IOException {
        IndexMetaData.Builder builder = IndexMetaData.builder(index)
            .settings(settings(Version.CURRENT).put(settings))
            .numberOfShards(numberOfShards)
            .state(state)
            .numberOfReplicas(0)
            .setRoutingNumShards(numberOfShards)
            .putMapping("_doc", mapping);

        if (custom != null) {
            builder.putCustom(Ccr.CCR_CUSTOM_METADATA_KEY, custom);
        }

        return builder.build();
    }

}
