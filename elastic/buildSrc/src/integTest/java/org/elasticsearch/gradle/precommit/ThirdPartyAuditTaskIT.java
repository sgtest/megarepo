/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.gradle.precommit;

import org.elasticsearch.gradle.test.GradleIntegrationTestCase;
import org.gradle.testkit.runner.BuildResult;
import org.junit.Before;

import static org.elasticsearch.gradle.test.TestClasspathUtils.setupJarJdkClasspath;

public class ThirdPartyAuditTaskIT extends GradleIntegrationTestCase {

    @Before
    public void setUp() throws Exception {
        // Build the sample jars
        getGradleRunner("thirdPartyAudit").withArguments(":sample_jars:build", "-s").build();
        // propagate jdkjarhell jar
        setupJarJdkClasspath(getProjectDir("thirdPartyAudit"));
    }

    public void testElasticsearchIgnored() {
        BuildResult result = getGradleRunner("thirdPartyAudit").withArguments(
            ":clean",
            ":empty",
            "-s",
            "-PcompileOnlyGroup=elasticsearch.gradle:broken-log4j",
            "-PcompileOnlyVersion=0.0.1",
            "-PcompileGroup=elasticsearch.gradle:dummy-io",
            "-PcompileVersion=0.0.1"
        ).build();
        assertTaskNoSource(result, ":empty");
        assertNoDeprecationWarning(result);
    }

    public void testViolationFoundAndCompileOnlyIgnored() {
        BuildResult result = getGradleRunner("thirdPartyAudit").withArguments(
            ":clean",
            ":absurd",
            "-s",
            "-PcompileOnlyGroup=other.gradle:broken-log4j",
            "-PcompileOnlyVersion=0.0.1",
            "-PcompileGroup=other.gradle:dummy-io",
            "-PcompileVersion=0.0.1"
        ).buildAndFail();

        assertTaskFailed(result, ":absurd");
        assertOutputContains(result.getOutput(), "Classes with violations:", "  * TestingIO", "> Audit of third party dependencies failed");
        assertOutputMissing(result.getOutput(), "Missing classes:");
        assertNoDeprecationWarning(result);
    }

    public void testClassNotFoundAndCompileOnlyIgnored() {
        BuildResult result = getGradleRunner("thirdPartyAudit").withArguments(
            ":clean",
            ":absurd",
            "-s",
            "-PcompileGroup=other.gradle:broken-log4j",
            "-PcompileVersion=0.0.1",
            "-PcompileOnlyGroup=other.gradle:dummy-io",
            "-PcompileOnlyVersion=0.0.1"
        ).buildAndFail();
        assertTaskFailed(result, ":absurd");

        assertOutputContains(
            result.getOutput(),
            "Missing classes:",
            "  * org.apache.logging.log4j.LogManager",
            "> Audit of third party dependencies failed"
        );
        assertOutputMissing(result.getOutput(), "Classes with violations:");
        assertNoDeprecationWarning(result);
    }

    public void testJarHellWithJDK() {
        BuildResult result = getGradleRunner("thirdPartyAudit").withArguments(
            ":clean",
            ":absurd",
            "-s",
            "-PcompileGroup=other.gradle:jarhellJdk",
            "-PcompileVersion=0.0.1",
            "-PcompileOnlyGroup=other.gradle:dummy-io",
            "-PcompileOnlyVersion=0.0.1"
        ).buildAndFail();
        assertTaskFailed(result, ":absurd");

        assertOutputContains(
            result.getOutput(),
            "> Audit of third party dependencies failed:",
            "   Jar Hell with the JDK:",
            "    * java.lang.String"
        );
        assertOutputMissing(result.getOutput(), "Classes with violations:");
        assertNoDeprecationWarning(result);
    }

    public void testElasticsearchIgnoredWithViolations() {
        BuildResult result = getGradleRunner("thirdPartyAudit").withArguments(
            ":clean",
            ":absurd",
            "-s",
            "-PcompileOnlyGroup=elasticsearch.gradle:broken-log4j",
            "-PcompileOnlyVersion=0.0.1",
            "-PcompileGroup=elasticsearch.gradle:dummy-io",
            "-PcompileVersion=0.0.1"
        ).build();
        assertTaskNoSource(result, ":absurd");
        assertNoDeprecationWarning(result);
    }

}
