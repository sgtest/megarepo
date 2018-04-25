/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.example;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.example.realm.CustomAuthenticationFailureHandler;
import org.elasticsearch.example.realm.CustomRealm;
import org.elasticsearch.example.role.CustomInMemoryRolesProvider;
import org.elasticsearch.watcher.ResourceWatcherService;
import org.elasticsearch.xpack.core.security.authc.AuthenticationFailureHandler;
import org.elasticsearch.xpack.core.security.authc.Realm;
import org.elasticsearch.xpack.core.security.authz.RoleDescriptor;
import org.elasticsearch.xpack.core.security.SecurityExtension;

import java.security.AccessController;
import java.security.PrivilegedAction;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.BiConsumer;

import static org.elasticsearch.example.role.CustomInMemoryRolesProvider.ROLE_A;
import static org.elasticsearch.example.role.CustomInMemoryRolesProvider.ROLE_B;

/**
 * An example x-pack extension for testing custom realms and custom role providers.
 */
public class ExampleSecurityExtension implements SecurityExtension {

    static {
        // check that the extension's policy works.
        AccessController.doPrivileged((PrivilegedAction<Void>) () -> {
            System.getSecurityManager().checkPrintJobAccess();
            return null;
        });
    }

    @Override
    public Map<String, Realm.Factory> getRealms(ResourceWatcherService resourceWatcherService) {
        return Collections.singletonMap(CustomRealm.TYPE, CustomRealm::new);
    }

    @Override
    public AuthenticationFailureHandler getAuthenticationFailureHandler() {
        return new CustomAuthenticationFailureHandler();
    }


    @Override
    public List<BiConsumer<Set<String>, ActionListener<Set<RoleDescriptor>>>>
    getRolesProviders(Settings settings, ResourceWatcherService resourceWatcherService) {
        CustomInMemoryRolesProvider rp1 = new CustomInMemoryRolesProvider(settings, Collections.singletonMap(ROLE_A, "read"));
        Map<String, String> roles = new HashMap<>();
        roles.put(ROLE_A, "all");
        roles.put(ROLE_B, "all");
        CustomInMemoryRolesProvider rp2 = new CustomInMemoryRolesProvider(settings, roles);
        return Arrays.asList(rp1, rp2);
    }
}
