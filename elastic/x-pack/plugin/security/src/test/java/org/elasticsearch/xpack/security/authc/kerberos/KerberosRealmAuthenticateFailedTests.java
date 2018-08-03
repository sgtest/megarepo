/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.security.authc.kerberos;

import org.elasticsearch.ElasticsearchSecurityException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.xpack.core.security.authc.AuthenticationResult;
import org.elasticsearch.xpack.core.security.authc.kerberos.KerberosRealmSettings;
import org.elasticsearch.xpack.core.security.authc.support.UsernamePasswordToken;
import org.elasticsearch.protocol.xpack.security.User;
import org.ietf.jgss.GSSException;

import java.nio.file.Path;
import java.util.List;

import javax.security.auth.login.LoginException;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.notNullValue;
import static org.mockito.AdditionalMatchers.aryEq;
import static org.mockito.Matchers.any;
import static org.mockito.Matchers.eq;
import static org.mockito.Mockito.verify;

public class KerberosRealmAuthenticateFailedTests extends KerberosRealmTestCase {

    public void testAuthenticateWithNonKerberosAuthenticationToken() {
        final KerberosRealm kerberosRealm = createKerberosRealm(randomAlphaOfLength(5));

        final UsernamePasswordToken usernamePasswordToken =
                new UsernamePasswordToken(randomAlphaOfLength(5), new SecureString(new char[] { 'a', 'b', 'c' }));
        expectThrows(AssertionError.class, () -> kerberosRealm.authenticate(usernamePasswordToken, PlainActionFuture.newFuture()));
    }

    public void testAuthenticateDifferentFailureScenarios() throws LoginException, GSSException {
        final String username = randomPrincipalName();
        final String outToken = randomAlphaOfLength(10);
        final KerberosRealm kerberosRealm = createKerberosRealm(username);
        final boolean validTicket = rarely();
        final boolean throwExceptionForInvalidTicket = validTicket ? false : randomBoolean();
        final boolean throwLoginException = randomBoolean();
        final byte[] decodedTicket = randomByteArrayOfLength(5);
        final Path keytabPath = config.env().configFile().resolve(KerberosRealmSettings.HTTP_SERVICE_KEYTAB_PATH.get(config.settings()));
        final boolean krbDebug = KerberosRealmSettings.SETTING_KRB_DEBUG_ENABLE.get(config.settings());
        if (validTicket) {
            mockKerberosTicketValidator(decodedTicket, keytabPath, krbDebug, new Tuple<>(username, outToken), null);
        } else {
            if (throwExceptionForInvalidTicket) {
                if (throwLoginException) {
                    mockKerberosTicketValidator(decodedTicket, keytabPath, krbDebug, null, new LoginException("Login Exception"));
                } else {
                    mockKerberosTicketValidator(decodedTicket, keytabPath, krbDebug, null, new GSSException(GSSException.FAILURE));
                }
            } else {
                mockKerberosTicketValidator(decodedTicket, keytabPath, krbDebug, new Tuple<>(null, outToken), null);
            }
        }
        final boolean nullKerberosAuthnToken = rarely();
        final KerberosAuthenticationToken kerberosAuthenticationToken =
                nullKerberosAuthnToken ? null : new KerberosAuthenticationToken(decodedTicket);
        if (nullKerberosAuthnToken) {
            expectThrows(AssertionError.class,
                    () -> kerberosRealm.authenticate(kerberosAuthenticationToken, PlainActionFuture.newFuture()));
        } else {
            final PlainActionFuture<AuthenticationResult> future = new PlainActionFuture<>();
            kerberosRealm.authenticate(kerberosAuthenticationToken, future);
            AuthenticationResult result = future.actionGet();
            assertThat(result, is(notNullValue()));
            if (validTicket) {
                final String expectedUsername = maybeRemoveRealmName(username);
                final User expectedUser = new User(expectedUsername, roles.toArray(new String[roles.size()]), null, null, null, true);
                assertSuccessAuthenticationResult(expectedUser, outToken, result);
            } else {
                assertThat(result.getStatus(), is(equalTo(AuthenticationResult.Status.TERMINATE)));
                if (throwExceptionForInvalidTicket == false) {
                    assertThat(result.getException(), is(instanceOf(ElasticsearchSecurityException.class)));
                    final List<String> wwwAuthnHeader = ((ElasticsearchSecurityException) result.getException())
                            .getHeader(KerberosAuthenticationToken.WWW_AUTHENTICATE);
                    assertThat(wwwAuthnHeader, is(notNullValue()));
                    assertThat(wwwAuthnHeader.get(0), is(equalTo(KerberosAuthenticationToken.NEGOTIATE_AUTH_HEADER_PREFIX + outToken)));
                    assertThat(result.getMessage(), is(equalTo("failed to authenticate user, gss context negotiation not complete")));
                } else {
                    if (throwLoginException) {
                        assertThat(result.getMessage(), is(equalTo("failed to authenticate user, service login failure")));
                    } else {
                        assertThat(result.getMessage(), is(equalTo("failed to authenticate user, gss context negotiation failure")));
                    }
                    assertThat(result.getException(), is(instanceOf(ElasticsearchSecurityException.class)));
                    final List<String> wwwAuthnHeader = ((ElasticsearchSecurityException) result.getException())
                            .getHeader(KerberosAuthenticationToken.WWW_AUTHENTICATE);
                    assertThat(wwwAuthnHeader, is(notNullValue()));
                    assertThat(wwwAuthnHeader.get(0), is(equalTo(KerberosAuthenticationToken.NEGOTIATE_SCHEME_NAME)));
                }
            }
            verify(mockKerberosTicketValidator).validateTicket(aryEq(decodedTicket), eq(keytabPath), eq(krbDebug),
                    any(ActionListener.class));
        }
    }
}
