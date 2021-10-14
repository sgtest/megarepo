/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.packaging.test;

import org.apache.http.client.fluent.Request;
import org.elasticsearch.packaging.util.FileUtils;
import org.elasticsearch.packaging.util.Platforms;
import org.elasticsearch.packaging.util.ServerUtils;
import org.junit.Before;

import static org.elasticsearch.packaging.util.FileUtils.append;
import static org.hamcrest.Matchers.equalTo;
import static org.junit.Assume.assumeFalse;

public class ConfigurationTests extends PackagingTestCase {

    @Before
    public void filterDistros() {
        assumeFalse("no docker", distribution.isDocker());
    }

    public void test10Install() throws Exception {
        install();
        setFileSuperuser("test_superuser", "test_superuser_password");
    }

    public void test20HostnameSubstitution() throws Exception {
        String hostnameKey = Platforms.WINDOWS ? "COMPUTERNAME" : "HOSTNAME";
        sh.getEnv().put(hostnameKey, "mytesthost");
        withCustomConfig(confPath -> {
            FileUtils.append(confPath.resolve("elasticsearch.yml"), "node.name: ${HOSTNAME}");
            if (distribution.isPackage()) {
                append(installation.envFile, "HOSTNAME=mytesthost");
            }
            // Packaged installations don't get autoconfigured yet
            // TODO: Remove this in https://github.com/elastic/elasticsearch/pull/75144
            String protocol = distribution.isPackage() ? "http" : "https";
            // security auto-config requires that the archive owner and the node process user be the same
            Platforms.onWindows(() -> sh.chown(confPath, installation.getOwner()));
            assertWhileRunning(() -> {
                final String nameResponse = ServerUtils.makeRequest(
                    Request.Get(protocol + "://localhost:9200/_cat/nodes?h=name"),
                    "test_superuser",
                    "test_superuser_password",
                    ServerUtils.getCaCert(confPath)
                ).strip();
                assertThat(nameResponse, equalTo("mytesthost"));
            });
            Platforms.onWindows(() -> sh.chown(confPath));
        });
    }
}
