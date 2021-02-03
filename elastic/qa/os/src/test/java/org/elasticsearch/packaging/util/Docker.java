/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.packaging.util;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.apache.commons.logging.Log;
import org.apache.commons.logging.LogFactory;
import org.apache.http.client.fluent.Request;
import org.elasticsearch.common.CheckedRunnable;

import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.file.attribute.PosixFileAttributes;
import java.nio.file.attribute.PosixFilePermission;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.stream.Stream;

import static java.nio.file.attribute.PosixFilePermissions.fromString;
import static org.elasticsearch.packaging.util.FileMatcher.p644;
import static org.elasticsearch.packaging.util.FileMatcher.p660;
import static org.elasticsearch.packaging.util.FileMatcher.p664;
import static org.elasticsearch.packaging.util.FileMatcher.p755;
import static org.elasticsearch.packaging.util.FileMatcher.p770;
import static org.elasticsearch.packaging.util.FileMatcher.p775;
import static org.elasticsearch.packaging.util.FileUtils.getCurrentVersion;
import static org.elasticsearch.packaging.util.ServerUtils.makeRequest;
import static org.hamcrest.MatcherAssert.assertThat;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

/**
 * Utilities for running packaging tests against the Elasticsearch Docker images.
 */
public class Docker {
    private static final Log logger = LogFactory.getLog(Docker.class);

    static final Shell sh = new Shell();
    private static final DockerShell dockerShell = new DockerShell();
    public static final int STARTUP_SLEEP_INTERVAL_MILLISECONDS = 1000;
    public static final int STARTUP_ATTEMPTS_MAX = 10;

    /**
     * Tracks the currently running Docker image. An earlier implementation used a fixed container name,
     * but that appeared to cause problems with repeatedly destroying and recreating containers with
     * the same name.
     */
    private static String containerId = null;

    /**
     * Checks whether the required Docker image exists. If not, the image is loaded from disk. No check is made
     * to see whether the image is up-to-date.
     * @param distribution details about the docker image to potentially load.
     */
    public static void ensureImageIsLoaded(Distribution distribution) {
        final long count = sh.run("docker image ls --format '{{.Repository}}' " + getImageName(distribution)).stdout.lines().count();

        if (count != 0) {
            return;
        }

        logger.info("Loading Docker image: " + distribution.path);
        sh.run("docker load -i " + distribution.path);
    }

    /**
     * Runs an Elasticsearch Docker container, and checks that it has started up
     * successfully.
     *
     * @param distribution details about the docker image being tested
     */
    public static Installation runContainer(Distribution distribution) {
        return runContainer(distribution, DockerRun.builder());
    }

    /**
     * Runs an Elasticsearch Docker container, and checks that it has started up
     * successfully.
     *
     * @param distribution details about the docker image being tested
     * @param builder the command to run
     */
    public static Installation runContainer(Distribution distribution, DockerRun builder) {
        executeDockerRun(distribution, builder);

        waitForElasticsearchToStart();

        return Installation.ofContainer(dockerShell, distribution);
    }

    /**
     * Similar to {@link #runContainer(Distribution, DockerRun)} in that it runs an Elasticsearch Docker
     * container, expect that the container expecting it to exit e.g. due to configuration problem.
     *
     * @param distribution details about the docker image being tested.
     * @param builder the command to run
     * @return the docker logs of the container
     */
    public static Shell.Result runContainerExpectingFailure(Distribution distribution, DockerRun builder) {
        executeDockerRun(distribution, builder);

        waitForElasticsearchToExit();

        return getContainerLogs();
    }

    private static void executeDockerRun(Distribution distribution, DockerRun builder) {
        removeContainer();

        final String command = builder.distribution(distribution).build();

        logger.info("Running command: " + command);
        containerId = sh.run(command).stdout.trim();
    }

    /**
     * Waits for the Elasticsearch process to start executing in the container.
     * This is called every time a container is started.
     */
    public static void waitForElasticsearchToStart() {
        boolean isElasticsearchRunning = false;
        int attempt = 0;

        String psOutput = null;

        do {
            try {
                // Give the container a chance to crash out
                Thread.sleep(STARTUP_SLEEP_INTERVAL_MILLISECONDS);

                // Set COLUMNS so that `ps` doesn't truncate its output
                psOutput = dockerShell.run("bash -c 'COLUMNS=2000 ps ax'").stdout;

                if (psOutput.contains("org.elasticsearch.bootstrap.Elasticsearch")) {
                    isElasticsearchRunning = true;
                    break;
                }
            } catch (Exception e) {
                logger.warn("Caught exception while waiting for ES to start", e);
            }
        } while (attempt++ < STARTUP_ATTEMPTS_MAX);

        if (isElasticsearchRunning == false) {
            final Shell.Result dockerLogs = getContainerLogs();
            fail(
                "Elasticsearch container did not start successfully.\n\nps output:\n"
                    + psOutput
                    + "\n\nStdout:\n"
                    + dockerLogs.stdout
                    + "\n\nStderr:\n"
                    + dockerLogs.stderr
            );
        }
    }

    /**
     * Waits for the Elasticsearch container to exit.
     */
    private static void waitForElasticsearchToExit() {
        boolean isElasticsearchRunning = true;
        int attempt = 0;

        do {
            try {
                // Give the container a chance to exit out
                Thread.sleep(1000);

                if (sh.run("docker ps --quiet --no-trunc").stdout.contains(containerId) == false) {
                    isElasticsearchRunning = false;
                    break;
                }
            } catch (Exception e) {
                logger.warn("Caught exception while waiting for ES to exit", e);
            }
        } while (attempt++ < 5);

        if (isElasticsearchRunning) {
            final Shell.Result dockerLogs = getContainerLogs();
            fail("Elasticsearch container didn't exit.\n\nStdout:\n" + dockerLogs.stdout + "\n\nStderr:\n" + dockerLogs.stderr);
        }
    }

    /**
     * Removes the currently running container.
     */
    public static void removeContainer() {
        if (containerId != null) {
            try {
                // Remove the container, forcibly killing it if necessary
                logger.debug("Removing container " + containerId);
                final String command = "docker rm -f " + containerId;
                final Shell.Result result = sh.runIgnoreExitCode(command);

                if (result.isSuccess() == false) {
                    boolean isErrorAcceptable = result.stderr.contains("removal of container " + containerId + " is already in progress")
                        || result.stderr.contains("Error: No such container: " + containerId);

                    // I'm not sure why we're already removing this container, but that's OK.
                    if (isErrorAcceptable == false) {
                        throw new RuntimeException("Command was not successful: [" + command + "] result: " + result.toString());
                    }
                }
            } finally {
                // Null out the containerId under all circumstances, so that even if the remove command fails
                // for some reason, the other tests will still proceed. Otherwise they can get stuck, continually
                // trying to remove a non-existent container ID.
                containerId = null;
            }
        }
    }

    /**
     * Copies a file from the container into the local filesystem
     * @param from the file to copy in the container
     * @param to the location to place the copy
     */
    public static void copyFromContainer(Path from, Path to) {
        final String script = "docker cp " + containerId + ":" + from + " " + to;
        logger.debug("Copying file from container with: " + script);
        sh.run(script);
    }

    /**
     * Extends {@link Shell} so that executed commands happen in the currently running Docker container.
     */
    public static class DockerShell extends Shell {
        @Override
        protected String[] getScriptCommand(String script) {
            assert containerId != null;

            List<String> cmd = new ArrayList<>();
            cmd.add("docker");
            cmd.add("exec");
            cmd.add("--user");
            cmd.add("elasticsearch:root");
            cmd.add("--tty");

            env.forEach((key, value) -> cmd.add("--env " + key + "=\"" + value + "\""));

            cmd.add(containerId);
            cmd.add(script);

            return super.getScriptCommand(String.join(" ", cmd));
        }

        /**
         * Overrides {@link Shell#run(String)} to attempt to collect Docker container
         * logs when a command fails to execute successfully.
         * @param script the command to run
         * @return the command's output
         */
        @Override
        public Result run(String script) {
            try {
                return super.run(script);
            } catch (ShellException e) {
                try {
                    final Shell.Result dockerLogs = getContainerLogs();
                    logger.error(
                        "Command [{}] failed.\n\nContainer stdout: [{}]\n\nContainer stderr: [{}]",
                        script,
                        dockerLogs.stdout,
                        dockerLogs.stderr
                    );
                } catch (ShellException shellException) {
                    logger.error(
                        "Command [{}] failed.\n\nTried to dump container logs but that failed too: [{}]",
                        script,
                        shellException.getMessage()
                    );
                }
                throw e;
            }
        }
    }

    /**
     * Checks whether a path exists in the Docker container.
     */
    public static boolean existsInContainer(Path path) {
        return existsInContainer(path.toString());
    }

    /**
     * Checks whether a path exists in the Docker container.
     */
    public static boolean existsInContainer(String path) {
        logger.debug("Checking whether file " + path + " exists in container");
        final Shell.Result result = dockerShell.runIgnoreExitCode("test -e " + path);

        return result.isSuccess();
    }

    /**
     * Run privilege escalated shell command on the local file system via a bind mount inside a Docker container.
     * @param shellCmd The shell command to execute on the localPath e.g. `mkdir /containerPath/dir`.
     * @param localPath The local path where shellCmd will be executed on (inside a container).
     * @param containerPath The path to mount localPath inside the container.
     */
    private static void executePrivilegeEscalatedShellCmd(String shellCmd, Path localPath, Path containerPath) {
        final List<String> args = new ArrayList<>();

        args.add("docker run");

        // Don't leave orphaned containers
        args.add("--rm");

        // Mount localPath to a known location inside the container, so that we can execute shell commands on it later
        args.add("--volume \"" + localPath.getParent() + ":" + containerPath.getParent() + "\"");

        // Use a lightweight musl libc based small image
        args.add("alpine");

        // And run inline commands via the POSIX shell
        args.add("/bin/sh -c \"" + shellCmd + "\"");

        final String command = String.join(" ", args);
        logger.info("Running command: " + command);
        sh.run(command);
    }

    /**
     * Create a directory with specified uid/gid using Docker backed privilege escalation.
     * @param localPath The path to the directory to create.
     * @param uid The numeric id for localPath
     * @param gid The numeric id for localPath
     */
    public static void mkDirWithPrivilegeEscalation(Path localPath, int uid, int gid) {
        final Path containerBasePath = Paths.get("/mount");
        final Path containerPath = containerBasePath.resolve(Paths.get("/").relativize(localPath));
        final List<String> args = new ArrayList<>();

        args.add("mkdir " + containerPath.toAbsolutePath());
        args.add("&&");
        args.add("chown " + uid + ":" + gid + " " + containerPath.toAbsolutePath());
        args.add("&&");
        args.add("chmod 0770 " + containerPath.toAbsolutePath());
        final String command = String.join(" ", args);
        executePrivilegeEscalatedShellCmd(command, localPath, containerPath);

        final PosixFileAttributes dirAttributes = FileUtils.getPosixFileAttributes(localPath);
        final Map<String, Integer> numericPathOwnership = FileUtils.getNumericUnixPathOwnership(localPath);
        assertThat(localPath + " has wrong uid", numericPathOwnership.get("uid"), equalTo(uid));
        assertThat(localPath + " has wrong gid", numericPathOwnership.get("gid"), equalTo(gid));
        assertThat(localPath + " has wrong permissions", dirAttributes.permissions(), equalTo(p770));
    }

    /**
     * Delete a directory using Docker backed privilege escalation.
     * @param localPath The path to the directory to delete.
     */
    public static void rmDirWithPrivilegeEscalation(Path localPath) {
        final Path containerBasePath = Paths.get("/mount");
        final Path containerPath = containerBasePath.resolve(localPath.getParent().getFileName());
        final List<String> args = new ArrayList<>();

        args.add("cd " + containerBasePath.toAbsolutePath());
        args.add("&&");
        args.add("rm -rf " + localPath.getFileName());
        final String command = String.join(" ", args);
        executePrivilegeEscalatedShellCmd(command, localPath, containerPath);
    }

    /**
     * Change the ownership of a path using Docker backed privilege escalation.
     * @param localPath The path to the file or directory to change.
     * @param ownership the ownership to apply. Can either be just the user, or the user and group, separated by a colon (":"),
     *                  or just the group if prefixed with a colon.
     */
    public static void chownWithPrivilegeEscalation(Path localPath, String ownership) {
        final Path containerBasePath = Paths.get("/mount");
        final Path containerPath = containerBasePath.resolve(localPath.getParent().getFileName());
        final List<String> args = new ArrayList<>();

        args.add("cd " + containerBasePath.toAbsolutePath());
        args.add("&&");
        args.add("chown -R " + ownership + " " + localPath.getFileName());
        final String command = String.join(" ", args);
        executePrivilegeEscalatedShellCmd(command, localPath, containerPath);
    }

    /**
     * Checks that the specified path's permissions and ownership match those specified.
     */
    public static void assertPermissionsAndOwnership(Path path, Set<PosixFilePermission> expectedPermissions) {
        logger.debug("Checking permissions and ownership of [" + path + "]");

        final String[] components = dockerShell.run("stat -c \"%U %G %A\" " + path).stdout.split("\\s+");

        final String username = components[0];
        final String group = components[1];
        final String permissions = components[2];

        // The final substring() is because we don't check the directory bit, and we
        // also don't want any SELinux security context indicator.
        Set<PosixFilePermission> actualPermissions = fromString(permissions.substring(1, 10));

        assertEquals("Permissions of " + path + " are wrong", expectedPermissions, actualPermissions);
        assertThat("File owner of " + path + " is wrong", username, equalTo("elasticsearch"));
        assertThat("File group of " + path + " is wrong", group, equalTo("root"));
    }

    /**
     * Waits for up to 20 seconds for a path to exist in the container.
     */
    public static void waitForPathToExist(Path path) throws InterruptedException {
        int attempt = 0;

        do {
            if (existsInContainer(path)) {
                return;
            }

            Thread.sleep(1000);
        } while (attempt++ < 20);

        fail(path + " failed to exist after 5000ms");
    }

    /**
     * Perform a variety of checks on an installation. If the current distribution is not OSS, additional checks are carried out.
     */
    public static void verifyContainerInstallation(Installation installation, Distribution distribution) {
        verifyOssInstallation(installation);
        if (distribution.flavor == Distribution.Flavor.DEFAULT) {
            verifyDefaultInstallation(installation);
        }
    }

    private static void verifyOssInstallation(Installation es) {
        dockerShell.run("id elasticsearch");
        dockerShell.run("getent group elasticsearch");

        final Shell.Result passwdResult = dockerShell.run("getent passwd elasticsearch");
        final String homeDir = passwdResult.stdout.trim().split(":")[5];
        assertThat(homeDir, equalTo("/usr/share/elasticsearch"));

        Stream.of(es.home, es.data, es.logs, es.config, es.plugins).forEach(dir -> assertPermissionsAndOwnership(dir, p775));

        Stream.of(es.modules).forEach(dir -> assertPermissionsAndOwnership(dir, p755));

        assertPermissionsAndOwnership(es.config("elasticsearch.keystore"), p660);

        Stream.of("elasticsearch.yml", "jvm.options", "log4j2.properties")
            .forEach(configFile -> assertPermissionsAndOwnership(es.config(configFile), p664));

        assertThat(dockerShell.run(es.bin("elasticsearch-keystore") + " list").stdout, containsString("keystore.seed"));

        Stream.of(es.bin, es.lib).forEach(dir -> assertPermissionsAndOwnership(dir, p755));

        Stream.of(
            "elasticsearch",
            "elasticsearch-cli",
            "elasticsearch-env",
            "elasticsearch-keystore",
            "elasticsearch-node",
            "elasticsearch-plugin",
            "elasticsearch-shard"
        ).forEach(executable -> assertPermissionsAndOwnership(es.bin(executable), p755));

        Stream.of("LICENSE.txt", "NOTICE.txt", "README.asciidoc").forEach(doc -> assertPermissionsAndOwnership(es.home.resolve(doc), p644));

        // nc is useful for checking network issues
        // zip/unzip are installed to help users who are working with certificates.
        Stream.of("nc", "unzip", "zip")
            .forEach(
                cliBinary -> assertTrue(
                    cliBinary + " ought to be available.",
                    dockerShell.runIgnoreExitCode("hash " + cliBinary).isSuccess()
                )
            );
    }

    private static void verifyDefaultInstallation(Installation es) {
        Stream.of(
            "elasticsearch-certgen",
            "elasticsearch-certutil",
            "elasticsearch-croneval",
            "elasticsearch-saml-metadata",
            "elasticsearch-setup-passwords",
            "elasticsearch-sql-cli",
            "elasticsearch-syskeygen",
            "elasticsearch-users",
            "x-pack-env",
            "x-pack-security-env",
            "x-pack-watcher-env"
        ).forEach(executable -> assertPermissionsAndOwnership(es.bin(executable), p755));

        // at this time we only install the current version of archive distributions, but if that changes we'll need to pass
        // the version through here
        assertPermissionsAndOwnership(es.bin("elasticsearch-sql-cli-" + getCurrentVersion() + ".jar"), p755);

        Stream.of("role_mapping.yml", "roles.yml", "users", "users_roles")
            .forEach(configFile -> assertPermissionsAndOwnership(es.config(configFile), p664));
    }

    public static void waitForElasticsearch(Installation installation) throws Exception {
        withLogging(() -> ServerUtils.waitForElasticsearch(installation));
    }

    public static void waitForElasticsearch(String status, String index, Installation installation, String username, String password)
        throws Exception {
        withLogging(() -> ServerUtils.waitForElasticsearch(status, index, installation, username, password));
    }

    /**
     * Runs the provided closure, and captures logging information if an exception is thrown.
     * @param r the closure to run
     * @throws Exception any exception encountered while running the closure are propagated.
     */
    private static <E extends Exception> void withLogging(CheckedRunnable<E> r) throws Exception {
        try {
            r.run();
        } catch (Exception e) {
            final Shell.Result logs = getContainerLogs();
            logger.warn("Elasticsearch container failed to start.\n\nStdout:\n" + logs.stdout + "\n\nStderr:\n" + logs.stderr);
            throw e;
        }
    }

    /**
     * @return The ID of the container that this class will be operating on.
     */
    public static String getContainerId() {
        return containerId;
    }

    public static JsonNode getJson(String path) throws Exception {
        final String pluginsResponse = makeRequest(Request.Get("http://localhost:9200/" + path));

        ObjectMapper mapper = new ObjectMapper();

        return mapper.readTree(pluginsResponse);
    }

    public static Map<String, String> getImageLabels(Distribution distribution) throws Exception {
        // The format below extracts the .Config.Labels value, and prints it as json. Without the json
        // modifier, a stringified Go map is printed instead, which isn't helpful.
        String labelsJson = sh.run("docker inspect -f '{{json .Config.Labels}}' " + getImageName(distribution)).stdout;

        ObjectMapper mapper = new ObjectMapper();

        final JsonNode jsonNode = mapper.readTree(labelsJson);

        Map<String, String> labels = new HashMap<>();

        jsonNode.fieldNames().forEachRemaining(field -> labels.put(field, jsonNode.get(field).asText()));

        return labels;
    }

    public static Shell.Result getContainerLogs() {
        return sh.run("docker logs " + containerId);
    }

    public static String getImageName(Distribution distribution) {
        return distribution.flavor.name + (distribution.packaging == Distribution.Packaging.DOCKER_UBI ? "-ubi8" : "") + ":test";
    }
}
