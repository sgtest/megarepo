/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.authc.ldap;

import com.unboundid.ldap.sdk.Attribute;
import com.unboundid.ldap.sdk.SearchRequest;
import com.unboundid.ldap.sdk.SearchScope;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.xpack.core.security.support.NoOpLogger;
import org.elasticsearch.xpack.security.authc.ldap.support.LdapUtils;

import java.util.Collection;
import java.util.List;

import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.hasItems;

public class UserAttributeGroupsResolverTests extends GroupsResolverTestCase {

    public static final String BRUCE_BANNER_DN = "cn=Bruce Banner,CN=Users,DC=ad,DC=test,DC=elasticsearch,DC=com";

    @SuppressWarnings("unchecked")
    public void testResolve() throws Exception {
        //falling back on the 'memberOf' attribute
        UserAttributeGroupsResolver resolver = new UserAttributeGroupsResolver(Settings.EMPTY);
        List<String> groups =
                resolveBlocking(resolver, ldapConnection, BRUCE_BANNER_DN, TimeValue.timeValueSeconds(20), NoOpLogger.INSTANCE, null);
        assertThat(groups, containsInAnyOrder(
                containsString("Avengers"),
                containsString("SHIELD"),
                containsString("Geniuses"),
                containsString("Philanthropists")));
    }

    @SuppressWarnings("unchecked")
    public void testResolveFromPreloadedAttributes() throws Exception {
        SearchRequest preSearch = new SearchRequest(BRUCE_BANNER_DN, SearchScope.BASE, LdapUtils.OBJECT_CLASS_PRESENCE_FILTER, "memberOf");
        final Collection<Attribute> attributes = ldapConnection.searchForEntry(preSearch).getAttributes();

        UserAttributeGroupsResolver resolver = new UserAttributeGroupsResolver(Settings.EMPTY);
        List<String> groups =
                resolveBlocking(resolver, ldapConnection, BRUCE_BANNER_DN, TimeValue.timeValueSeconds(20), NoOpLogger.INSTANCE, attributes);
        assertThat(groups, containsInAnyOrder(
                containsString("Avengers"),
                containsString("SHIELD"),
                containsString("Geniuses"),
                containsString("Philanthropists")));
    }

    @SuppressWarnings("unchecked")
    public void testResolveCustomGroupAttribute() throws Exception {
        Settings settings = Settings.builder()
                .put("user_group_attribute", "seeAlso")
                .build();
        UserAttributeGroupsResolver resolver = new UserAttributeGroupsResolver(settings);
        List<String> groups =
                resolveBlocking(resolver, ldapConnection, BRUCE_BANNER_DN, TimeValue.timeValueSeconds(20), NoOpLogger.INSTANCE, null);
        assertThat(groups, hasItems(containsString("Avengers")));  //seeAlso only has Avengers
    }

    public void testResolveInvalidGroupAttribute() throws Exception {
        Settings settings = Settings.builder()
                .put("user_group_attribute", "doesntExist")
                .build();
        UserAttributeGroupsResolver resolver = new UserAttributeGroupsResolver(settings);
        List<String> groups =
                resolveBlocking(resolver, ldapConnection, BRUCE_BANNER_DN, TimeValue.timeValueSeconds(20), NoOpLogger.INSTANCE, null);
        assertThat(groups, empty());
    }

    @Override
    protected String ldapUrl() {
        return ActiveDirectorySessionFactoryTests.AD_LDAP_URL;
    }

    @Override
    protected String bindDN() {
        return BRUCE_BANNER_DN;
    }

    @Override
    protected String bindPassword() {
        return ActiveDirectorySessionFactoryTests.PASSWORD;
    }

    @Override
    protected String trustPath() {
        return "/org/elasticsearch/xpack/security/authc/ldap/support/ADtrust.jks";
    }
}
