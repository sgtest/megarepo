/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.action;

import org.elasticsearch.ElasticsearchSecurityException;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.common.network.NetworkModule;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.license.AbstractLicensesIntegrationTestCase;
import org.elasticsearch.license.License;
import org.elasticsearch.license.License.OperationMode;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.search.fetch.subphase.DocValueFieldsContext;
import org.elasticsearch.search.fetch.subphase.FetchSourceContext;
import org.elasticsearch.test.hamcrest.ElasticsearchAssertions;
import org.elasticsearch.transport.Netty4Plugin;
import org.elasticsearch.xpack.sql.plugin.SqlQueryAction;
import org.elasticsearch.xpack.sql.plugin.SqlQueryResponse;
import org.elasticsearch.xpack.sql.plugin.SqlTranslateAction;
import org.elasticsearch.xpack.sql.plugin.SqlTranslateResponse;
import org.hamcrest.Matchers;
import org.junit.Before;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Locale;

import static org.elasticsearch.license.XPackLicenseStateTests.randomBasicStandardOrGold;
import static org.elasticsearch.license.XPackLicenseStateTests.randomTrialBasicStandardGoldOrPlatinumMode;
import static org.elasticsearch.license.XPackLicenseStateTests.randomTrialOrPlatinumMode;
import static org.hamcrest.Matchers.equalTo;

public class SqlLicenseIT extends AbstractLicensesIntegrationTestCase {
    @Override
    protected boolean ignoreExternalCluster() {
        return true;
    }

    @Before
    public void resetLicensing() throws Exception {
        enableJdbcLicensing();
    }

    @Override
    protected Collection<Class<? extends Plugin>> nodePlugins() {
        // Add Netty so we can test JDBC licensing because only exists on the REST layer.
        List<Class<? extends Plugin>> plugins = new ArrayList<>(super.nodePlugins());
        plugins.add(Netty4Plugin.class);
        return plugins;
    }

    @Override
    protected boolean addMockHttpTransport() {
        return false; // enable http
    }

    @Override
    protected Settings nodeSettings(int nodeOrdinal) {
        // Enable http so we can test JDBC licensing because only exists on the REST layer.
        return Settings.builder()
                .put(super.nodeSettings(nodeOrdinal))
                .put(NetworkModule.HTTP_TYPE_KEY, Netty4Plugin.NETTY_HTTP_TRANSPORT_NAME)
                .build();
    }

    private static OperationMode randomValidSqlLicenseType() {
        return randomTrialBasicStandardGoldOrPlatinumMode();
    }

    private static OperationMode randomInvalidSqlLicenseType() {
        return OperationMode.MISSING;
    }

    private static OperationMode randomValidJdbcLicenseType() {
        return randomTrialOrPlatinumMode();
    }

    private static OperationMode randomInvalidJdbcLicenseType() {
        return randomBasicStandardOrGold();
    }

    public void enableSqlLicensing() throws Exception {
        updateLicensing(randomValidSqlLicenseType());
    }

    public void disableSqlLicensing() throws Exception {
        updateLicensing(randomInvalidSqlLicenseType());
    }

    public void enableJdbcLicensing() throws Exception {
        updateLicensing(randomValidJdbcLicenseType());
    }

    public void disableJdbcLicensing() throws Exception {
        updateLicensing(randomInvalidJdbcLicenseType());
    }

    public void updateLicensing(OperationMode licenseOperationMode) throws Exception {
        String licenseType = licenseOperationMode.name().toLowerCase(Locale.ROOT);
        wipeAllLicenses();
        if (licenseType.equals("missing")) {
            putLicenseTombstone();
        } else {
            License license = org.elasticsearch.license.TestUtils.generateSignedLicense(licenseType, TimeValue.timeValueMinutes(1));
            putLicense(license);
        }
    }

    public void testSqlQueryActionLicense() throws Exception {
        setupTestIndex();
        disableSqlLicensing();

        ElasticsearchSecurityException e = expectThrows(ElasticsearchSecurityException.class,
                () -> client().prepareExecute(SqlQueryAction.INSTANCE).query("SELECT * FROM test").get());
        assertThat(e.getMessage(), equalTo("current license is non-compliant for [sql]"));
        enableSqlLicensing();

        SqlQueryResponse response = client().prepareExecute(SqlQueryAction.INSTANCE).query("SELECT * FROM test").get();
        assertThat(response.size(), Matchers.equalTo(2L));
    }


    public void testSqlQueryActionJdbcModeLicense() throws Exception {
        setupTestIndex();
        disableJdbcLicensing();

        ElasticsearchSecurityException e = expectThrows(ElasticsearchSecurityException.class,
                () -> client().prepareExecute(SqlQueryAction.INSTANCE).query("SELECT * FROM test").mode("jdbc").get());
        assertThat(e.getMessage(), equalTo("current license is non-compliant for [jdbc]"));
        enableJdbcLicensing();

        SqlQueryResponse response = client().prepareExecute(SqlQueryAction.INSTANCE).query("SELECT * FROM test").mode("jdbc").get();
        assertThat(response.size(), Matchers.equalTo(2L));
    }

    public void testSqlTranslateActionLicense() throws Exception {
        setupTestIndex();
        disableSqlLicensing();

        ElasticsearchSecurityException e = expectThrows(ElasticsearchSecurityException.class,
                () -> client().prepareExecute(SqlTranslateAction.INSTANCE).query("SELECT * FROM test").get());
        assertThat(e.getMessage(), equalTo("current license is non-compliant for [sql]"));
        enableSqlLicensing();

        SqlTranslateResponse response = client().prepareExecute(SqlTranslateAction.INSTANCE).query("SELECT * FROM test").get();
        SearchSourceBuilder source = response.source();
        assertThat(source.docValueFields(), Matchers.contains(
                new DocValueFieldsContext.FieldAndFormat("count", DocValueFieldsContext.USE_DEFAULT_FORMAT)));
        FetchSourceContext fetchSource = source.fetchSource();
        assertThat(fetchSource.includes(), Matchers.arrayContaining("data"));
    }

    // TODO test SqlGetIndicesAction. Skipping for now because of lack of serialization support.

    private void setupTestIndex() {
        ElasticsearchAssertions.assertAcked(client().admin().indices().prepareCreate("test").get());
        client().prepareBulk()
                .add(new IndexRequest("test", "doc", "1").source("data", "bar", "count", 42))
                .add(new IndexRequest("test", "doc", "2").source("data", "baz", "count", 43))
                .setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE)
                .get();
    }

}
