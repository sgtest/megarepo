/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.idp.action;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.client.Client;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.RequestOptions;
import org.elasticsearch.client.Response;
import org.elasticsearch.client.ResponseException;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.test.ESIntegTestCase;
import org.elasticsearch.test.rest.yaml.ObjectPath;
import org.elasticsearch.xpack.core.security.action.CreateApiKeyRequestBuilder;
import org.elasticsearch.xpack.core.security.action.CreateApiKeyResponse;
import org.elasticsearch.xpack.core.security.authc.support.UsernamePasswordToken;
import org.elasticsearch.xpack.idp.saml.sp.SamlServiceProviderDocument;
import org.elasticsearch.xpack.idp.saml.sp.SamlServiceProviderIndex;
import org.elasticsearch.xpack.idp.saml.test.IdentityProviderIntegTestCase;
import org.elasticsearch.xpack.idp.saml.support.SamlFactory;
import org.opensaml.core.xml.util.XMLObjectSupport;
import org.opensaml.saml.common.SAMLObject;
import org.opensaml.saml.saml2.core.AuthnRequest;
import org.opensaml.saml.saml2.core.Issuer;
import org.opensaml.saml.saml2.core.NameIDPolicy;
import org.opensaml.security.SecurityException;
import org.opensaml.security.x509.X509Credential;
import org.opensaml.xmlsec.crypto.XMLSigningUtil;
import org.opensaml.xmlsec.signature.support.SignatureConstants;

import java.io.ByteArrayOutputStream;
import java.io.UnsupportedEncodingException;
import java.net.URL;
import java.net.URLEncoder;
import java.nio.charset.StandardCharsets;
import java.util.Base64;
import java.util.Collections;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.TimeUnit;
import java.util.zip.Deflater;
import java.util.zip.DeflaterOutputStream;

import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.elasticsearch.xpack.core.security.authc.support.UsernamePasswordToken.basicAuthHeaderValue;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasKey;
import static org.joda.time.DateTime.now;
import static org.opensaml.saml.saml2.core.NameIDType.TRANSIENT;

@ESIntegTestCase.ClusterScope(scope = ESIntegTestCase.Scope.SUITE, numClientNodes = 0, numDataNodes = 0)
public class SamlIdentityProviderTests extends IdentityProviderIntegTestCase {

    private final SamlFactory samlFactory = new SamlFactory();

    public void testIdpInitiatedSso() throws Exception {
        String acsUrl = "https://" + randomAlphaOfLength(12) + ".elastic-cloud.com/saml/acs";
        String entityId = SP_ENTITY_ID;
        registerServiceProvider(entityId, acsUrl);
        ensureGreen(SamlServiceProviderIndex.INDEX_NAME);

        // User login a.k.a exchange the user credentials for an API Key
        final String apiKeyCredentials = getApiKeyFromCredentials(SAMPLE_IDPUSER_NAME,
            new SecureString(SAMPLE_IDPUSER_PASSWORD.toCharArray()));
        // Make a request to init an SSO flow with the API Key as secondary authentication
        Request request = new Request("POST", "/_idp/saml/init");
        request.setOptions(RequestOptions.DEFAULT.toBuilder()
            .addHeader("Authorization", basicAuthHeaderValue(CONSOLE_USER_NAME,
                new SecureString(CONSOLE_USER_PASSWORD.toCharArray())))
            .addHeader("es-secondary-authorization", "ApiKey " + apiKeyCredentials)
            .build());
        request.setJsonEntity("{ \"entity_id\": \"" + entityId + "\"}");
        Response initResponse = getRestClient().performRequest(request);
        ObjectPath objectPath = ObjectPath.createFromResponse(initResponse);
        assertThat(objectPath.evaluate("post_url").toString(), equalTo(acsUrl));
        final String body = objectPath.evaluate("saml_response").toString();
        assertThat(body, containsString("Destination=\"" + acsUrl + "\""));
        assertThat(body, containsString("<saml2:Audience>" + entityId + "</saml2:Audience>"));
        assertThat(body, containsString("<saml2:NameID Format=\"" + TRANSIENT + "\">"));
        Map<String, String> serviceProvider = objectPath.evaluate("service_provider");
        assertThat(serviceProvider, hasKey("entity_id"));
        assertThat(serviceProvider.get("entity_id"), equalTo(entityId));
        assertContainsAttributeWithValue(body, "principal", SAMPLE_IDPUSER_NAME);
    }

    public void testIdPInitiatedSsoFailsForUnknownSP() throws Exception {
        String acsUrl = "https://" + randomAlphaOfLength(12) + ".elastic-cloud.com/saml/acs";
        String entityId = SP_ENTITY_ID;
        registerServiceProvider(entityId, acsUrl);
        ensureGreen(SamlServiceProviderIndex.INDEX_NAME);
        // User login a.k.a exchange the user credentials for an API Key
        final String apiKeyCredentials = getApiKeyFromCredentials(SAMPLE_IDPUSER_NAME,
            new SecureString(SAMPLE_IDPUSER_PASSWORD.toCharArray()));
        // Make a request to init an SSO flow with the API Key as secondary authentication
        Request request = new Request("POST", "/_idp/saml/init");
        request.setOptions(RequestOptions.DEFAULT.toBuilder()
            .addHeader("Authorization", basicAuthHeaderValue(CONSOLE_USER_NAME,
                new SecureString(CONSOLE_USER_PASSWORD.toCharArray())))
            .addHeader("es-secondary-authorization", "ApiKey " + apiKeyCredentials)
            .build());
        request.setJsonEntity("{ \"entity_id\": \"" + entityId + randomAlphaOfLength(3) + "\"}");
        ResponseException e = expectThrows(ResponseException.class, () -> getRestClient().performRequest(request));
        assertThat(e.getMessage(), containsString("is not registered with this Identity Provider"));
        assertThat(e.getResponse().getStatusLine().getStatusCode(), equalTo(RestStatus.BAD_REQUEST.getStatus()));
    }

    public void testIdPInitiatedSsoFailsWithoutSecondaryAuthentication() throws Exception {
        String acsUrl = "https://" + randomAlphaOfLength(12) + ".elastic-cloud.com/saml/acs";
        String entityId = SP_ENTITY_ID;
        registerServiceProvider(entityId, acsUrl);
        ensureGreen(SamlServiceProviderIndex.INDEX_NAME);
        // Make a request to init an SSO flow with the API Key as secondary authentication
        Request request = new Request("POST", "/_idp/saml/init");
        request.setOptions(REQUEST_OPTIONS_AS_CONSOLE_USER);
        request.setJsonEntity("{ \"entity_id\": \"" + entityId + "\"}");
        ResponseException e = expectThrows(ResponseException.class, () -> getRestClient().performRequest(request));
        assertThat(e.getMessage(), containsString("Request is missing secondary authentication"));
    }

    public void testSpInitiatedSso() throws Exception {
        String acsUrl = "https://" + randomAlphaOfLength(12) + ".elastic-cloud.com/saml/acs";
        String entityId = SP_ENTITY_ID;
        registerServiceProvider(entityId, acsUrl);
        ensureGreen(SamlServiceProviderIndex.INDEX_NAME);
        // Validate incoming authentication request
        Request validateRequest = new Request("POST", "/_idp/saml/validate");
        validateRequest.setOptions(REQUEST_OPTIONS_AS_CONSOLE_USER);
        final String nameIdFormat = TRANSIENT;
        final String relayString = randomBoolean() ? randomAlphaOfLength(8) : null;
        final boolean forceAuthn = true;
        final AuthnRequest authnRequest = buildAuthnRequest(entityId, new URL(acsUrl),
            new URL("https://idp.org/sso/redirect"), nameIdFormat, forceAuthn);
        final String query = getQueryString(authnRequest, relayString, false, null);
        validateRequest.setJsonEntity("{\"authn_request_query\":\"" + query + "\"}");
        Response validateResponse = getRestClient().performRequest(validateRequest);
        ObjectPath validateResponseObject = ObjectPath.createFromResponse(validateResponse);
        Map<String, String> serviceProvider = validateResponseObject.evaluate("service_provider");
        assertThat(serviceProvider, hasKey("entity_id"));
        assertThat(serviceProvider.get("entity_id"), equalTo(entityId));
        assertThat(validateResponseObject.evaluate("force_authn"), equalTo(forceAuthn));
        Map<String, String> authnState = validateResponseObject.evaluate("authn_state");
        assertThat(authnState, hasKey("nameid_format"));
        assertThat(authnState.get("nameid_format"), equalTo(nameIdFormat));
        assertThat(authnState, hasKey("entity_id"));
        assertThat(authnState.get("entity_id"), equalTo(entityId));
        assertThat(authnState, hasKey("acs_url"));
        assertThat(authnState.get("acs_url"), equalTo(acsUrl));
        assertThat(authnState, hasKey("authn_request_id"));
        final String expectedInResponeTo = authnState.get("authn_request_id");

        // User login a.k.a exchange the user credentials for an API Key
        final String apiKeyCredentials = getApiKeyFromCredentials(SAMPLE_IDPUSER_NAME,
            new SecureString(SAMPLE_IDPUSER_PASSWORD.toCharArray()));
        // Make a request to init an SSO flow with the API Key as secondary authentication
        Request initRequest = new Request("POST", "/_idp/saml/init");
        initRequest.setOptions(RequestOptions.DEFAULT.toBuilder()
            .addHeader("Authorization", basicAuthHeaderValue(CONSOLE_USER_NAME,
                new SecureString(CONSOLE_USER_PASSWORD.toCharArray())))
            .addHeader("es-secondary-authorization", "ApiKey " + apiKeyCredentials)
            .build());
        XContentBuilder authnStateBuilder = jsonBuilder();
        authnStateBuilder.map(authnState);
        initRequest.setJsonEntity("{ \"entity_id\":\"" + entityId + "\", \"authn_state\":" + Strings.toString(authnStateBuilder) + "}");
        Response initResponse = getRestClient().performRequest(initRequest);
        ObjectPath initResponseObject = ObjectPath.createFromResponse(initResponse);
        assertThat(initResponseObject.evaluate("post_url").toString(), equalTo(acsUrl));
        final String body = initResponseObject.evaluate("saml_response").toString();
        assertThat(body, containsString("<saml2p:StatusCode Value=\"urn:oasis:names:tc:SAML:2.0:status:Success\"/>"));
        assertThat(body, containsString("Destination=\"" + acsUrl + "\""));
        assertThat(body, containsString("<saml2:Audience>" + entityId + "</saml2:Audience>"));
        assertThat(body, containsString("<saml2:NameID Format=\"" + nameIdFormat + "\">"));
        assertThat(body, containsString("InResponseTo=\"" + expectedInResponeTo + "\""));
        Map<String, String> sp = initResponseObject.evaluate("service_provider");
        assertThat(sp, hasKey("entity_id"));
        assertThat(sp.get("entity_id"), equalTo(entityId));
        assertContainsAttributeWithValue(body, "principal", SAMPLE_IDPUSER_NAME);
    }

    public void testSpInitiatedSsoFailsForUnknownSp() throws Exception {
        String acsUrl = "https://" + randomAlphaOfLength(12) + ".elastic-cloud.com/saml/acs";
        String entityId = SP_ENTITY_ID;
        registerServiceProvider(entityId, acsUrl);
        ensureGreen(SamlServiceProviderIndex.INDEX_NAME);
        // Validate incoming authentication request
        Request validateRequest = new Request("POST", "/_idp/saml/validate");
        validateRequest.setOptions(REQUEST_OPTIONS_AS_CONSOLE_USER);
        final String nameIdFormat = TRANSIENT;
        final String relayString = null;
        final boolean forceAuthn = randomBoolean();
        final AuthnRequest authnRequest = buildAuthnRequest(entityId + randomAlphaOfLength(4), new URL(acsUrl),
            new URL("https://idp.org/sso/redirect"), nameIdFormat, forceAuthn);
        final String query = getQueryString(authnRequest, relayString, false, null);
        validateRequest.setJsonEntity("{\"authn_request_query\":\"" + query + "\"}");
        ResponseException e = expectThrows(ResponseException.class, () -> getRestClient().performRequest(validateRequest));
        assertThat(e.getMessage(), containsString("is not registered with this Identity Provider"));
        assertThat(e.getResponse().getStatusLine().getStatusCode(), equalTo(RestStatus.BAD_REQUEST.getStatus()));
    }

    public void testSpInitiatedSsoFailsForMalformedRequest() throws Exception {
        String acsUrl = "https://" + randomAlphaOfLength(12) + ".elastic-cloud.com/saml/acs";
        String entityId = SP_ENTITY_ID;
        registerServiceProvider(entityId, acsUrl);
        ensureGreen(SamlServiceProviderIndex.INDEX_NAME);
        // Validate incoming authentication request
        Request validateRequest = new Request("POST", "/_idp/saml/validate");
        validateRequest.setOptions(REQUEST_OPTIONS_AS_CONSOLE_USER);
        final String nameIdFormat = TRANSIENT;
        final String relayString = null;
        final boolean forceAuthn = randomBoolean();
        final AuthnRequest authnRequest = buildAuthnRequest(entityId + randomAlphaOfLength(4), new URL(acsUrl),
            new URL("https://idp.org/sso/redirect"), nameIdFormat, forceAuthn);
        final String query = getQueryString(authnRequest, relayString, false, null);
        // Skip http parameter name
        final String queryWithoutParam = query.substring(12);
        validateRequest.setJsonEntity("{\"authn_request_query\":\"" + queryWithoutParam + "\"}");
        ResponseException e = expectThrows(ResponseException.class, () -> getRestClient().performRequest(validateRequest));
        assertThat(e.getMessage(), containsString("does not contain a SAMLRequest parameter"));
        assertThat(e.getResponse().getStatusLine().getStatusCode(), equalTo(RestStatus.BAD_REQUEST.getStatus()));
        // arbitrarily trim the request
        final String malformedRequestQuery = query.substring(0, query.length() - randomIntBetween(10, 15));
        validateRequest.setJsonEntity("{\"authn_request_query\":\"" + malformedRequestQuery + "\"}");
        ResponseException e1 = expectThrows(ResponseException.class, () -> getRestClient().performRequest(validateRequest));
        assertThat(e1.getResponse().getStatusLine().getStatusCode(), equalTo(RestStatus.BAD_REQUEST.getStatus()));
    }

    private void registerServiceProvider(String entityId, String acsUrl) throws Exception {
        Map<String, Object> spFields = new HashMap<>();
        spFields.put(SamlServiceProviderDocument.Fields.ACS.getPreferredName(), acsUrl);
        spFields.put(SamlServiceProviderDocument.Fields.ENTITY_ID.getPreferredName(), entityId);
        spFields.put(SamlServiceProviderDocument.Fields.NAME_ID.getPreferredName(), TRANSIENT);
        spFields.put(SamlServiceProviderDocument.Fields.NAME.getPreferredName(), "Dummy SP");
        spFields.put("attributes", Map.of(
            "principal", "https://saml.elasticsearch.org/attributes/principal"));
        spFields.put("privileges", Map.of(
            "resource", entityId,
            "roles", Map.of("superuser", "sso:superuser")
        ));
        Request request =
            new Request("PUT", "/_idp/saml/sp/" + urlEncode(entityId) + "?refresh=" + WriteRequest.RefreshPolicy.IMMEDIATE.getValue());
        request.setOptions(REQUEST_OPTIONS_AS_CONSOLE_USER);
        final XContentBuilder builder = XContentFactory.jsonBuilder();
        builder.map(spFields);
        request.setJsonEntity(Strings.toString(builder));
        Response registerResponse = getRestClient().performRequest(request);
        assertThat(registerResponse.getStatusLine().getStatusCode(), equalTo(200));
        ObjectPath registerResponseObject = ObjectPath.createFromResponse(registerResponse);
        Map<String, Object> document = registerResponseObject.evaluate("document");
        assertThat(document, hasKey("_created"));
        assertThat(document.get("_created"), equalTo(true));
        Map<String, Object> serviceProvider = registerResponseObject.evaluate("service_provider");
        assertThat(serviceProvider, hasKey("entity_id"));
        assertThat(serviceProvider.get("entity_id"), equalTo(entityId));
        assertThat(serviceProvider, hasKey("enabled"));
        assertThat(serviceProvider.get("enabled"), equalTo(true));
    }

    private String getApiKeyFromCredentials(String username, SecureString password) {
        Client client = client().filterWithHeader(Collections.singletonMap("Authorization",
            UsernamePasswordToken.basicAuthHeaderValue(username, password)));
        final CreateApiKeyResponse response = new CreateApiKeyRequestBuilder(client)
            .setName("test key")
            .setExpiration(TimeValue.timeValueHours(TimeUnit.DAYS.toHours(7L)))
            .get();
        assertNotNull(response);
        return Base64.getEncoder().encodeToString(
            (response.getId() + ":" + response.getKey().toString()).getBytes(StandardCharsets.UTF_8));
    }

    private AuthnRequest buildAuthnRequest(String entityId, URL acs, URL destination, String nameIdFormat, boolean forceAuthn) {
        final Issuer issuer = samlFactory.buildObject(Issuer.class, Issuer.DEFAULT_ELEMENT_NAME);
        issuer.setValue(entityId);
        final NameIDPolicy nameIDPolicy = samlFactory.buildObject(NameIDPolicy.class, NameIDPolicy.DEFAULT_ELEMENT_NAME);
        nameIDPolicy.setFormat(nameIdFormat);
        final AuthnRequest authnRequest = samlFactory.buildObject(AuthnRequest.class, AuthnRequest.DEFAULT_ELEMENT_NAME);
        authnRequest.setID(samlFactory.secureIdentifier());
        authnRequest.setIssuer(issuer);
        authnRequest.setIssueInstant(now());
        authnRequest.setAssertionConsumerServiceURL(acs.toString());
        authnRequest.setDestination(destination.toString());
        authnRequest.setNameIDPolicy(nameIDPolicy);
        authnRequest.setForceAuthn(forceAuthn);
        return authnRequest;
    }

    private String getQueryString(AuthnRequest authnRequest, String relayState, boolean sign, @Nullable X509Credential credential) {
        try {
            final String request = deflateAndBase64Encode(authnRequest);
            String queryParam = "SAMLRequest=" + urlEncode(request);
            if (relayState != null) {
                queryParam += "&RelayState=" + urlEncode(relayState);
            }
            if (sign) {
                final String algo = SignatureConstants.ALGO_ID_SIGNATURE_RSA_SHA256;
                queryParam += "&SigAlg=" + urlEncode(algo);
                final byte[] sig = sign(queryParam, algo, credential);
                queryParam += "&Signature=" + urlEncode(base64Encode(sig));
            }
            return queryParam;
        } catch (Exception e) {
            throw new ElasticsearchException("Cannot construct SAML redirect", e);
        }
    }

    private String base64Encode(byte[] bytes) {
        return Base64.getEncoder().encodeToString(bytes);
    }

    private static String urlEncode(String param) throws UnsupportedEncodingException {
        return URLEncoder.encode(param, StandardCharsets.UTF_8.name());
    }

    private String deflateAndBase64Encode(SAMLObject message) throws Exception {
        Deflater deflater = new Deflater(Deflater.DEFLATED, true);
        try (ByteArrayOutputStream bytesOut = new ByteArrayOutputStream();
             DeflaterOutputStream deflaterStream = new DeflaterOutputStream(bytesOut, deflater)) {
            String messageStr = samlFactory.toString(XMLObjectSupport.marshall(message), false);
            deflaterStream.write(messageStr.getBytes(StandardCharsets.UTF_8));
            deflaterStream.finish();
            return base64Encode(bytesOut.toByteArray());
        }
    }

    private byte[] sign(String text, String algo, X509Credential credential) throws SecurityException {
        return sign(text.getBytes(StandardCharsets.UTF_8), algo, credential);
    }

    private byte[] sign(byte[] content, String algo, X509Credential credential) throws SecurityException {
        return XMLSigningUtil.signWithURI(credential, algo, content);
    }

    private void assertContainsAttributeWithValue(String message, String attribute, String value) {
        assertThat(message, containsString("<saml2:Attribute FriendlyName=\"" + attribute + "\" Name=\"https://saml.elasticsearch" +
            ".org/attributes/" + attribute + "\" NameFormat=\"urn:oasis:names:tc:SAML:2.0:attrname-format:uri\"><saml2:AttributeValue " +
            "xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:type=\"xsd:string\">" + value + "</saml2:AttributeValue></saml2" +
            ":Attribute>"));
    }
}
