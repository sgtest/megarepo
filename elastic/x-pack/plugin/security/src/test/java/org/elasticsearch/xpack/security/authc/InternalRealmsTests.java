/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.security.authc;

import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.env.TestEnvironment;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.watcher.ResourceWatcherService;
import org.elasticsearch.xpack.core.security.authc.Realm;
import org.elasticsearch.xpack.core.security.authc.RealmConfig;
import org.elasticsearch.xpack.core.security.authc.esnative.NativeRealmSettings;
import org.elasticsearch.xpack.core.ssl.SSLService;
import org.elasticsearch.xpack.security.SecurityLifecycleService;
import org.elasticsearch.xpack.security.authc.esnative.NativeUsersStore;
import org.elasticsearch.xpack.security.authc.support.mapper.NativeRoleMappingStore;

import java.util.Map;
import java.util.function.BiConsumer;

import static org.elasticsearch.mock.orig.Mockito.times;
import static org.hamcrest.Matchers.any;
import static org.hamcrest.Matchers.hasEntry;
import static org.hamcrest.Matchers.is;
import static org.mockito.Matchers.isA;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.verifyZeroInteractions;

public class InternalRealmsTests extends ESTestCase {

    public void testNativeRealmRegistersIndexHealthChangeListener() throws Exception {
        SecurityLifecycleService lifecycleService = mock(SecurityLifecycleService.class);
        Map<String, Realm.Factory> factories = InternalRealms.getFactories(mock(ThreadPool.class), mock(ResourceWatcherService.class),
                mock(SSLService.class), mock(NativeUsersStore.class), mock(NativeRoleMappingStore.class), lifecycleService);
        assertThat(factories, hasEntry(is(NativeRealmSettings.TYPE), any(Realm.Factory.class)));
        verifyZeroInteractions(lifecycleService);

        Settings settings = Settings.builder().put("path.home", createTempDir()).build();
        factories.get(NativeRealmSettings.TYPE).create(new RealmConfig("test", Settings.EMPTY, settings,
            TestEnvironment.newEnvironment(settings), new ThreadContext(settings)));
        verify(lifecycleService).addSecurityIndexHealthChangeListener(isA(BiConsumer.class));

        factories.get(NativeRealmSettings.TYPE).create(new RealmConfig("test", Settings.EMPTY, settings,
            TestEnvironment.newEnvironment(settings), new ThreadContext(settings)));
        verify(lifecycleService, times(2)).addSecurityIndexHealthChangeListener(isA(BiConsumer.class));
    }
}
