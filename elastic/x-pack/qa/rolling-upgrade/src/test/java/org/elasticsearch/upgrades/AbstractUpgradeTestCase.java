/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.upgrades;

import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.test.SecuritySettingsSourceField;
import org.elasticsearch.test.rest.ESRestTestCase;
import org.junit.Before;

import java.io.IOException;
import java.util.Collection;
import java.util.Collections;

import static org.elasticsearch.xpack.core.security.authc.support.UsernamePasswordToken.basicAuthHeaderValue;

public abstract class AbstractUpgradeTestCase extends ESRestTestCase {

    private static final String BASIC_AUTH_VALUE =
            basicAuthHeaderValue("test_user", SecuritySettingsSourceField.TEST_PASSWORD_SECURE_STRING);

    @Override
    protected boolean preserveIndicesUponCompletion() {
        return true;
    }

    @Override
    protected boolean preserveReposUponCompletion() {
        return true;
    }

    @Override
    protected boolean preserveTemplatesUponCompletion() {
        return true;
    }

    enum CLUSTER_TYPE {
        OLD,
        MIXED,
        UPGRADED;

        public static CLUSTER_TYPE parse(String value) {
            switch (value) {
                case "old_cluster":
                    return OLD;
                case "mixed_cluster":
                    return MIXED;
                case "upgraded_cluster":
                    return UPGRADED;
                default:
                    throw new AssertionError("unknown cluster type: " + value);
            }
        }
    }

    protected final CLUSTER_TYPE clusterType = CLUSTER_TYPE.parse(System.getProperty("tests.rest.suite"));

    @Override
    protected Settings restClientSettings() {
        return Settings.builder()
                .put(ThreadContext.PREFIX + ".Authorization", BASIC_AUTH_VALUE)
                .build();
    }

    protected Collection<String> templatesToWaitFor() {
        return Collections.singletonList("security-index-template");
    }

    @Before
    public void setupForTests() throws Exception {
        awaitBusy(() -> {
            boolean success = true;
            for (String template : templatesToWaitFor()) {
                try {
                    final boolean exists =
                            adminClient().performRequest("HEAD", "_template/" + template).getStatusLine().getStatusCode() == 200;
                    success &= exists;
                    logger.debug("template [{}] exists [{}]", template, exists);
                } catch (IOException e) {
                    logger.warn("error calling template api", e);
                }
            }
            return success;
        });
    }
}
