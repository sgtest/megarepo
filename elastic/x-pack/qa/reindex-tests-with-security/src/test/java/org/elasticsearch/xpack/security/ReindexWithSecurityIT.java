/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security;

import org.elasticsearch.action.admin.cluster.node.info.NodeInfo;
import org.elasticsearch.action.admin.cluster.node.info.NodesInfoResponse;
import org.elasticsearch.index.reindex.BulkByScrollResponse;
import org.elasticsearch.common.network.NetworkModule;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.IndexNotFoundException;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.index.reindex.DeleteByQueryAction;
import org.elasticsearch.index.reindex.ReindexAction;
import org.elasticsearch.index.reindex.UpdateByQueryAction;
import org.elasticsearch.test.SecurityIntegTestCase;
import org.elasticsearch.xpack.core.security.SecurityField;

import java.util.Collection;
import java.util.stream.Collectors;

import static org.hamcrest.core.IsCollectionContaining.hasItem;

public class ReindexWithSecurityIT extends SecurityIntegTestCase {

    @Override
    protected Settings externalClusterClientSettings() {
        Settings.Builder builder = Settings.builder().put(super.externalClusterClientSettings());
        builder.put(NetworkModule.TRANSPORT_TYPE_KEY, SecurityField.NAME4);
        builder.put(SecurityField.USER_SETTING.getKey(), "test_admin:x-pack-test-password");
        return builder.build();
    }

    /**
     * TODO: this entire class should be removed. SecurityIntegTestCase is meant for tests, but we run against real xpack
     */
    @Override
    public void doAssertXPackIsInstalled() {
        // this assertion doesn't make sense with a real distribution, since there is not currently a way
        // from nodes info to see which modules are loaded
    }

    public void testDeleteByQuery() {
        createIndicesWithRandomAliases("test1", "test2", "test3");

        BulkByScrollResponse response = DeleteByQueryAction.INSTANCE.newRequestBuilder(client())
                .source("test1", "test2")
                .filter(QueryBuilders.matchAllQuery())
                .get();
        assertNotNull(response);

        response = DeleteByQueryAction.INSTANCE.newRequestBuilder(client())
                .source("test*")
                .filter(QueryBuilders.matchAllQuery())
                .get();
        assertNotNull(response);

        IndexNotFoundException e = expectThrows(IndexNotFoundException.class,
                () -> DeleteByQueryAction.INSTANCE.newRequestBuilder(client())
                        .source("test1", "index1")
                        .filter(QueryBuilders.matchAllQuery())
                        .get());
        assertEquals("no such index", e.getMessage());
    }

    public void testUpdateByQuery() {
        createIndicesWithRandomAliases("test1", "test2", "test3");

        BulkByScrollResponse response = UpdateByQueryAction.INSTANCE.newRequestBuilder(client()).source("test1", "test2").get();
        assertNotNull(response);

        response = UpdateByQueryAction.INSTANCE.newRequestBuilder(client()).source("test*").get();
        assertNotNull(response);

        IndexNotFoundException e = expectThrows(IndexNotFoundException.class,
                () -> UpdateByQueryAction.INSTANCE.newRequestBuilder(client()).source("test1", "index1").get());
        assertEquals("no such index", e.getMessage());
    }

    public void testReindex() {
        createIndicesWithRandomAliases("test1", "test2", "test3", "dest");

        BulkByScrollResponse response = ReindexAction.INSTANCE.newRequestBuilder(client()).source("test1", "test2")
                .destination("dest").get();
        assertNotNull(response);

        response = ReindexAction.INSTANCE.newRequestBuilder(client()).source("test*").destination("dest").get();
        assertNotNull(response);

        IndexNotFoundException e = expectThrows(IndexNotFoundException.class,
                () -> ReindexAction.INSTANCE.newRequestBuilder(client()).source("test1", "index1").destination("dest").get());
        assertEquals("no such index", e.getMessage());
    }
}
