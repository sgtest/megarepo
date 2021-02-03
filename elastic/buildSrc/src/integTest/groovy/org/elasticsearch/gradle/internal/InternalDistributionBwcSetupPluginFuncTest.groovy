/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.gradle.internal

import org.elasticsearch.gradle.fixtures.AbstractGitAwareGradleFuncTest
import org.gradle.testkit.runner.TaskOutcome
import spock.lang.Unroll

class InternalDistributionBwcSetupPluginFuncTest extends AbstractGitAwareGradleFuncTest {

    def setup() {
        internalBuild()
        buildFile << """
            apply plugin: 'elasticsearch.internal-distribution-bwc-setup'
        """
        execute("git branch origin/7.x", file("cloned"))
        execute("git branch origin/7.9", file("cloned"))
    }

    @Unroll
    def "builds distribution from branches via archives #expectedAssembleTaskName"() {
        when:
        def result = gradleRunner(":distribution:bwc:${bwcProject}:buildBwcDarwinTar",
                ":distribution:bwc:${bwcProject}:buildBwcOssDarwinTar",
                "-DtestRemoteRepo=" + remoteGitRepo,
                "-Dbwc.remote=origin",
                "-Dbwc.dist.version=${bwcDistVersion}-SNAPSHOT")
                .build()
        then:
        result.task(":distribution:bwc:${bwcProject}:buildBwcDarwinTar").outcome == TaskOutcome.SUCCESS
        result.task(":distribution:bwc:${bwcProject}:buildBwcOssDarwinTar").outcome == TaskOutcome.SUCCESS

        and: "assemble task triggered"
        assertOutputContains(result.output, "[$bwcDistVersion] > Task :distribution:archives:darwin-tar:${expectedAssembleTaskName}")
        assertOutputContains(result.output, "[$bwcDistVersion] > Task :distribution:archives:oss-darwin-tar:${expectedAssembleTaskName}")

        where:
        bwcDistVersion | bwcProject | expectedAssembleTaskName
        "7.9.1"        | "bugfix"   | "assemble"
        "7.11.0"       | "minor"    | "extractedAssemble"
    }

    def "bwc distribution archives can be resolved as bwc project artifact"() {
        setup:
        buildFile << """

        configurations {
            dists
        }

        dependencies {
            dists project(path: ":distribution:bwc:bugfix", configuration:"darwin-tar")
        }

        tasks.register("resolveDistributionArchive") {
            inputs.files(configurations.dists)
            doLast {
                configurations.dists.files.each {
                    println "distfile " + (it.absolutePath - project.rootDir.absolutePath)
                }
            }
        }
        """
        when:
        def result = gradleRunner(":resolveDistributionArchive",
                "-DtestRemoteRepo=" + remoteGitRepo,
                "-Dbwc.remote=origin")
                .build()
        then:
        result.task(":resolveDistributionArchive").outcome == TaskOutcome.SUCCESS
        result.task(":distribution:bwc:bugfix:buildBwcDarwinTar").outcome == TaskOutcome.SUCCESS

        and: "assemble task triggered"
        result.output.contains("[7.9.1] > Task :distribution:archives:darwin-tar:assemble")
        normalized(result.output)
                .contains("distfile /distribution/bwc/bugfix/build/bwc/checkout-7.9/distribution/archives/darwin-tar/" +
                        "build/distributions/elasticsearch-7.9.1-SNAPSHOT-darwin-x86_64.tar.gz")
    }

    def "bwc expanded distribution folder can be resolved as bwc project artifact"() {
        setup:
        buildFile << """

        configurations {
            expandedDist
        }

        dependencies {
            expandedDist project(path: ":distribution:bwc:minor", configuration:"expanded-darwin-tar")
        }

        tasks.register("resolveExpandedDistribution") {
            inputs.files(configurations.expandedDist)
            doLast {
                configurations.expandedDist.files.each {
                    println "expandedRootPath " + (it.absolutePath - project.rootDir.absolutePath)
                    it.eachFile { nested ->
                        println "nested folder " + (nested.absolutePath - project.rootDir.absolutePath)
                    }
                }
            }
        }
        """
        when:
        def result = gradleRunner(":resolveExpandedDistribution",
                "-DtestRemoteRepo=" + remoteGitRepo,
                "-Dbwc.remote=origin")
                .build()
        then:
        result.task(":resolveExpandedDistribution").outcome == TaskOutcome.SUCCESS
        result.task(":distribution:bwc:minor:buildBwcDarwinTar").outcome == TaskOutcome.SUCCESS
        and: "assemble task triggered"
        result.output.contains("[7.11.0] > Task :distribution:archives:darwin-tar:extractedAssemble")
        normalized(result.output)
                .contains("expandedRootPath /distribution/bwc/minor/build/bwc/checkout-7.x/" +
                        "distribution/archives/darwin-tar/build/install")
        normalized(result.output)
                .contains("nested folder /distribution/bwc/minor/build/bwc/checkout-7.x/" +
                        "distribution/archives/darwin-tar/build/install/elasticsearch-7.11.0-SNAPSHOT")
    }
}
