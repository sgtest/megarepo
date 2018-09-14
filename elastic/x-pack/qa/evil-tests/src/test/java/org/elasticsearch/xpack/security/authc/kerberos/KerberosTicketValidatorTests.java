/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.security.authc.kerberos;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.common.util.concurrent.UncategorizedExecutionException;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.TestEnvironment;
import org.elasticsearch.xpack.core.security.authc.kerberos.KerberosRealmSettings;
import org.ietf.jgss.GSSException;

import java.io.IOException;
import java.nio.file.Path;
import java.security.PrivilegedActionException;
import java.util.Base64;
import java.util.concurrent.ExecutionException;

import javax.security.auth.login.LoginException;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;

public class KerberosTicketValidatorTests extends KerberosTestCase {

    private KerberosTicketValidator kerberosTicketValidator = new KerberosTicketValidator();

    public void testKerbTicketGeneratedForDifferentServerFailsValidation() throws Exception {
        createPrincipalKeyTab(workDir, "differentServer");

        // Client login and init token preparation
        final String clientUserName = randomFrom(clientUserNames);
        try (SpnegoClient spnegoClient =
                new SpnegoClient(principalName(clientUserName), new SecureString("pwd".toCharArray()), principalName("differentServer"));) {
            final String base64KerbToken = spnegoClient.getBase64EncodedTokenForSpnegoHeader();
            assertThat(base64KerbToken, is(notNullValue()));

            final Environment env = TestEnvironment.newEnvironment(globalSettings);
            final Path keytabPath = env.configFile().resolve(KerberosRealmSettings.HTTP_SERVICE_KEYTAB_PATH.get(settings));
            final PlainActionFuture<Tuple<String, String>> future = new PlainActionFuture<>();
            kerberosTicketValidator.validateTicket(Base64.getDecoder().decode(base64KerbToken), keytabPath, true, future);
            final GSSException gssException = expectThrows(GSSException.class, () -> unwrapExpectedExceptionFromFutureAndThrow(future));
            assertThat(gssException.getMajor(), equalTo(GSSException.FAILURE));
        }
    }

    public void testInvalidKerbTicketFailsValidation() throws Exception {
        final String base64KerbToken = Base64.getEncoder().encodeToString(randomByteArrayOfLength(5));

        final Environment env = TestEnvironment.newEnvironment(globalSettings);
        final Path keytabPath = env.configFile().resolve(KerberosRealmSettings.HTTP_SERVICE_KEYTAB_PATH.get(settings));
        kerberosTicketValidator.validateTicket(Base64.getDecoder().decode(base64KerbToken), keytabPath, true,
                new ActionListener<Tuple<String, String>>() {
                    boolean exceptionHandled = false;

                    @Override
                    public void onResponse(Tuple<String, String> response) {
                        fail("expected exception to be thrown of type GSSException");
                    }

                    @Override
                    public void onFailure(Exception e) {
                        assertThat(exceptionHandled, is(false));
                        assertThat(e, instanceOf(GSSException.class));
                        assertThat(((GSSException) e).getMajor(), equalTo(GSSException.DEFECTIVE_TOKEN));
                        exceptionHandled = true;
                    }
                });
    }

    public void testWhenKeyTabWithInvalidContentFailsValidation()
            throws LoginException, GSSException, IOException, PrivilegedActionException {
        // Client login and init token preparation
        final String clientUserName = randomFrom(clientUserNames);
        try (SpnegoClient spnegoClient = new SpnegoClient(principalName(clientUserName), new SecureString("pwd".toCharArray()),
                principalName(randomFrom(serviceUserNames)));) {
            final String base64KerbToken = spnegoClient.getBase64EncodedTokenForSpnegoHeader();
            assertThat(base64KerbToken, is(notNullValue()));

            final Path ktabPath = KerberosRealmTestCase.writeKeyTab(workDir.resolve("invalid.keytab"), "not - a - valid - key - tab");
            settings = KerberosRealmTestCase.buildKerberosRealmSettings(ktabPath.toString());
            final Environment env = TestEnvironment.newEnvironment(globalSettings);
            final Path keytabPath = env.configFile().resolve(KerberosRealmSettings.HTTP_SERVICE_KEYTAB_PATH.get(settings));
            final PlainActionFuture<Tuple<String, String>> future = new PlainActionFuture<>();
            kerberosTicketValidator.validateTicket(Base64.getDecoder().decode(base64KerbToken), keytabPath, true, future);
            final GSSException gssException = expectThrows(GSSException.class, () -> unwrapExpectedExceptionFromFutureAndThrow(future));
            assertThat(gssException.getMajor(), equalTo(GSSException.FAILURE));
        }
    }

    public void testValidKebrerosTicket() throws PrivilegedActionException, GSSException, LoginException {
        // Client login and init token preparation
        final String clientUserName = randomFrom(clientUserNames);
        try (SpnegoClient spnegoClient = new SpnegoClient(principalName(clientUserName), new SecureString("pwd".toCharArray()),
                principalName(randomFrom(serviceUserNames)));) {
            final String base64KerbToken = spnegoClient.getBase64EncodedTokenForSpnegoHeader();
            assertThat(base64KerbToken, is(notNullValue()));

            final Environment env = TestEnvironment.newEnvironment(globalSettings);
            final Path keytabPath = env.configFile().resolve(KerberosRealmSettings.HTTP_SERVICE_KEYTAB_PATH.get(settings));
            final PlainActionFuture<Tuple<String, String>> future = new PlainActionFuture<>();
            kerberosTicketValidator.validateTicket(Base64.getDecoder().decode(base64KerbToken), keytabPath, true, future);
            assertThat(future.actionGet(), is(notNullValue()));
            assertThat(future.actionGet().v1(), equalTo(principalName(clientUserName)));
            assertThat(future.actionGet().v2(), is(notNullValue()));

            final String outToken = spnegoClient.handleResponse(future.actionGet().v2());
            assertThat(outToken, is(nullValue()));
            assertThat(spnegoClient.isEstablished(), is(true));
        }
    }

    private void unwrapExpectedExceptionFromFutureAndThrow(PlainActionFuture<Tuple<String, String>> future) throws Throwable {
        try {
            future.actionGet();
        } catch (Throwable t) {
            Throwable throwThis = t;
            while (throwThis instanceof UncategorizedExecutionException || throwThis instanceof ExecutionException) {
                throwThis = throwThis.getCause();
            }
            throw throwThis;
        }
    }
}
