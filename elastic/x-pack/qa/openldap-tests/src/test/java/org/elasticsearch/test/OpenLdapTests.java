/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.test;

import com.unboundid.ldap.sdk.LDAPConnection;
import com.unboundid.ldap.sdk.LDAPException;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.common.settings.MockSecureSettings;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.util.concurrent.UncategorizedExecutionException;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.TestEnvironment;
import org.elasticsearch.threadpool.TestThreadPool;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.security.authc.RealmConfig;
import org.elasticsearch.xpack.core.security.authc.ldap.support.LdapSearchScope;
import org.elasticsearch.xpack.core.security.authc.ldap.support.SessionFactorySettings;
import org.elasticsearch.xpack.core.ssl.SSLService;
import org.elasticsearch.xpack.core.ssl.VerificationMode;
import org.elasticsearch.xpack.security.authc.ldap.LdapSessionFactory;
import org.elasticsearch.xpack.security.authc.ldap.LdapTestUtils;
import org.elasticsearch.xpack.security.authc.ldap.support.LdapMetaDataResolver;
import org.elasticsearch.xpack.security.authc.ldap.support.LdapSession;
import org.elasticsearch.xpack.security.authc.ldap.support.LdapTestCase;
import org.elasticsearch.xpack.security.authc.ldap.support.SessionFactory;
import org.junit.After;
import org.junit.Before;

import java.nio.file.Path;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ExecutionException;

import static org.hamcrest.Matchers.anyOf;
import static org.hamcrest.Matchers.contains;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasItem;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.startsWith;

public class OpenLdapTests extends ESTestCase {

    public static final String OPEN_LDAP_DNS_URL = "ldaps://localhost:60636";
    public static final String OPEN_LDAP_IP_URL = "ldaps://127.0.0.1:60636";

    public static final String PASSWORD = "NickFuryHeartsES";
    private static final String HAWKEYE_DN = "uid=hawkeye,ou=people,dc=oldap,dc=test,dc=elasticsearch,dc=com";
    public static final String LDAPTRUST_PATH = "/idptrust.jks";
    private static final SecureString PASSWORD_SECURE_STRING = new SecureString(PASSWORD.toCharArray());

    private boolean useGlobalSSL;
    private SSLService sslService;
    private ThreadPool threadPool;
    private Settings globalSettings;

    @Before
    public void init() throws Exception {
        threadPool = new TestThreadPool("OpenLdapTests thread pool");
    }

    @After
    public void shutdown() throws InterruptedException {
        terminate(threadPool);
    }

    @Override
    public boolean enableWarningsCheck() {
        return false;
    }

    @Before
    public void initializeSslSocketFactory() throws Exception {
        Path truststore = getDataPath(LDAPTRUST_PATH);
        /*
         * Prior to each test we reinitialize the socket factory with a new SSLService so that we get a new SSLContext.
         * If we re-use a SSLContext, previously connected sessions can get re-established which breaks hostname
         * verification tests since a re-established connection does not perform hostname verification.
         */
        useGlobalSSL = randomBoolean();
        MockSecureSettings mockSecureSettings = new MockSecureSettings();
        Settings.Builder builder = Settings.builder().put("path.home", createTempDir());
        if (useGlobalSSL) {
            builder.put("xpack.ssl.truststore.path", truststore);
            mockSecureSettings.setString("xpack.ssl.truststore.secure_password", "changeit");

            // fake realm to load config with certificate verification mode
            builder.put("xpack.security.authc.realms.bar.ssl.truststore.path", truststore);
            mockSecureSettings.setString("xpack.security.authc.realms.bar.ssl.truststore.secure_password", "changeit");
            builder.put("xpack.security.authc.realms.bar.ssl.verification_mode", VerificationMode.CERTIFICATE);
        } else {
            // fake realms so ssl will get loaded
            builder.put("xpack.security.authc.realms.foo.ssl.truststore.path", truststore);
            mockSecureSettings.setString("xpack.security.authc.realms.foo.ssl.truststore.secure_password", "changeit");
            builder.put("xpack.security.authc.realms.foo.ssl.verification_mode", VerificationMode.FULL);
            builder.put("xpack.security.authc.realms.bar.ssl.truststore.path", truststore);
            mockSecureSettings.setString("xpack.security.authc.realms.bar.ssl.truststore.secure_password", "changeit");
            builder.put("xpack.security.authc.realms.bar.ssl.verification_mode", VerificationMode.CERTIFICATE);
        }
        globalSettings = builder.setSecureSettings(mockSecureSettings).build();
        Environment environment = TestEnvironment.newEnvironment(globalSettings);
        sslService = new SSLService(globalSettings, environment);
    }

    public void testConnect() throws Exception {
        //openldap does not use cn as naming attributes by default
        String groupSearchBase = "ou=people,dc=oldap,dc=test,dc=elasticsearch,dc=com";
        String userTemplate = "uid={0},ou=people,dc=oldap,dc=test,dc=elasticsearch,dc=com";
        RealmConfig config = new RealmConfig("oldap-test", buildLdapSettings(OPEN_LDAP_DNS_URL, userTemplate, groupSearchBase,
                LdapSearchScope.ONE_LEVEL), globalSettings, TestEnvironment.newEnvironment(globalSettings),
                new ThreadContext(Settings.EMPTY));
        LdapSessionFactory sessionFactory = new LdapSessionFactory(config, sslService, threadPool);

        String[] users = new String[] { "blackwidow", "cap", "hawkeye", "hulk", "ironman", "thor" };
        for (String user : users) {
            logger.info("testing connect as user [{}]", user);
            try (LdapSession ldap = session(sessionFactory, user, PASSWORD_SECURE_STRING)) {
                assertThat(groups(ldap), hasItem(containsString("Avengers")));
            }
        }
    }

    public void testGroupSearchScopeBase() throws Exception {
        //base search on a groups means that the user can be in just one group

        String groupSearchBase = "cn=Avengers,ou=people,dc=oldap,dc=test,dc=elasticsearch,dc=com";
        String userTemplate = "uid={0},ou=people,dc=oldap,dc=test,dc=elasticsearch,dc=com";
        RealmConfig config = new RealmConfig("oldap-test", buildLdapSettings(OPEN_LDAP_DNS_URL, userTemplate, groupSearchBase,
                LdapSearchScope.BASE), globalSettings, TestEnvironment.newEnvironment(globalSettings), new ThreadContext(Settings.EMPTY));
        LdapSessionFactory sessionFactory = new LdapSessionFactory(config, sslService, threadPool);

        String[] users = new String[] { "blackwidow", "cap", "hawkeye", "hulk", "ironman", "thor" };
        for (String user : users) {
            try (LdapSession ldap = session(sessionFactory, user, PASSWORD_SECURE_STRING)) {
                assertThat(groups(ldap), hasItem(containsString("Avengers")));
            }
        }
    }

    public void testCustomFilter() throws Exception {
        String groupSearchBase = "ou=people,dc=oldap,dc=test,dc=elasticsearch,dc=com";
        String userTemplate = "uid={0},ou=people,dc=oldap,dc=test,dc=elasticsearch,dc=com";
        Settings settings = Settings.builder()
                .put(buildLdapSettings(OPEN_LDAP_DNS_URL, userTemplate, groupSearchBase, LdapSearchScope.ONE_LEVEL))
                .put("group_search.filter", "(&(objectclass=posixGroup)(memberUid={0}))")
                .put("group_search.user_attribute", "uid")
                .build();
        RealmConfig config = new RealmConfig("oldap-test", settings, globalSettings, TestEnvironment.newEnvironment(globalSettings),
                new ThreadContext(Settings.EMPTY));
        LdapSessionFactory sessionFactory = new LdapSessionFactory(config, sslService, threadPool);

        try (LdapSession ldap = session(sessionFactory, "selvig", PASSWORD_SECURE_STRING)) {
            assertThat(groups(ldap), hasItem(containsString("Geniuses")));
        }
    }

    @AwaitsFix(bugUrl = "https://github.com/elastic/x-plugins/issues/2849")
    public void testTcpTimeout() throws Exception {
        String groupSearchBase = "ou=people,dc=oldap,dc=test,dc=elasticsearch,dc=com";
        String userTemplate = "uid={0},ou=people,dc=oldap,dc=test,dc=elasticsearch,dc=com";
        Settings settings = Settings.builder()
                .put(buildLdapSettings(OPEN_LDAP_DNS_URL, userTemplate, groupSearchBase, LdapSearchScope.SUB_TREE))
                .put("group_search.filter", "(objectClass=*)")
                .put("ssl.verification_mode", VerificationMode.CERTIFICATE)
                .put(SessionFactorySettings.TIMEOUT_TCP_READ_SETTING, "1ms") //1 millisecond
                .build();
        RealmConfig config = new RealmConfig("oldap-test", settings, globalSettings, TestEnvironment.newEnvironment(globalSettings),
                new ThreadContext(Settings.EMPTY));
        LdapSessionFactory sessionFactory = new LdapSessionFactory(config, sslService, threadPool);

        LDAPException expected = expectThrows(LDAPException.class,
                () -> session(sessionFactory, "thor", PASSWORD_SECURE_STRING).groups(new PlainActionFuture<>()));
        assertThat(expected.getMessage(), containsString("A client-side timeout was encountered while waiting"));
    }

    public void testStandardLdapConnectionHostnameVerificationFailure() throws Exception {
        //openldap does not use cn as naming attributes by default
        String groupSearchBase = "ou=people,dc=oldap,dc=test,dc=elasticsearch,dc=com";
        String userTemplate = "uid={0},ou=people,dc=oldap,dc=test,dc=elasticsearch,dc=com";
        Settings settings = Settings.builder()
                // The certificate used in the vagrant box is valid for "localhost", but not for "127.0.0.1"
                .put(buildLdapSettings(OPEN_LDAP_IP_URL, userTemplate, groupSearchBase, LdapSearchScope.ONE_LEVEL))
                .put("ssl.verification_mode", VerificationMode.FULL)
                .build();

        RealmConfig config = new RealmConfig("oldap-test", settings, globalSettings, TestEnvironment.newEnvironment(globalSettings),
                new ThreadContext(Settings.EMPTY));
        LdapSessionFactory sessionFactory = new LdapSessionFactory(config, sslService, threadPool);

        String user = "blackwidow";
        UncategorizedExecutionException e = expectThrows(UncategorizedExecutionException.class,
                () -> session(sessionFactory, user, PASSWORD_SECURE_STRING));
        assertThat(e.getCause(), instanceOf(ExecutionException.class));
        assertThat(e.getCause().getCause(), instanceOf(LDAPException.class));
        assertThat(e.getCause().getCause().getMessage(),
                anyOf(containsString("Hostname verification failed"), containsString("peer not authenticated")));
    }

    public void testStandardLdapConnectionHostnameVerificationSuccess() throws Exception {
        //openldap does not use cn as naming attributes by default
        String groupSearchBase = "ou=people,dc=oldap,dc=test,dc=elasticsearch,dc=com";
        String userTemplate = "uid={0},ou=people,dc=oldap,dc=test,dc=elasticsearch,dc=com";
        Settings settings = Settings.builder()
                // The certificate used in the vagrant box is valid for "localhost" (but not for "127.0.0.1")
                .put(buildLdapSettings(OPEN_LDAP_DNS_URL, userTemplate, groupSearchBase, LdapSearchScope.ONE_LEVEL))
                .put("ssl.verification_mode", VerificationMode.FULL)
                .build();

        RealmConfig config = new RealmConfig("oldap-test", settings, globalSettings, TestEnvironment.newEnvironment(globalSettings),
                new ThreadContext(Settings.EMPTY));
        LdapSessionFactory sessionFactory = new LdapSessionFactory(config, sslService, threadPool);

        final String user = "blackwidow";
        try (LdapSession ldap = session(sessionFactory, user, PASSWORD_SECURE_STRING)) {
            assertThat(ldap, notNullValue());
            assertThat(ldap.userDn(), startsWith("uid=" + user + ","));
        }
    }

    public void testResolveSingleValuedAttributeFromConnection() throws Exception {
        LdapMetaDataResolver resolver = new LdapMetaDataResolver(Settings.builder().putList("metadata", "cn", "sn").build(), true);
        try (LDAPConnection ldapConnection = setupOpenLdapConnection()) {
            final Map<String, Object> map = resolve(ldapConnection, resolver);
            assertThat(map.size(), equalTo(2));
            assertThat(map.get("cn"), equalTo("Clint Barton"));
            assertThat(map.get("sn"), equalTo("Clint Barton"));
        }
    }

    public void testResolveMultiValuedAttributeFromConnection() throws Exception {
        LdapMetaDataResolver resolver = new LdapMetaDataResolver(Settings.builder().putList("metadata", "objectClass").build(), true);
        try (LDAPConnection ldapConnection = setupOpenLdapConnection()) {
            final Map<String, Object> map = resolve(ldapConnection, resolver);
            assertThat(map.size(), equalTo(1));
            assertThat(map.get("objectClass"), instanceOf(List.class));
            assertThat((List<?>) map.get("objectClass"), contains("top", "posixAccount", "inetOrgPerson"));
        }
    }

    public void testResolveMissingAttributeFromConnection() throws Exception {
        LdapMetaDataResolver resolver = new LdapMetaDataResolver(Settings.builder().putList("metadata", "alias").build(), true);
        try (LDAPConnection ldapConnection = setupOpenLdapConnection()) {
            final Map<String, Object> map = resolve(ldapConnection, resolver);
            assertThat(map.size(), equalTo(0));
        }
    }

    private Settings buildLdapSettings(String ldapUrl, String userTemplate, String groupSearchBase, LdapSearchScope scope) {
        Settings.Builder builder = Settings.builder()
            .put(LdapTestCase.buildLdapSettings(ldapUrl, userTemplate, groupSearchBase, scope));
        builder.put("group_search.user_attribute", "uid");
        if (useGlobalSSL) {
            return builder.build();
        }
        return builder
                .put("ssl.truststore.path", getDataPath(LDAPTRUST_PATH))
                .put("ssl.truststore.password", "changeit")
                .build();
    }

    private LdapSession session(SessionFactory factory, String username, SecureString password) {
        PlainActionFuture<LdapSession> future = new PlainActionFuture<>();
        factory.session(username, password, future);
        return future.actionGet();
    }

    private List<String> groups(LdapSession ldapSession) {
        PlainActionFuture<List<String>> future = new PlainActionFuture<>();
        ldapSession.groups(future);
        return future.actionGet();
    }

    private LDAPConnection setupOpenLdapConnection() throws Exception {
        Path truststore = getDataPath(LDAPTRUST_PATH);
        return LdapTestUtils.openConnection(OpenLdapTests.OPEN_LDAP_DNS_URL, HAWKEYE_DN, OpenLdapTests.PASSWORD, truststore);
    }

    private Map<String, Object> resolve(LDAPConnection connection, LdapMetaDataResolver resolver) throws Exception {
        final PlainActionFuture<Map<String, Object>> future = new PlainActionFuture<>();
        resolver.resolve(connection, HAWKEYE_DN, TimeValue.timeValueSeconds(1), logger, null, future);
        return future.get();
    }
}
