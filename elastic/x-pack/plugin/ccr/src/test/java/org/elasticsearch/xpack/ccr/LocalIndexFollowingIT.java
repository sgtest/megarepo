/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.ccr;

import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.IndexSettings;
import org.elasticsearch.xpack.CcrSingleNodeTestCase;
import org.elasticsearch.xpack.core.ccr.action.PauseFollowAction;
import org.elasticsearch.xpack.core.ccr.action.PutFollowAction;
import org.elasticsearch.xpack.core.ccr.action.ResumeFollowAction;

import java.io.IOException;
import java.util.Map;

import static java.util.Collections.singletonMap;
import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.elasticsearch.test.hamcrest.ElasticsearchAssertions.assertAcked;
import static org.hamcrest.Matchers.equalTo;

public class LocalIndexFollowingIT extends CcrSingleNodeTestCase {

    public void testFollowIndex() throws Exception {
        final String leaderIndexSettings = getIndexSettings(2, 0,
            singletonMap(IndexSettings.INDEX_SOFT_DELETES_SETTING.getKey(), "true"));
        assertAcked(client().admin().indices().prepareCreate("leader").setSource(leaderIndexSettings, XContentType.JSON));
        ensureGreen("leader");

        final PutFollowAction.Request followRequest = new PutFollowAction.Request(getFollowRequest());
        client().execute(PutFollowAction.INSTANCE, followRequest).get();

        final long firstBatchNumDocs = randomIntBetween(2, 64);
        for (int i = 0; i < firstBatchNumDocs; i++) {
            client().prepareIndex("leader", "doc").setSource("{}", XContentType.JSON).get();
        }

        assertBusy(() -> {
            assertThat(client().prepareSearch("follower").get().getHits().totalHits, equalTo(firstBatchNumDocs));
        });

        final long secondBatchNumDocs = randomIntBetween(2, 64);
        for (int i = 0; i < secondBatchNumDocs; i++) {
            client().prepareIndex("leader", "doc").setSource("{}", XContentType.JSON).get();
        }

        assertBusy(() -> {
            assertThat(client().prepareSearch("follower").get().getHits().totalHits, equalTo(firstBatchNumDocs + secondBatchNumDocs));
        });

        PauseFollowAction.Request pauseRequest = new PauseFollowAction.Request();
        pauseRequest.setFollowIndex("follower");
        client().execute(PauseFollowAction.INSTANCE, pauseRequest);

        final long thirdBatchNumDocs = randomIntBetween(2, 64);
        for (int i = 0; i < thirdBatchNumDocs; i++) {
            client().prepareIndex("leader", "doc").setSource("{}", XContentType.JSON).get();
        }

        client().execute(ResumeFollowAction.INSTANCE, getFollowRequest()).get();
        assertBusy(() -> {
            assertThat(client().prepareSearch("follower").get().getHits().totalHits,
                equalTo(firstBatchNumDocs + secondBatchNumDocs + thirdBatchNumDocs));
        });
    }

    private String getIndexSettings(final int numberOfShards, final int numberOfReplicas,
                                    final Map<String, String> additionalIndexSettings) throws IOException {
        final String settings;
        try (XContentBuilder builder = jsonBuilder()) {
            builder.startObject();
            {
                builder.startObject("settings");
                {
                    builder.field("index.number_of_shards", numberOfShards);
                    builder.field("index.number_of_replicas", numberOfReplicas);
                    for (final Map.Entry<String, String> additionalSetting : additionalIndexSettings.entrySet()) {
                        builder.field(additionalSetting.getKey(), additionalSetting.getValue());
                    }
                }
                builder.endObject();
            }
            builder.endObject();
            settings = BytesReference.bytes(builder).utf8ToString();
        }
        return settings;
    }

}
