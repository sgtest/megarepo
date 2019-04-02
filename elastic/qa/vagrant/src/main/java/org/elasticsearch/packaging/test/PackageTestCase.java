/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.packaging.test;

import com.carrotsearch.randomizedtesting.annotations.TestCaseOrdering;
import com.carrotsearch.randomizedtesting.generators.RandomStrings;
import org.apache.http.client.fluent.Request;
import org.elasticsearch.packaging.util.FileUtils;
import org.elasticsearch.packaging.util.Shell;
import org.elasticsearch.packaging.util.Shell.Result;
import org.hamcrest.CoreMatchers;
import org.junit.Before;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.file.StandardOpenOption;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

import static com.carrotsearch.randomizedtesting.RandomizedTest.getRandom;
import static org.elasticsearch.packaging.util.FileUtils.append;
import static org.elasticsearch.packaging.util.FileUtils.assertPathsDontExist;
import static org.elasticsearch.packaging.util.FileUtils.assertPathsExist;
import static org.elasticsearch.packaging.util.FileUtils.cp;
import static org.elasticsearch.packaging.util.FileUtils.fileWithGlobExist;
import static org.elasticsearch.packaging.util.FileUtils.mkdir;
import static org.elasticsearch.packaging.util.FileUtils.mv;
import static org.elasticsearch.packaging.util.FileUtils.rm;
import static org.elasticsearch.packaging.util.FileUtils.slurp;
import static org.elasticsearch.packaging.util.Packages.SYSTEMD_SERVICE;
import static org.elasticsearch.packaging.util.Packages.assertInstalled;
import static org.elasticsearch.packaging.util.Packages.assertRemoved;
import static org.elasticsearch.packaging.util.Packages.install;
import static org.elasticsearch.packaging.util.Packages.remove;
import static org.elasticsearch.packaging.util.Packages.restartElasticsearch;
import static org.elasticsearch.packaging.util.Packages.startElasticsearch;
import static org.elasticsearch.packaging.util.Packages.stopElasticsearch;
import static org.elasticsearch.packaging.util.Packages.verifyPackageInstallation;
import static org.elasticsearch.packaging.util.Platforms.getOsRelease;
import static org.elasticsearch.packaging.util.Platforms.isSystemd;
import static org.elasticsearch.packaging.util.ServerUtils.makeRequest;
import static org.elasticsearch.packaging.util.ServerUtils.runElasticsearchTests;
import static org.hamcrest.CoreMatchers.equalTo;
import static org.hamcrest.CoreMatchers.not;
import static org.hamcrest.CoreMatchers.notNullValue;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.isEmptyString;
import static org.hamcrest.core.Is.is;
import static org.junit.Assume.assumeThat;
import static org.junit.Assume.assumeTrue;

@TestCaseOrdering(TestCaseOrdering.AlphabeticOrder.class)
public abstract class PackageTestCase extends PackagingTestCase {
    private Shell sh;

    @Before
    public void onlyCompatibleDistributions() throws Exception {
        assumeTrue("only compatible distributions", distribution().packaging.compatible);
        sh = newShell();
    }

    public void test10InstallPackage() throws Exception {
        assertRemoved(distribution());
        installation = install(distribution());
        assertInstalled(distribution());
        verifyPackageInstallation(installation, distribution(), sh);
    }

    public void test20PluginsCommandWhenNoPlugins() throws Exception {
        assumeThat(installation, is(notNullValue()));

        assertThat(sh.run(installation.bin("elasticsearch-plugin") + " list").stdout, isEmptyString());
    }

    public void test30DaemonIsNotEnabledOnRestart() {
        if (isSystemd()) {
            sh.run("systemctl daemon-reload");
            String isEnabledOutput = sh.runIgnoreExitCode("systemctl is-enabled elasticsearch.service").stdout.trim();
            assertThat(isEnabledOutput, equalTo("disabled"));
        }
    }

    public void test31InstallDoesNotStartServer() {
        assumeThat(installation, is(notNullValue()));

        assertThat(sh.run("ps aux").stdout, not(containsString("org.elasticsearch.bootstrap.Elasticsearch")));
    }

    public void assertRunsWithJavaHome() throws Exception {
        String systemJavaHome = sh.run("echo $SYSTEM_JAVA_HOME").stdout.trim();
        byte[] originalEnvFile = Files.readAllBytes(installation.envFile);
        try {
            Files.write(installation.envFile, ("JAVA_HOME=" + systemJavaHome + "\n").getBytes(StandardCharsets.UTF_8),
                StandardOpenOption.APPEND);
            startElasticsearch(sh);
            runElasticsearchTests();
            stopElasticsearch(sh);
        } finally {
            Files.write(installation.envFile, originalEnvFile);
        }

        Path log = installation.logs.resolve("elasticsearch.log");
        assertThat(new String(Files.readAllBytes(log), StandardCharsets.UTF_8), containsString(systemJavaHome));
    }

    public void test32JavaHomeOverride() throws Exception {
        assumeThat(installation, is(notNullValue()));
        // we always run with java home when no bundled jdk is included, so this test would be repetitive
        assumeThat(distribution().hasJdk, is(true));

        assertRunsWithJavaHome();
    }

    public void test42BundledJdkRemoved() throws Exception {
        assumeThat(installation, is(notNullValue()));
        assumeThat(distribution().hasJdk, is(true));

        Path relocatedJdk = installation.bundledJdk.getParent().resolve("jdk.relocated");
        try {
            mv(installation.bundledJdk, relocatedJdk);
            assertRunsWithJavaHome();
        } finally {
            mv(relocatedJdk, installation.bundledJdk);
        }
    }

    public void test40StartServer() throws Exception {
        String start = sh.runIgnoreExitCode("date ").stdout.trim();
        assumeThat(installation, is(notNullValue()));

        startElasticsearch(sh);

        String journalEntries = sh.runIgnoreExitCode("journalctl _SYSTEMD_UNIT=elasticsearch.service " +
            "--since \"" + start + "\" --output cat | wc -l").stdout.trim();
        assertThat(journalEntries, equalTo("0"));

        assertPathsExist(installation.pidDir.resolve("elasticsearch.pid"));
        assertPathsExist(installation.logs.resolve("elasticsearch_server.json"));

        runElasticsearchTests();
        verifyPackageInstallation(installation, distribution(), sh); // check startup script didn't change permissions
    }

    public void test50Remove() throws Exception {
        assumeThat(installation, is(notNullValue()));

        remove(distribution());

        // removing must stop the service
        assertThat(sh.run("ps aux").stdout, not(containsString("org.elasticsearch.bootstrap.Elasticsearch")));

        if (isSystemd()) {

            final int statusExitCode;

            // Before version 231 systemctl returned exit code 3 for both services that were stopped, and nonexistent
            // services [1]. In version 231 and later it returns exit code 4 for non-existent services.
            //
            // The exception is Centos 7 and oel 7 where it returns exit code 4 for non-existent services from a systemd reporting a version
            // earlier than 231. Centos 6 does not have an /etc/os-release, but that's fine because it also doesn't use systemd.
            //
            // [1] https://github.com/systemd/systemd/pull/3385
            if (getOsRelease().contains("ID=\"centos\"") || getOsRelease().contains("ID=\"ol\"")) {
                statusExitCode = 4;
            } else {

                final Result versionResult = sh.run("systemctl --version");
                final Matcher matcher = Pattern.compile("^systemd (\\d+)\n").matcher(versionResult.stdout);
                matcher.find();
                final int version = Integer.parseInt(matcher.group(1));

                statusExitCode = version < 231
                    ? 3
                    : 4;
            }

            assertThat(sh.runIgnoreExitCode("systemctl status elasticsearch.service").exitCode, is(statusExitCode));
            assertThat(sh.runIgnoreExitCode("systemctl is-enabled elasticsearch.service").exitCode, is(1));

        }

        assertPathsDontExist(
            installation.bin,
            installation.lib,
            installation.modules,
            installation.plugins,
            installation.logs,
            installation.pidDir
        );

        assertFalse(Files.exists(SYSTEMD_SERVICE));
    }

    public void test60Reinstall() throws Exception {
        assumeThat(installation, is(notNullValue()));

        installation = install(distribution());
        assertInstalled(distribution());
        verifyPackageInstallation(installation, distribution(), sh);

        remove(distribution());
        assertRemoved(distribution());
    }

    public void test70RestartServer() throws Exception {
        try {
            installation = install(distribution());
            assertInstalled(distribution());

            startElasticsearch(sh);
            restartElasticsearch(sh);
            runElasticsearchTests();
            stopElasticsearch(sh);
        } finally {
            cleanup();
        }
    }


    public void test72TestRuntimeDirectory() throws Exception {
        try {
            installation = install(distribution());
            FileUtils.rm(installation.pidDir);
            startElasticsearch(sh);
            assertPathsExist(installation.pidDir);
            stopElasticsearch(sh);
        } finally {
            cleanup();
        }
    }

    public void test73gcLogsExist() throws Exception {
        installation = install(distribution());
        startElasticsearch(sh);
        // it can be gc.log or gc.log.0.current
        assertThat(installation.logs, fileWithGlobExist("gc.log*"));
        stopElasticsearch(sh);
    }

    // TEST CASES FOR SYSTEMD ONLY


    /**
     * # Simulates the behavior of a system restart:
     * # the PID directory is deleted by the operating system
     * # but it should not block ES from starting
     * # see https://github.com/elastic/elasticsearch/issues/11594
     */
    public void test80DeletePID_DIRandRestart() throws Exception {
        assumeTrue(isSystemd());

        rm(installation.pidDir);

        sh.run("systemd-tmpfiles --create");

        startElasticsearch(sh);

        final Path pidFile = installation.pidDir.resolve("elasticsearch.pid");

        assertTrue(Files.exists(pidFile));

        stopElasticsearch(sh);
    }

    public void test81CustomPathConfAndJvmOptions() throws Exception {
        assumeTrue(isSystemd());

        assumeThat(installation, is(notNullValue()));
        assertPathsExist(installation.envFile);

        stopElasticsearch(sh);

        // The custom config directory is not under /tmp or /var/tmp because
        // systemd's private temp directory functionally means different
        // processes can have different views of what's in these directories
        String randomName = RandomStrings.randomAsciiAlphanumOfLength(getRandom(), 10);
        sh.run("mkdir /etc/"+randomName);
        final Path tempConf = Paths.get("/etc/"+randomName);

        try {
            mkdir(tempConf);
            cp(installation.config("elasticsearch.yml"), tempConf.resolve("elasticsearch.yml"));
            cp(installation.config("log4j2.properties"), tempConf.resolve("log4j2.properties"));

            // we have to disable Log4j from using JMX lest it will hit a security
            // manager exception before we have configured logging; this will fail
            // startup since we detect usages of logging before it is configured
            final String jvmOptions =
                "-Xms512m\n" +
                    "-Xmx512m\n" +
                    "-Dlog4j2.disable.jmx=true\n";
            append(tempConf.resolve("jvm.options"), jvmOptions);

            sh.runIgnoreExitCode("chown -R elasticsearch:elasticsearch " + tempConf);

            final Shell serverShell = newShell();
            cp(installation.envFile, tempConf.resolve("elasticsearch.bk"));//backup
            append(installation.envFile, "ES_PATH_CONF=" + tempConf + "\n");
            append(installation.envFile, "ES_JAVA_OPTS=-XX:-UseCompressedOops");

            startElasticsearch(serverShell);

            final String nodesResponse = makeRequest(Request.Get("http://localhost:9200/_nodes"));
            assertThat(nodesResponse, CoreMatchers.containsString("\"heap_init_in_bytes\":536870912"));
            assertThat(nodesResponse, CoreMatchers.containsString("\"using_compressed_ordinary_object_pointers\":\"false\""));

            stopElasticsearch(serverShell);

        } finally {
            rm(installation.envFile);
            cp(tempConf.resolve("elasticsearch.bk"), installation.envFile);
            rm(tempConf);
            cleanup();
        }
    }

    public void test82SystemdMask() throws Exception {
        try {
            assumeTrue(isSystemd());

            sh.run("systemctl mask systemd-sysctl.service");

            installation = install(distribution());

            sh.run("systemctl unmask systemd-sysctl.service");
        } finally {
            cleanup();
        }
    }

    public void test83serviceFileSetsLimits() throws Exception {
        // Limits are changed on systemd platforms only
        assumeTrue(isSystemd());

        installation = install(distribution());

        startElasticsearch(sh);

        final Path pidFile = installation.pidDir.resolve("elasticsearch.pid");
        assertTrue(Files.exists(pidFile));
        String pid = slurp(pidFile).trim();
        String maxFileSize = sh.run("cat /proc/%s/limits | grep \"Max file size\" | awk '{ print $4 }'", pid).stdout.trim();
        assertThat(maxFileSize, equalTo("unlimited"));

        String maxProcesses = sh.run("cat /proc/%s/limits | grep \"Max processes\" | awk '{ print $3 }'", pid).stdout.trim();
        assertThat(maxProcesses, equalTo("4096"));

        String maxOpenFiles = sh.run("cat /proc/%s/limits | grep \"Max open files\" | awk '{ print $4 }'", pid).stdout.trim();
        assertThat(maxOpenFiles, equalTo("65535"));

        String maxAddressSpace = sh.run("cat /proc/%s/limits | grep \"Max address space\" | awk '{ print $4 }'", pid).stdout.trim();
        assertThat(maxAddressSpace, equalTo("unlimited"));

        stopElasticsearch(sh);
    }
}
