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
package org.elasticsearch.gradle;

import com.carrotsearch.randomizedtesting.annotations.ParametersFactory;
import org.apache.commons.io.FileUtils;
import org.elasticsearch.gradle.test.GradleIntegrationTestCase;
import org.gradle.testkit.runner.GradleRunner;
import org.junit.BeforeClass;
import org.junit.Rule;
import org.junit.rules.TemporaryFolder;

import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Objects;
import java.util.stream.Collectors;

public class BuildExamplePluginsIT extends GradleIntegrationTestCase {

    private static List<File> EXAMPLE_PLUGINS = Collections.unmodifiableList(
        Arrays.stream(
            Objects.requireNonNull(System.getProperty("test.build-tools.plugin.examples"))
                .split(File.pathSeparator)
        ).map(File::new).collect(Collectors.toList())
    );

    @Rule
    public TemporaryFolder tmpDir = new TemporaryFolder();

    public final File examplePlugin;

    public BuildExamplePluginsIT(File examplePlugin) {
        this.examplePlugin = examplePlugin;
    }

    @BeforeClass
    public static void assertProjectsExist() {
        assertEquals(
            EXAMPLE_PLUGINS,
            EXAMPLE_PLUGINS.stream().filter(File::exists).collect(Collectors.toList())
        );
    }

    @ParametersFactory
    public static Iterable<Object[]> parameters() {
        return EXAMPLE_PLUGINS
            .stream()
            .map(each -> new Object[] {each})
            .collect(Collectors.toList());
    }

    public void testCurrentExamplePlugin() throws IOException {
        FileUtils.copyDirectory(examplePlugin, tmpDir.getRoot());
        // just get rid of deprecation warnings
        Files.write(
            getTempPath("settings.gradle"),
            "enableFeaturePreview('STABLE_PUBLISHING')\n".getBytes(StandardCharsets.UTF_8)
        );

        adaptBuildScriptForTest();

        Files.write(
            tmpDir.newFile("NOTICE.txt").toPath(),
            "dummy test notice".getBytes(StandardCharsets.UTF_8)
        );

        GradleRunner.create()
            .withProjectDir(tmpDir.getRoot())
            .withArguments("clean", "check", "-s", "-i", "--warning-mode=all", "--scan")
            .withPluginClasspath()
            .build();
    }

    private void adaptBuildScriptForTest() throws IOException {
        // Add the local repo as a build script URL so we can pull in build-tools and apply the plugin under test
        // + is ok because we have no other repo and just want to pick up latest
        writeBuildScript(
            "buildscript {\n" +
                "    repositories {\n" +
                "        maven {\n" +
                "            url = '" + getLocalTestRepoPath() + "'\n" +
                "        }\n" +
                "    }\n" +
                "    dependencies {\n" +
                "        classpath \"org.elasticsearch.gradle:build-tools:+\"\n" +
                "    }\n" +
                "}\n"
        );
        // get the original file
        Files.readAllLines(getTempPath("build.gradle"), StandardCharsets.UTF_8)
            .stream()
            .map(line -> line + "\n")
            .forEach(this::writeBuildScript);
        // Add a repositories section to be able to resolve dependencies
        String luceneSnapshotRepo = "";
        String luceneSnapshotRevision = System.getProperty("test.lucene-snapshot-revision");
        if (luceneSnapshotRepo != null) {
            luceneSnapshotRepo =  "  maven {\n" +
                "    url \"http://s3.amazonaws.com/download.elasticsearch.org/lucenesnapshots/" + luceneSnapshotRevision + "\"\n" +
                "  }\n";
        }
        writeBuildScript("\n" +
                "repositories {\n" +
                "  maven {\n" +
                "    url \"" + getLocalTestRepoPath()  + "\"\n" +
                "  }\n" +
                luceneSnapshotRepo +
                "}\n"
        );
        Files.delete(getTempPath("build.gradle"));
        Files.move(getTempPath("build.gradle.new"), getTempPath("build.gradle"));
        System.err.print("Generated build script is:");
        Files.readAllLines(getTempPath("build.gradle")).forEach(System.err::println);
    }

    private Path getTempPath(String fileName) {
        return new File(tmpDir.getRoot(), fileName).toPath();
    }

    private Path writeBuildScript(String script) {
        try {
            Path path = getTempPath("build.gradle.new");
            return Files.write(
                path,
                script.getBytes(StandardCharsets.UTF_8),
                Files.exists(path) ? StandardOpenOption.APPEND : StandardOpenOption.CREATE_NEW
            );
        } catch (IOException e) {
            throw new RuntimeException(e);
        }
    }

    private String getLocalTestRepoPath() {
        String property = System.getProperty("test.local-test-repo-path");
        Objects.requireNonNull(property, "test.local-test-repo-path not passed to tests");
        File file = new File(property);
        assertTrue("Expected " + property + " to exist, but it did not!", file.exists());
        return file.getAbsolutePath();
    }

}
