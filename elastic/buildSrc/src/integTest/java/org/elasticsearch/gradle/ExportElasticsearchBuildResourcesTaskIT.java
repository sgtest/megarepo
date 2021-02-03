package org.elasticsearch.gradle;

/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

import org.elasticsearch.gradle.test.GradleIntegrationTestCase;
import org.gradle.testkit.runner.BuildResult;

public class ExportElasticsearchBuildResourcesTaskIT extends GradleIntegrationTestCase {

    public static final String PROJECT_NAME = "elasticsearch-build-resources";

    public void testUpToDateWithSourcesConfigured() {
        getGradleRunner(PROJECT_NAME).withArguments("clean", "-s").build();

        BuildResult result = getGradleRunner(PROJECT_NAME).withArguments("buildResources", "-s", "-i").build();
        assertTaskSuccessful(result, ":buildResources");
        assertBuildFileExists(result, PROJECT_NAME, "build-tools-exported/checkstyle.xml");
        assertBuildFileExists(result, PROJECT_NAME, "build-tools-exported/checkstyle_suppressions.xml");

        result = getGradleRunner(PROJECT_NAME).withArguments("buildResources", "-s", "-i").build();
        assertTaskUpToDate(result, ":buildResources");
        assertBuildFileExists(result, PROJECT_NAME, "build-tools-exported/checkstyle.xml");
        assertBuildFileExists(result, PROJECT_NAME, "build-tools-exported/checkstyle_suppressions.xml");
    }

    public void testOutputAsInput() {
        BuildResult result = getGradleRunner(PROJECT_NAME).withArguments("clean", "sampleCopy", "-s", "-i").build();

        assertTaskSuccessful(result, ":sampleCopy");
        assertBuildFileExists(result, PROJECT_NAME, "sampleCopy/checkstyle.xml");
        assertBuildFileExists(result, PROJECT_NAME, "sampleCopy/checkstyle_suppressions.xml");
    }

    public void testIncorrectUsage() {
        assertOutputContains(
            getGradleRunner(PROJECT_NAME).withArguments("noConfigAfterExecution", "-s", "-i").buildAndFail().getOutput(),
            "buildResources can't be configured after the task ran"
        );
    }
}
