/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.packaging.test;

import org.apache.http.client.fluent.Request;
import org.elasticsearch.packaging.util.Installation;
import org.elasticsearch.packaging.util.Platforms;
import org.elasticsearch.packaging.util.Shell;
import org.junit.Before;

import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;

import static org.elasticsearch.packaging.util.ServerUtils.makeRequest;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.junit.Assume.assumeFalse;
import static org.junit.Assume.assumeTrue;

public class PluginCliTests extends PackagingTestCase {

    private static final String EXAMPLE_PLUGIN_NAME = "custom-settings";
    private static final Path EXAMPLE_PLUGIN_ZIP;
    static {
        // re-read before each test so the plugin path can be manipulated within tests
        EXAMPLE_PLUGIN_ZIP = Paths.get(System.getProperty("tests.example-plugin"));
    }

    @Before
    public void filterDistros() {
        assumeFalse("no docker", distribution.isDocker());
    }

    @FunctionalInterface
    public interface PluginAction {
        void run(Shell.Result installResult) throws Exception;
    }

    private Shell.Result assertWithPlugin(Installation.Executable pluginTool, Path pluginZip, String pluginName, PluginAction action)
        throws Exception {
        Shell.Result installResult = pluginTool.run("install --batch \"" + pluginZip.toUri().toString() + "\"");
        action.run(installResult);
        return pluginTool.run("remove " + pluginName);
    }

    private void assertWithExamplePlugin(PluginAction action) throws Exception {
        assertWithPlugin(installation.executables().pluginTool, EXAMPLE_PLUGIN_ZIP, EXAMPLE_PLUGIN_NAME, action);
    }

    public void test10Install() throws Exception {
        install();
    }

    public void test20SymlinkPluginsDir() throws Exception {
        Path pluginsDir = installation.plugins;
        Path stashedPluginsDir = createTempDir("stashed-plugins");

        Files.delete(stashedPluginsDir); // delete so we can replace it
        Files.move(pluginsDir, stashedPluginsDir);
        Path linkedPlugins = createTempDir("symlinked-plugins");
        Platforms.onLinux(() -> sh.run("chown elasticsearch:elasticsearch " + linkedPlugins.toString()));
        Files.createSymbolicLink(pluginsDir, linkedPlugins);
        assertWithExamplePlugin(installResult -> {
            assertWhileRunning(() -> {
                final String pluginsResponse = makeRequest(Request.Get("http://localhost:9200/_cat/plugins?h=component")).strip();
                assertThat(pluginsResponse, equalTo(EXAMPLE_PLUGIN_NAME));

                String settingsPath = "_cluster/settings?include_defaults&filter_path=defaults.custom.simple";
                final String settingsResponse = makeRequest(Request.Get("http://localhost:9200/" + settingsPath)).strip();
                assertThat(settingsResponse, equalTo("{\"defaults\":{\"custom\":{\"simple\":\"foo\"}}}"));
            });
        });

        Files.delete(pluginsDir);
        Files.move(stashedPluginsDir, pluginsDir);
    }

    public void test21CustomConfDir() throws Exception {
        withCustomConfig(confPath -> assertWithExamplePlugin(installResult -> {}));
    }

    public void test22PluginZipWithSpace() throws Exception {
        Path spacedDir = createTempDir("spaced dir");
        Path plugin = Files.copy(EXAMPLE_PLUGIN_ZIP, spacedDir.resolve(EXAMPLE_PLUGIN_ZIP.getFileName()));
        assertWithPlugin(installation.executables().pluginTool, plugin, EXAMPLE_PLUGIN_NAME, installResult -> {});
    }

    public void test23ElasticsearchWithSpace() throws Exception {
        assumeTrue(distribution.isArchive());

        Path spacedDir = createTempDir("spaced dir");
        Path elasticsearch = spacedDir.resolve("elasticsearch");
        Files.move(installation.home, elasticsearch);
        Installation spacedInstallation = Installation.ofArchive(sh, distribution, elasticsearch);

        assertWithPlugin(spacedInstallation.executables().pluginTool, EXAMPLE_PLUGIN_ZIP, EXAMPLE_PLUGIN_NAME, installResult -> {});

        Files.move(elasticsearch, installation.home);
    }

    public void test24JavaOpts() throws Exception {
        sh.getEnv().put("ES_JAVA_OPTS", "-XX:+PrintFlagsFinal");
        assertWithExamplePlugin(installResult -> assertThat(installResult.stdout, containsString("MaxHeapSize")));
    }

    public void test25Umask() throws Exception {
        sh.setUmask("0077");
        assertWithExamplePlugin(installResult -> {});
    }
}
