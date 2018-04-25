/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.authc.ldap;

import com.unboundid.ldap.sdk.LDAPConnection;
import com.unboundid.ldap.sdk.LDAPConnectionOptions;
import com.unboundid.ldap.sdk.LDAPURL;
import org.apache.lucene.util.LuceneTestCase;
import org.elasticsearch.common.settings.MockSecureSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.TestEnvironment;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.core.security.authc.ldap.support.SessionFactorySettings;
import org.elasticsearch.xpack.core.ssl.SSLService;
import org.elasticsearch.xpack.core.ssl.VerificationMode;
import org.elasticsearch.xpack.security.authc.ldap.support.LdapUtils;

import java.nio.file.Path;

public class LdapTestUtils {

    private LdapTestUtils() {
        // Utility class
    }

    public static LDAPConnection openConnection(String url, String bindDN, String bindPassword, Path truststore) throws Exception {
        boolean useGlobalSSL = ESTestCase.randomBoolean();
        Settings.Builder builder = Settings.builder().put("path.home", LuceneTestCase.createTempDir());
        MockSecureSettings secureSettings = new MockSecureSettings();
        builder.setSecureSettings(secureSettings);
        if (useGlobalSSL) {
            builder.put("xpack.ssl.truststore.path", truststore);
            // fake realm to load config with certificate verification mode
            builder.put("xpack.security.authc.realms.bar.ssl.truststore.path", truststore);
            builder.put("xpack.security.authc.realms.bar.ssl.verification_mode", VerificationMode.CERTIFICATE);
            secureSettings.setString("xpack.ssl.truststore.secure_password", "changeit");
            secureSettings.setString("xpack.security.authc.realms.bar.ssl.truststore.secure_password", "changeit");
        } else {
            // fake realms so ssl will get loaded
            builder.put("xpack.security.authc.realms.foo.ssl.truststore.path", truststore);
            builder.put("xpack.security.authc.realms.foo.ssl.verification_mode", VerificationMode.FULL);
            builder.put("xpack.security.authc.realms.bar.ssl.truststore.path", truststore);
            builder.put("xpack.security.authc.realms.bar.ssl.verification_mode", VerificationMode.CERTIFICATE);
            secureSettings.setString("xpack.security.authc.realms.foo.ssl.truststore.secure_password", "changeit");
            secureSettings.setString("xpack.security.authc.realms.bar.ssl.truststore.secure_password", "changeit");
        }
        Settings settings = builder.build();
        Environment env = TestEnvironment.newEnvironment(settings);
        SSLService sslService = new SSLService(settings, env);

        LDAPURL ldapurl = new LDAPURL(url);
        LDAPConnectionOptions options = new LDAPConnectionOptions();
        options.setFollowReferrals(true);
        options.setAllowConcurrentSocketFactoryUse(true);
        options.setConnectTimeoutMillis(Math.toIntExact(SessionFactorySettings.TIMEOUT_DEFAULT.millis()));
        options.setResponseTimeoutMillis(SessionFactorySettings.TIMEOUT_DEFAULT.millis());

        Settings connectionSettings;
        if (useGlobalSSL) {
            connectionSettings = Settings.EMPTY;
        } else {
            MockSecureSettings connSecureSettings = new MockSecureSettings();
            connSecureSettings.setString("truststore.secure_password", "changeit");
            connectionSettings = Settings.builder().put("truststore.path", truststore)
                        .setSecureSettings(connSecureSettings).build();
        }
        return LdapUtils.privilegedConnect(() -> new LDAPConnection(sslService.sslSocketFactory(connectionSettings), options,
                ldapurl.getHost(), ldapurl.getPort(), bindDN, bindPassword));
    }
}
