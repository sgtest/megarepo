/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.authc.ldap;

import com.unboundid.ldap.sdk.LDAPConnection;
import com.unboundid.ldap.sdk.LDAPConnectionPool;
import com.unboundid.ldap.sdk.LDAPException;
import com.unboundid.ldap.sdk.LDAPInterface;

import org.elasticsearch.ExceptionsHelper;
import org.elasticsearch.common.Booleans;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.TestEnvironment;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.core.security.authc.ldap.ActiveDirectorySessionFactorySettings;
import org.elasticsearch.xpack.core.security.authc.ldap.support.LdapSearchScope;
import org.elasticsearch.xpack.core.security.authc.ldap.support.SessionFactorySettings;
import org.elasticsearch.xpack.core.ssl.SSLService;
import org.elasticsearch.xpack.core.ssl.VerificationMode;
import org.junit.Before;

import java.io.IOException;
import java.nio.file.FileVisitResult;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.SimpleFileVisitor;
import java.nio.file.attribute.BasicFileAttributes;
import java.security.AccessController;
import java.security.PrivilegedAction;
import java.util.ArrayList;
import java.util.List;

public abstract class AbstractActiveDirectoryTestCase extends ESTestCase {

    // follow referrals defaults to false here which differs from the default value of the setting
    // this is needed to prevent test logs being filled by errors as the default configuration of
    // the tests run against a vagrant samba4 instance configured as a domain controller with the
    // ports mapped into the ephemeral port range and there is the possibility of incorrect results
    // as we cannot control the URL of the referral which may contain a non-resolvable DNS name as
    // this name would be served by the samba4 instance
    public static final Boolean FOLLOW_REFERRALS = Booleans.parseBoolean(getFromEnv("TESTS_AD_FOLLOW_REFERRALS", "false"));
    public static final String AD_LDAP_URL = getFromEnv("TESTS_AD_LDAP_URL", "ldaps://localhost:61636");
    public static final String AD_LDAP_GC_URL = getFromEnv("TESTS_AD_LDAP_GC_URL", "ldaps://localhost:63269");
    public static final String PASSWORD = getFromEnv("TESTS_AD_USER_PASSWORD", "Passw0rd");
    public static final String AD_LDAP_PORT = getFromEnv("TESTS_AD_LDAP_PORT", "61389");
    public static final String AD_LDAPS_PORT = getFromEnv("TESTS_AD_LDAPS_PORT", "61636");
    public static final String AD_GC_LDAP_PORT = getFromEnv("TESTS_AD_GC_LDAP_PORT", "63268");
    public static final String AD_GC_LDAPS_PORT = getFromEnv("TESTS_AD_GC_LDAPS_PORT", "63269");
    public static final String AD_DOMAIN = "ad.test.elasticsearch.com";

    protected SSLService sslService;
    protected Settings globalSettings;
    protected boolean useGlobalSSL;
    protected List<String> certificatePaths;

    @Before
    public void initializeSslSocketFactory() throws Exception {
        useGlobalSSL = randomBoolean();
        // We use certificates in PEM format and `ssl.certificate_authorities` instead of ssl.trustore
        // so that these tests can also run in a FIPS JVM where JKS keystores can't be used.
        certificatePaths = new ArrayList<>();
        Files.walkFileTree(getDataPath
            ("../ldap/support"), new SimpleFileVisitor<Path>() {
            @Override
            public FileVisitResult visitFile(Path file, BasicFileAttributes attrs) throws IOException {
                String fileName = file.getFileName().toString();
                if (fileName.endsWith(".crt")) {
                    certificatePaths.add(getDataPath("../ldap/support/" + fileName).toString());
                }
                return FileVisitResult.CONTINUE;
            }
        });
        /*
         * Prior to each test we reinitialize the socket factory with a new SSLService so that we get a new SSLContext.
         * If we re-use a SSLContext, previously connected sessions can get re-established which breaks hostname
         * verification tests since a re-established connection does not perform hostname verification.
         */
        Settings.Builder builder = Settings.builder().put("path.home", createTempDir());
        if (useGlobalSSL) {
            builder.putList("xpack.ssl.certificate_authorities", certificatePaths);

            // fake realm to load config with certificate verification mode
            builder.putList("xpack.security.authc.realms.bar.ssl.certificate_authorities", certificatePaths);
            builder.put("xpack.security.authc.realms.bar.ssl.verification_mode", VerificationMode.CERTIFICATE);
        } else {
            // fake realms so ssl will get loaded
            builder.putList("xpack.security.authc.realms.foo.ssl.certificate_authorities", certificatePaths);
            builder.put("xpack.security.authc.realms.foo.ssl.verification_mode", VerificationMode.FULL);
            builder.putList("xpack.security.authc.realms.bar.ssl.certificate_authorities", certificatePaths);
            builder.put("xpack.security.authc.realms.bar.ssl.verification_mode", VerificationMode.CERTIFICATE);
        }
        globalSettings = builder.build();
        Environment environment = TestEnvironment.newEnvironment(globalSettings);
        sslService = new SSLService(globalSettings, environment);
    }

    Settings buildAdSettings(String ldapUrl, String adDomainName, String userSearchDN, LdapSearchScope scope,
                                           boolean hostnameVerification) {
        Settings.Builder builder = Settings.builder()
                .putList(SessionFactorySettings.URLS_SETTING, ldapUrl)
                .put(ActiveDirectorySessionFactorySettings.AD_DOMAIN_NAME_SETTING, adDomainName)
                .put(ActiveDirectorySessionFactorySettings.AD_USER_SEARCH_BASEDN_SETTING, userSearchDN)
                .put(ActiveDirectorySessionFactorySettings.AD_USER_SEARCH_SCOPE_SETTING, scope)
                .put(ActiveDirectorySessionFactorySettings.AD_LDAP_PORT_SETTING.getKey(), AD_LDAP_PORT)
                .put(ActiveDirectorySessionFactorySettings.AD_LDAPS_PORT_SETTING.getKey(), AD_LDAPS_PORT)
                .put(ActiveDirectorySessionFactorySettings.AD_GC_LDAP_PORT_SETTING.getKey(), AD_GC_LDAP_PORT)
                .put(ActiveDirectorySessionFactorySettings.AD_GC_LDAPS_PORT_SETTING.getKey(), AD_GC_LDAPS_PORT)
                .put("follow_referrals", FOLLOW_REFERRALS);
        if (randomBoolean()) {
            builder.put("ssl.verification_mode", hostnameVerification ? VerificationMode.FULL : VerificationMode.CERTIFICATE);
        } else {
            builder.put(SessionFactorySettings.HOSTNAME_VERIFICATION_SETTING, hostnameVerification);
        }
        if (useGlobalSSL == false) {
            builder.putList("ssl.certificate_authorities", certificatePaths);
        }
        return builder.build();
    }

    protected static void assertConnectionCanReconnect(LDAPInterface conn) {
        AccessController.doPrivileged(new PrivilegedAction<Void>() {
            @Override
            public Void run() {
                try {
                    if (conn instanceof LDAPConnection) {
                        ((LDAPConnection) conn).reconnect();
                    } else if (conn instanceof LDAPConnectionPool) {
                        try (LDAPConnection c = ((LDAPConnectionPool) conn).getConnection()) {
                            c.reconnect();
                        }
                    }
                } catch (LDAPException e) {
                    fail("Connection is not valid. It will not work on follow referral flow." +
                            System.lineSeparator() + ExceptionsHelper.stackTrace(e));
                }
                return null;
            }
        });
    }

    private static String getFromEnv(String envVar, String defaultValue) {
        final String value = System.getenv(envVar);
        return value == null ? defaultValue : value;
    }
}
