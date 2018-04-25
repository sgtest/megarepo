/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.authc.esnative;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.cluster.health.ClusterHealthStatus;
import org.elasticsearch.cluster.health.ClusterIndexHealth;
import org.elasticsearch.xpack.core.security.authc.AuthenticationResult;
import org.elasticsearch.xpack.core.security.authc.RealmConfig;
import org.elasticsearch.xpack.core.security.authc.esnative.NativeRealmSettings;
import org.elasticsearch.xpack.core.security.authc.support.UsernamePasswordToken;
import org.elasticsearch.xpack.core.security.user.User;
import org.elasticsearch.xpack.security.authc.support.CachingUsernamePasswordRealm;

/**
 * User/password realm that is backed by an Elasticsearch index
 */
public class NativeRealm extends CachingUsernamePasswordRealm {

    private final NativeUsersStore userStore;

    public NativeRealm(RealmConfig config, NativeUsersStore usersStore) {
        super(NativeRealmSettings.TYPE, config);
        this.userStore = usersStore;
    }

    @Override
    protected void doLookupUser(String username, ActionListener<User> listener) {
        userStore.getUser(username, listener);
    }

    @Override
    protected void doAuthenticate(UsernamePasswordToken token, ActionListener<AuthenticationResult> listener) {
        userStore.verifyPassword(token.principal(), token.credentials(), listener);
    }

    public void onSecurityIndexHealthChange(ClusterIndexHealth previousHealth, ClusterIndexHealth currentHealth) {
        final boolean movedFromRedToNonRed = (previousHealth == null || previousHealth.getStatus() == ClusterHealthStatus.RED)
                && currentHealth != null && currentHealth.getStatus() != ClusterHealthStatus.RED;
        final boolean indexDeleted = previousHealth != null && currentHealth == null;

        if (movedFromRedToNonRed || indexDeleted) {
            clearCache();
        }
    }

    // method is used for testing to verify cache expiration since expireAll is final
    void clearCache() {
        expireAll();
    }

}
