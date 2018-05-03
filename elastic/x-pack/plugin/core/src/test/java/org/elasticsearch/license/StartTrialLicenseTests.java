/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.license;

import org.elasticsearch.client.Response;
import org.elasticsearch.client.ResponseException;
import org.elasticsearch.client.RestClient;
import org.elasticsearch.common.io.Streams;
import org.elasticsearch.common.network.NetworkModule;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.test.ESIntegTestCase;
import org.elasticsearch.transport.Netty4Plugin;
import org.elasticsearch.xpack.core.LocalStateCompositeXPackPlugin;
import org.elasticsearch.xpack.core.XPackClientPlugin;

import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.Collection;

import static org.elasticsearch.test.ESIntegTestCase.Scope.SUITE;

@ESIntegTestCase.ClusterScope(scope = SUITE)
public class StartTrialLicenseTests extends AbstractLicensesIntegrationTestCase {

    @Override
    protected boolean addMockHttpTransport() {
        return false; // enable http
    }

    @Override
    protected Settings nodeSettings(int nodeOrdinal) {
        return Settings.builder()
                .put(super.nodeSettings(nodeOrdinal))
                .put("node.data", true)
                .put(LicenseService.SELF_GENERATED_LICENSE_TYPE.getKey(), "basic").build();
    }

    @Override
    protected Collection<Class<? extends Plugin>> nodePlugins() {
        return Arrays.asList(LocalStateCompositeXPackPlugin.class, Netty4Plugin.class);
    }

    @Override
    protected Collection<Class<? extends Plugin>> transportClientPlugins() {
        return Arrays.asList(XPackClientPlugin.class, Netty4Plugin.class);
    }

    public void testStartTrial() throws Exception {
        LicensingClient licensingClient = new LicensingClient(client());
        ensureStartingWithBasic();

        RestClient restClient = getRestClient();
        Response response = restClient.performRequest("GET", "/_xpack/license/trial_status");
        String body = Streams.copyToString(new InputStreamReader(response.getEntity().getContent(), StandardCharsets.UTF_8));
        assertEquals(200, response.getStatusLine().getStatusCode());
        assertEquals("{\"eligible_to_start_trial\":true}", body);

        // Test that starting will fail without acknowledgement
        Response response2 = restClient.performRequest("POST", "/_xpack/license/start_trial");
        String body2 = Streams.copyToString(new InputStreamReader(response2.getEntity().getContent(), StandardCharsets.UTF_8));
        assertEquals(200, response2.getStatusLine().getStatusCode());
        assertTrue(body2.contains("\"trial_was_started\":false"));
        assertTrue(body2.contains("\"error_message\":\"Operation failed: Needs acknowledgement.\""));
        assertTrue(body2.contains("\"acknowledged\":false"));

        assertBusy(() -> {
            GetLicenseResponse getLicenseResponse = licensingClient.prepareGetLicense().get();
            assertEquals("basic", getLicenseResponse.license().type());
        });

        String type = randomFrom(LicenseService.VALID_TRIAL_TYPES);

        Response response3 = restClient.performRequest("POST", "/_xpack/license/start_trial?acknowledge=true&type=" + type);
        String body3 = Streams.copyToString(new InputStreamReader(response3.getEntity().getContent(), StandardCharsets.UTF_8));
        assertEquals(200, response3.getStatusLine().getStatusCode());
        assertTrue(body3.contains("\"trial_was_started\":true"));
        assertTrue(body3.contains("\"type\":\"" + type + "\""));
        assertTrue(body3.contains("\"acknowledged\":true"));

        assertBusy(() -> {
            GetLicenseResponse postTrialLicenseResponse = licensingClient.prepareGetLicense().get();
            assertEquals(type, postTrialLicenseResponse.license().type());
        });

        Response response4 = restClient.performRequest("GET", "/_xpack/license/trial_status");
        String body4 = Streams.copyToString(new InputStreamReader(response4.getEntity().getContent(), StandardCharsets.UTF_8));
        assertEquals(200, response4.getStatusLine().getStatusCode());
        assertEquals("{\"eligible_to_start_trial\":false}", body4);

        String secondAttemptType = randomFrom(LicenseService.VALID_TRIAL_TYPES);

        ResponseException ex = expectThrows(ResponseException.class,
                () -> restClient.performRequest("POST", "/_xpack/license/start_trial?acknowledge=true&type=" + secondAttemptType));
        Response response5 = ex.getResponse();
        String body5 = Streams.copyToString(new InputStreamReader(response5.getEntity().getContent(), StandardCharsets.UTF_8));
        assertEquals(403, response5.getStatusLine().getStatusCode());
        assertTrue(body5.contains("\"trial_was_started\":false"));
        assertTrue(body5.contains("\"error_message\":\"Operation failed: Trial was already activated.\""));
    }

    public void testInvalidType() throws Exception {
        ensureStartingWithBasic();

        ResponseException ex = expectThrows(ResponseException.class, () ->
                getRestClient().performRequest("POST", "/_xpack/license/start_trial?type=basic"));
        Response response = ex.getResponse();
        String body = Streams.copyToString(new InputStreamReader(response.getEntity().getContent(), StandardCharsets.UTF_8));
        assertEquals(400, response.getStatusLine().getStatusCode());
        assertTrue(body.contains("\"type\":\"illegal_argument_exception\""));
        assertTrue(body.contains("\"reason\":\"Cannot start trial of type [basic]. Valid trial types are ["));
    }

    private void ensureStartingWithBasic() throws Exception {
        LicensingClient licensingClient = new LicensingClient(client());
        GetLicenseResponse getLicenseResponse = licensingClient.prepareGetLicense().get();

        if ("basic".equals(getLicenseResponse.license().type()) == false) {
            licensingClient.preparePostStartBasic().setAcknowledge(true).get();
        }

        assertBusy(() -> {
            GetLicenseResponse postTrialLicenseResponse = licensingClient.prepareGetLicense().get();
            assertEquals("basic", postTrialLicenseResponse.license().type());
        });
    }
}
