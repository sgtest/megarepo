/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.integration;

import org.elasticsearch.action.admin.indices.get.GetIndexResponse;
import org.elasticsearch.action.admin.indices.mapping.get.GetFieldMappingsResponse;
import org.elasticsearch.action.admin.indices.mapping.get.GetFieldMappingsResponse.FieldMappingMetadata;
import org.elasticsearch.action.admin.indices.mapping.get.GetMappingsResponse;
import org.elasticsearch.action.admin.indices.validate.query.ValidateQueryResponse;
import org.elasticsearch.action.search.MultiSearchResponse;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.cluster.metadata.MappingMetadata;
import org.elasticsearch.common.collect.ImmutableOpenMap;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.test.NativeRealmIntegTestCase;
import org.elasticsearch.xpack.core.security.authc.support.UsernamePasswordToken;

import java.util.Map;

import static java.util.Collections.singletonMap;
import static org.hamcrest.Matchers.arrayContaining;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;
import static org.hamcrest.Matchers.hasKey;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.notNullValue;

public class KibanaUserRoleIntegTests extends NativeRealmIntegTestCase {

    protected static final SecureString USERS_PASSWD = new SecureString("change_me".toCharArray());

    @Override
    public String configRoles() {
        return super.configRoles() + "\n" +
                "my_kibana_user:\n" +
                "  indices:\n" +
                "    - names: 'logstash-*'\n" +
                "      privileges:\n" +
                "        - view_index_metadata\n" +
                "        - read\n";
    }

    @Override
    public String configUsers() {
        final String usersPasswdHashed = new String(getFastStoredHashAlgoForTests().hash(USERS_PASSWD));
        return super.configUsers() +
            "kibana_user:" + usersPasswdHashed;
    }

    @Override
    public String configUsersRoles() {
        return super.configUsersRoles() +
                "my_kibana_user:kibana_user\n" +
                "kibana_user:kibana_user";
    }

    public void testFieldMappings() throws Exception {
        final String index = "logstash-20-12-2015";
        final String field = "foo";
        indexRandom(true, client().prepareIndex().setIndex(index).setSource(field, "bar"));

        GetFieldMappingsResponse response = client().admin().indices().prepareGetFieldMappings().addIndices("logstash-*").setFields("*")
                .includeDefaults(true).get();
        FieldMappingMetadata fieldMappingMetadata = response.fieldMappings(index, field);
        assertThat(fieldMappingMetadata, notNullValue());

        response = client()
                .filterWithHeader(singletonMap("Authorization", UsernamePasswordToken.basicAuthHeaderValue("kibana_user", USERS_PASSWD)))
                .admin().indices().prepareGetFieldMappings().addIndices("logstash-*")
                .setFields("*")
                .includeDefaults(true).get();
        FieldMappingMetadata fieldMappingMetadata1 = response.fieldMappings(index, field);
        assertThat(fieldMappingMetadata1, notNullValue());
        assertThat(fieldMappingMetadata1.fullName(), equalTo(fieldMappingMetadata.fullName()));
    }

    public void testValidateQuery() throws Exception {
        final String index = "logstash-20-12-2015";
        final String type = "event";
        final String field = "foo";
        indexRandom(true, client().prepareIndex().setIndex(index).setSource(field, "bar"));

        ValidateQueryResponse response = client().admin().indices()
                .prepareValidateQuery(index).setQuery(QueryBuilders.termQuery(field, "bar")).get();
        assertThat(response.isValid(), is(true));

        response = client()
                .filterWithHeader(singletonMap("Authorization", UsernamePasswordToken.basicAuthHeaderValue("kibana_user", USERS_PASSWD)))
                .admin().indices()
                .prepareValidateQuery(index)
                .setQuery(QueryBuilders.termQuery(field, "bar")).get();
        assertThat(response.isValid(), is(true));
    }

    public void testSearchAndMSearch() throws Exception {
        final String index = "logstash-20-12-2015";
        final String type = "event";
        final String field = "foo";
        indexRandom(true, client().prepareIndex().setIndex(index).setSource(field, "bar"));

        SearchResponse response = client().prepareSearch(index).setQuery(QueryBuilders.matchAllQuery()).get();
        final long hits = response.getHits().getTotalHits().value;
        assertThat(hits, greaterThan(0L));
        response = client()
                .filterWithHeader(singletonMap("Authorization", UsernamePasswordToken.basicAuthHeaderValue("kibana_user", USERS_PASSWD)))
                .prepareSearch(index)
                .setQuery(QueryBuilders.matchAllQuery()).get();
        assertEquals(response.getHits().getTotalHits().value, hits);


        MultiSearchResponse multiSearchResponse = client().prepareMultiSearch()
                .add(client().prepareSearch(index).setQuery(QueryBuilders.matchAllQuery())).get();
        final long multiHits = multiSearchResponse.getResponses()[0].getResponse().getHits().getTotalHits().value;
        assertThat(hits, greaterThan(0L));
        multiSearchResponse = client()
                .filterWithHeader(singletonMap("Authorization", UsernamePasswordToken.basicAuthHeaderValue("kibana_user", USERS_PASSWD)))
                .prepareMultiSearch()
                .add(client().prepareSearch(index).setQuery(QueryBuilders.matchAllQuery())).get();
        assertEquals(multiSearchResponse.getResponses()[0].getResponse().getHits().getTotalHits().value, multiHits);
    }

    public void testGetIndex() throws Exception {
        final String index = "logstash-20-12-2015";
        final String type = "event";
        final String field = "foo";
        indexRandom(true, client().prepareIndex().setIndex(index).setSource(field, "bar"));

        GetIndexResponse response = client().admin().indices().prepareGetIndex().setIndices(index).get();
        assertThat(response.getIndices(), arrayContaining(index));

        response = client()
                .filterWithHeader(singletonMap("Authorization", UsernamePasswordToken.basicAuthHeaderValue("kibana_user", USERS_PASSWD)))
                .admin().indices().prepareGetIndex()
                .setIndices(index).get();
        assertThat(response.getIndices(), arrayContaining(index));
    }

    public void testGetMappings() throws Exception {
        final String index = "logstash-20-12-2015";
        final String type = "_doc";
        final String field = "foo";
        indexRandom(true, client().prepareIndex().setIndex(index).setSource(field, "bar"));

        GetMappingsResponse response = client()
                .filterWithHeader(singletonMap("Authorization", UsernamePasswordToken.basicAuthHeaderValue("kibana_user", USERS_PASSWD)))
                .admin()
                .indices()
                .prepareGetMappings("logstash-*")
                .get();
        ImmutableOpenMap<String, MappingMetadata> mappingsMap = response.getMappings();
        assertNotNull(mappingsMap);
        assertNotNull(mappingsMap.get(index));
        MappingMetadata mappingMetadata = mappingsMap.get(index);
        assertThat(mappingMetadata.getSourceAsMap(), hasKey("properties"));
        assertThat(mappingMetadata.getSourceAsMap().get("properties"), instanceOf(Map.class));
        Map<String, Object> propertiesMap = (Map<String, Object>) mappingMetadata.getSourceAsMap().get("properties");
        assertThat(propertiesMap, hasKey(field));
    }

    // TODO: When we have an XPackIntegTestCase, this should test that we can send MonitoringBulkActions

}
