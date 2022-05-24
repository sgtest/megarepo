/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.security.authc.jwt;

import com.nimbusds.jose.JWSAlgorithm;
import com.nimbusds.jose.JWSHeader;
import com.nimbusds.jose.util.Base64URL;
import com.nimbusds.jwt.JWTClaimsSet;
import com.nimbusds.jwt.PlainJWT;
import com.nimbusds.jwt.SignedJWT;

import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.settings.MockSecureSettings;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.settings.SettingsException;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.xpack.core.security.authc.AuthenticationResult;
import org.elasticsearch.xpack.core.security.authc.Realm;
import org.elasticsearch.xpack.core.security.authc.RealmSettings;
import org.elasticsearch.xpack.core.security.authc.jwt.JwtRealmSettings;
import org.elasticsearch.xpack.core.security.user.User;

import java.nio.charset.StandardCharsets;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Date;
import java.util.List;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;

public class JwtRealmAuthenticateTests extends JwtRealmTestCase {

    /**
     * Test with empty roles.
     * @throws Exception Unexpected test failure
     */
    public void testJwtAuthcRealmAuthcAuthzWithEmptyRoles() throws Exception {
        this.jwtIssuerAndRealms = this.generateJwtIssuerRealmPairs(
            this.createJwtRealmsSettingsBuilder(),
            new MinMax(1, 1), // realmsRange
            new MinMax(0, 1), // authzRange
            new MinMax(1, JwtRealmSettings.SUPPORTED_SIGNATURE_ALGORITHMS.size()), // algsRange
            new MinMax(1, 3), // audiencesRange
            new MinMax(1, 3), // usersRange
            new MinMax(0, 0), // rolesRange
            new MinMax(0, 1), // jwtCacheSizeRange
            randomBoolean() // createHttpsServer
        );
        final JwtIssuerAndRealm jwtIssuerAndRealm = this.randomJwtIssuerRealmPair();
        final User user = this.randomUser(jwtIssuerAndRealm.issuer());
        final SecureString jwt = this.randomJwt(jwtIssuerAndRealm, user);
        final SecureString clientSecret = jwtIssuerAndRealm.realm().clientAuthenticationSharedSecret;
        final MinMax jwtAuthcRange = new MinMax(2, 3);
        this.doMultipleAuthcAuthzAndVerifySuccess(jwtIssuerAndRealm.realm(), user, jwt, clientSecret, jwtAuthcRange);
    }

    /**
     * Test with no authz realms.
     * @throws Exception Unexpected test failure
     */
    public void testJwtAuthcRealmAuthcAuthzWithoutAuthzRealms() throws Exception {
        this.jwtIssuerAndRealms = this.generateJwtIssuerRealmPairs(
            this.createJwtRealmsSettingsBuilder(),
            new MinMax(1, 3), // realmsRange
            new MinMax(0, 0), // authzRange
            new MinMax(1, JwtRealmSettings.SUPPORTED_SIGNATURE_ALGORITHMS.size()), // algsRange
            new MinMax(1, 3), // audiencesRange
            new MinMax(1, 3), // usersRange
            new MinMax(0, 3), // rolesRange
            new MinMax(0, 1), // jwtCacheSizeRange
            randomBoolean() // createHttpsServer
        );
        final JwtIssuerAndRealm jwtIssuerAndRealm = this.randomJwtIssuerRealmPair();
        assertThat(jwtIssuerAndRealm.realm().delegatedAuthorizationSupport.hasDelegation(), is(false));

        final User user = this.randomUser(jwtIssuerAndRealm.issuer());
        final SecureString jwt = this.randomJwt(jwtIssuerAndRealm, user);
        final SecureString clientSecret = jwtIssuerAndRealm.realm().clientAuthenticationSharedSecret;
        final MinMax jwtAuthcRange = new MinMax(2, 3);
        this.doMultipleAuthcAuthzAndVerifySuccess(jwtIssuerAndRealm.realm(), user, jwt, clientSecret, jwtAuthcRange);
    }

    /**
     * Test with authz realms.
     * @throws Exception Unexpected test failure
     */
    public void testJwtAuthcRealmAuthcAuthzWithAuthzRealms() throws Exception {
        this.jwtIssuerAndRealms = this.generateJwtIssuerRealmPairs(
            this.createJwtRealmsSettingsBuilder(),
            new MinMax(1, 3), // realmsRange
            new MinMax(1, 3), // authzRange
            new MinMax(1, JwtRealmSettings.SUPPORTED_SIGNATURE_ALGORITHMS.size()), // algsRange
            new MinMax(1, 3), // audiencesRange
            new MinMax(1, 3), // usersRange
            new MinMax(0, 3), // rolesRange
            new MinMax(0, 1), // jwtCacheSizeRange
            randomBoolean() // createHttpsServer
        );
        final JwtIssuerAndRealm jwtIssuerAndRealm = this.randomJwtIssuerRealmPair();
        assertThat(jwtIssuerAndRealm.realm().delegatedAuthorizationSupport.hasDelegation(), is(true));

        final User user = this.randomUser(jwtIssuerAndRealm.issuer());
        final SecureString jwt = this.randomJwt(jwtIssuerAndRealm, user);
        final SecureString clientSecret = jwtIssuerAndRealm.realm().clientAuthenticationSharedSecret;
        final MinMax jwtAuthcRange = new MinMax(2, 3);
        this.doMultipleAuthcAuthzAndVerifySuccess(jwtIssuerAndRealm.realm(), user, jwt, clientSecret, jwtAuthcRange);

        // After the above success path test, do a negative path test for an authc user that does not exist in any authz realm.
        // In other words, above the `user` was found in an authz realm, but below `otherUser` will not be found in any authz realm.
        {
            final String otherUsername = randomValueOtherThanMany(
                candidate -> jwtIssuerAndRealm.issuer().principals.containsKey(candidate),
                () -> randomAlphaOfLengthBetween(4, 12)
            );
            final User otherUser = new User(otherUsername);
            final SecureString otherJwt = this.randomJwt(jwtIssuerAndRealm, otherUser);

            final JwtAuthenticationToken otherToken = new JwtAuthenticationToken(
                List.of(jwtIssuerAndRealm.realm().claimParserPrincipal.getClaimName()),
                otherJwt,
                clientSecret
            );
            final PlainActionFuture<AuthenticationResult<User>> otherFuture = new PlainActionFuture<>();
            jwtIssuerAndRealm.realm().authenticate(otherToken, otherFuture);
            final AuthenticationResult<User> otherResult = otherFuture.actionGet();
            assertThat(otherResult.isAuthenticated(), is(false));
            assertThat(otherResult.getException(), nullValue());
            assertThat(
                otherResult.getMessage(),
                containsString("[" + otherUsername + "] was authenticated, but no user could be found in realms [")
            );
        }
    }

    /**
     * Verify that a JWT realm successfully connects to HTTPS server, and can handle an HTTP 404 Not Found response correctly.
     * @throws Exception Unexpected test failure
     */
    public void testPkcJwkSetUrlNotFound() throws Exception {
        final JwtRealmsService jwtRealmsService = this.generateJwtRealmsService(this.createJwtRealmsSettingsBuilder());
        final String principalClaimName = randomFrom(jwtRealmsService.getPrincipalClaimNames());

        final List<Realm> allRealms = new ArrayList<>(); // authc and authz realms
        final boolean createHttpsServer = true; // force issuer to create HTTPS server for its PKC JWKSet
        final JwtIssuer jwtIssuer = this.createJwtIssuer(0, principalClaimName, 12, 1, 1, 1, createHttpsServer);
        assertThat(jwtIssuer.httpsServer, is(notNullValue()));
        try {
            final JwtRealmSettingsBuilder jwtRealmSettingsBuilder = this.createJwtRealmSettingsBuilder(jwtIssuer, 0, 0);
            final String configKey = RealmSettings.getFullSettingKey(jwtRealmSettingsBuilder.name(), JwtRealmSettings.PKC_JWKSET_PATH);
            final String configValue = jwtIssuer.httpsServer.url.replace("/valid/", "/invalid"); // right host, wrong path
            jwtRealmSettingsBuilder.settingsBuilder().put(configKey, configValue);
            final Exception exception = expectThrows(
                SettingsException.class,
                () -> this.createJwtRealm(allRealms, jwtRealmsService, jwtIssuer, jwtRealmSettingsBuilder)
            );
            assertThat(exception.getMessage(), equalTo("Can't get contents for setting [" + configKey + "] value [" + configValue + "]."));
            assertThat(exception.getCause().getMessage(), equalTo("Get [" + configValue + "] failed, status [404], reason [Not Found]."));
        } finally {
            jwtIssuer.close();
        }
    }

    /**
     * Test token parse failures and authentication failures.
     * @throws Exception Unexpected test failure
     */
    public void testJwtValidationFailures() throws Exception {
        this.jwtIssuerAndRealms = this.generateJwtIssuerRealmPairs(
            this.createJwtRealmsSettingsBuilder(),
            new MinMax(1, 1), // realmsRange
            new MinMax(0, 0), // authzRange
            new MinMax(1, JwtRealmSettings.SUPPORTED_SIGNATURE_ALGORITHMS.size()), // algsRange
            new MinMax(1, 1), // audiencesRange
            new MinMax(1, 1), // usersRange
            new MinMax(1, 1), // rolesRange
            new MinMax(0, 1), // jwtCacheSizeRange
            randomBoolean() // createHttpsServer
        );
        final JwtIssuerAndRealm jwtIssuerAndRealm = this.randomJwtIssuerRealmPair();
        final User user = this.randomUser(jwtIssuerAndRealm.issuer());
        final SecureString jwt = this.randomJwt(jwtIssuerAndRealm, user);
        final SecureString clientSecret = jwtIssuerAndRealm.realm().clientAuthenticationSharedSecret;
        final MinMax jwtAuthcRange = new MinMax(2, 3);

        // Indirectly verify authentication works before performing any failure scenarios
        this.doMultipleAuthcAuthzAndVerifySuccess(jwtIssuerAndRealm.realm(), user, jwt, clientSecret, jwtAuthcRange);

        // The above confirmed JWT realm authc/authz is working.
        // Now perform negative path tests to confirm JWT validation rejects invalid JWTs for different scenarios.

        {   // Do one more direct SUCCESS scenario by checking token() and authenticate() directly before moving on to FAILURE scenarios.
            final ThreadContext requestThreadContext = super.createThreadContext(jwt, clientSecret);
            final JwtAuthenticationToken token = (JwtAuthenticationToken) jwtIssuerAndRealm.realm().token(requestThreadContext);
            final PlainActionFuture<AuthenticationResult<User>> plainActionFuture = PlainActionFuture.newFuture();
            jwtIssuerAndRealm.realm().authenticate(token, plainActionFuture);
            assertThat(plainActionFuture.get(), is(notNullValue()));
            assertThat(plainActionFuture.get().isAuthenticated(), is(true));
        }

        // Directly verify FAILURE scenarios for token() parsing failures and authenticate() validation failures.

        // Null JWT
        final ThreadContext tc1 = super.createThreadContext(null, clientSecret);
        assertThat(jwtIssuerAndRealm.realm().token(tc1), nullValue());

        // Empty JWT string
        final ThreadContext tc2 = super.createThreadContext("", clientSecret);
        final Exception e2 = expectThrows(IllegalArgumentException.class, () -> jwtIssuerAndRealm.realm().token(tc2));
        assertThat(e2.getMessage(), equalTo("JWT bearer token must be non-empty"));

        // Non-empty whitespace JWT string
        final ThreadContext tc3 = super.createThreadContext("", clientSecret);
        final Exception e3 = expectThrows(IllegalArgumentException.class, () -> jwtIssuerAndRealm.realm().token(tc3));
        assertThat(e3.getMessage(), equalTo("JWT bearer token must be non-empty"));

        // Blank client secret
        final ThreadContext tc4 = super.createThreadContext(jwt, "");
        final Exception e4 = expectThrows(IllegalArgumentException.class, () -> jwtIssuerAndRealm.realm().token(tc4));
        assertThat(e4.getMessage(), equalTo("Client shared secret must be non-empty"));

        // Non-empty whitespace JWT client secret
        final ThreadContext tc5 = super.createThreadContext(jwt, " ");
        final Exception e5 = expectThrows(IllegalArgumentException.class, () -> jwtIssuerAndRealm.realm().token(tc5));
        assertThat(e5.getMessage(), equalTo("Client shared secret must be non-empty"));

        // JWT parse exception
        final ThreadContext tc6 = super.createThreadContext("Head.Body.Sig", clientSecret);
        final Exception e6 = expectThrows(IllegalArgumentException.class, () -> jwtIssuerAndRealm.realm().token(tc6));
        assertThat(e6.getMessage(), equalTo("Failed to parse JWT bearer token"));

        // Parse JWT into three parts, for rejecting testing of tampered JWT contents
        final SignedJWT parsedJwt = SignedJWT.parse(jwt.toString());
        final JWSHeader validHeader = parsedJwt.getHeader();
        final JWTClaimsSet validClaimsSet = parsedJwt.getJWTClaimsSet();
        final Base64URL validSignature = parsedJwt.getSignature();

        {   // Verify rejection of unsigned JWT
            final SecureString unsignedJwt = new SecureString(new PlainJWT(validClaimsSet).serialize().toCharArray());
            final ThreadContext tc = super.createThreadContext(unsignedJwt, clientSecret);
            expectThrows(IllegalArgumentException.class, () -> jwtIssuerAndRealm.realm().token(tc));
        }

        {   // Verify rejection of a tampered header (flip HMAC=>RSA or RSA/EC=>HMAC)
            final String mixupAlg; // Check if there are any algorithms available in the realm for attempting a flip test
            if (JwtRealmSettings.SUPPORTED_SIGNATURE_ALGORITHMS_HMAC.contains(validHeader.getAlgorithm().getName())) {
                if (jwtIssuerAndRealm.realm().jwksAlgsPkc.algs().isEmpty()) {
                    mixupAlg = null; // cannot flip HMAC to PKC (no PKC algs available)
                } else {
                    mixupAlg = randomFrom(jwtIssuerAndRealm.realm().jwksAlgsPkc.algs()); // flip HMAC to PKC
                }
            } else {
                if (jwtIssuerAndRealm.realm().jwksAlgsHmac.algs().isEmpty()) {
                    mixupAlg = null; // cannot flip PKC to HMAC (no HMAC algs available)
                } else {
                    mixupAlg = randomFrom(jwtIssuerAndRealm.realm().jwksAlgsHmac.algs()); // flip HMAC to PKC
                }
            }
            // This check can only be executed if there is a flip algorithm available in the realm
            if (Strings.hasText(mixupAlg)) {
                final JWSHeader tamperedHeader = new JWSHeader.Builder(JWSAlgorithm.parse(mixupAlg)).build();
                final SecureString jwtTamperedHeader = JwtValidateUtil.buildJwt(tamperedHeader, validClaimsSet, validSignature);
                this.verifyAuthenticateFailureHelper(jwtIssuerAndRealm, jwtTamperedHeader, clientSecret);
            }
        }

        {   // Verify rejection of a tampered claim set
            final JWTClaimsSet tamperedClaimsSet = new JWTClaimsSet.Builder(validClaimsSet).claim("gr0up", "superuser").build();
            final SecureString jwtTamperedClaimsSet = JwtValidateUtil.buildJwt(validHeader, tamperedClaimsSet, validSignature);
            this.verifyAuthenticateFailureHelper(jwtIssuerAndRealm, jwtTamperedClaimsSet, clientSecret);
        }

        {   // Verify rejection of a tampered signature
            final SecureString jwtWithTruncatedSignature = new SecureString(jwt.toString().substring(0, jwt.length() - 1).toCharArray());
            this.verifyAuthenticateFailureHelper(jwtIssuerAndRealm, jwtWithTruncatedSignature, clientSecret);
        }

        // Get read to re-sign JWTs for time claim failure tests
        final JwtIssuer.AlgJwkPair algJwkPair = randomFrom(jwtIssuerAndRealm.issuer().algAndJwksAll);
        final JWSHeader jwtHeader = new JWSHeader.Builder(JWSAlgorithm.parse(algJwkPair.alg())).build();
        final Instant now = Instant.now();
        final Date past = Date.from(now.minusSeconds(86400));
        final Date future = Date.from(now.plusSeconds(86400));

        {   // Verify rejection of JWT auth_time > now
            final JWTClaimsSet claimsSet = new JWTClaimsSet.Builder(validClaimsSet).claim("auth_time", future).build();
            final SecureString jwtIatFuture = JwtValidateUtil.signJwt(algJwkPair.jwk(), new SignedJWT(jwtHeader, claimsSet));
            this.verifyAuthenticateFailureHelper(jwtIssuerAndRealm, jwtIatFuture, clientSecret);
        }

        {   // Verify rejection of JWT iat > now
            final JWTClaimsSet claimsSet = new JWTClaimsSet.Builder(validClaimsSet).issueTime(future).build();
            final SecureString jwtIatFuture = JwtValidateUtil.signJwt(algJwkPair.jwk(), new SignedJWT(jwtHeader, claimsSet));
            this.verifyAuthenticateFailureHelper(jwtIssuerAndRealm, jwtIatFuture, clientSecret);
        }

        {   // Verify rejection of JWT nbf > now
            final JWTClaimsSet claimsSet = new JWTClaimsSet.Builder(validClaimsSet).notBeforeTime(future).build();
            final SecureString jwtIatFuture = JwtValidateUtil.signJwt(algJwkPair.jwk(), new SignedJWT(jwtHeader, claimsSet));
            this.verifyAuthenticateFailureHelper(jwtIssuerAndRealm, jwtIatFuture, clientSecret);
        }

        {   // Verify rejection of JWT now > exp
            final JWTClaimsSet claimsSet = new JWTClaimsSet.Builder(validClaimsSet).expirationTime(past).build();
            final SecureString jwtExpPast = JwtValidateUtil.signJwt(algJwkPair.jwk(), new SignedJWT(jwtHeader, claimsSet));
            this.verifyAuthenticateFailureHelper(jwtIssuerAndRealm, jwtExpPast, clientSecret);
        }
    }

    /**
     * Configure two realms for same issuer. Use identical realm config, except different client secrets.
     * Generate a JWT which is valid for both realms, but verify authentication only succeeds for second realm with correct client secret.
     * @throws Exception Unexpected test failure
     */
    public void testSameIssuerTwoRealmsDifferentClientSecrets() throws Exception {
        final JwtRealmsService jwtRealmsService = this.generateJwtRealmsService(this.createJwtRealmsSettingsBuilder());
        final String principalClaimName = randomFrom(jwtRealmsService.getPrincipalClaimNames());

        final int realmsCount = 2;
        final List<Realm> allRealms = new ArrayList<>(realmsCount); // two identical realms for same issuer, except different client secret
        final JwtIssuer jwtIssuer = this.createJwtIssuer(0, principalClaimName, 12, 1, 1, 1, false);
        this.jwtIssuerAndRealms = new ArrayList<>(realmsCount);
        for (int i = 0; i < realmsCount; i++) {
            final String realmName = "realm_" + jwtIssuer.issuerClaimValue + "_" + i;
            final String clientSecret = "clientSecret_" + jwtIssuer.issuerClaimValue + "_" + i;

            final Settings.Builder authcSettings = Settings.builder()
                .put(this.globalSettings)
                .put(RealmSettings.getFullSettingKey(realmName, JwtRealmSettings.ALLOWED_ISSUER), jwtIssuer.issuerClaimValue)
                .put(
                    RealmSettings.getFullSettingKey(realmName, JwtRealmSettings.ALLOWED_SIGNATURE_ALGORITHMS),
                    String.join(",", jwtIssuer.algorithmsAll)
                )
                .put(RealmSettings.getFullSettingKey(realmName, JwtRealmSettings.ALLOWED_AUDIENCES), jwtIssuer.audiencesClaimValue.get(0))
                .put(RealmSettings.getFullSettingKey(realmName, JwtRealmSettings.CLAIMS_PRINCIPAL.getClaim()), principalClaimName)
                .put(
                    RealmSettings.getFullSettingKey(realmName, JwtRealmSettings.CLIENT_AUTHENTICATION_TYPE),
                    JwtRealmSettings.ClientAuthenticationType.SHARED_SECRET.value()
                );
            if (Strings.hasText(jwtIssuer.encodedJwkSetPkcPublic)) {
                authcSettings.put(
                    RealmSettings.getFullSettingKey(realmName, JwtRealmSettings.PKC_JWKSET_PATH),
                    super.saveToTempFile("jwkset.", ".json", jwtIssuer.encodedJwkSetPkcPublic.getBytes(StandardCharsets.UTF_8))
                );
            }
            // JWT authc realm secure settings
            final MockSecureSettings secureSettings = new MockSecureSettings();
            if (Strings.hasText(jwtIssuer.encodedJwkSetHmac)) {
                secureSettings.setString(
                    RealmSettings.getFullSettingKey(realmName, JwtRealmSettings.HMAC_JWKSET),
                    jwtIssuer.encodedJwkSetHmac
                );
            }
            if (Strings.hasText(jwtIssuer.encodedKeyHmacOidc)) {
                secureSettings.setString(
                    RealmSettings.getFullSettingKey(realmName, JwtRealmSettings.HMAC_KEY),
                    jwtIssuer.encodedKeyHmacOidc
                );
            }
            secureSettings.setString(
                RealmSettings.getFullSettingKey(realmName, JwtRealmSettings.CLIENT_AUTHENTICATION_SHARED_SECRET),
                clientSecret
            );
            authcSettings.setSecureSettings(secureSettings);
            final JwtRealmSettingsBuilder jwtRealmSettingsBuilder = new JwtRealmSettingsBuilder(realmName, authcSettings);
            final JwtRealm jwtRealm = this.createJwtRealm(allRealms, jwtRealmsService, jwtIssuer, jwtRealmSettingsBuilder);
            jwtRealm.initialize(allRealms, super.licenseState);
            final JwtIssuerAndRealm jwtIssuerAndRealm = new JwtIssuerAndRealm(jwtIssuer, jwtRealm, jwtRealmSettingsBuilder);
            this.jwtIssuerAndRealms.add(jwtIssuerAndRealm); // add them so the test will clean them up
        }

        // pick 2nd realm and use its secret, verify 2nd realm does authc, which implies 1st realm rejects the secret
        final JwtIssuerAndRealm jwtIssuerAndRealm = this.jwtIssuerAndRealms.get(1);
        final User user = this.randomUser(jwtIssuerAndRealm.issuer());
        final SecureString jwt = this.randomJwt(jwtIssuerAndRealm, user);
        final SecureString clientSecret = jwtIssuerAndRealm.realm().clientAuthenticationSharedSecret;
        final MinMax jwtAuthcRange = new MinMax(2, 3);
        this.doMultipleAuthcAuthzAndVerifySuccess(jwtIssuerAndRealm.realm(), user, jwt, clientSecret, jwtAuthcRange);
    }
}
