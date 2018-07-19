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
package org.elasticsearch.gradle.plugin

import com.github.jengelman.gradle.plugins.shadow.ShadowPlugin
import nebula.plugin.info.scm.ScmInfoPlugin
import org.elasticsearch.gradle.BuildPlugin
import org.elasticsearch.gradle.NoticeTask
import org.elasticsearch.gradle.test.RestIntegTestTask
import org.elasticsearch.gradle.test.RunTask
import org.gradle.api.InvalidUserDataException
import org.gradle.api.JavaVersion
import org.gradle.api.Project
import org.gradle.api.Task
import org.gradle.api.XmlProvider
import org.gradle.api.publish.maven.MavenPublication
import org.gradle.api.publish.maven.plugins.MavenPublishPlugin
import org.gradle.api.tasks.SourceSet
import org.gradle.api.tasks.bundling.Zip

import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.StandardCopyOption
import java.util.regex.Matcher
import java.util.regex.Pattern

/**
 * Encapsulates build configuration for an Elasticsearch plugin.
 */
public class PluginBuildPlugin extends BuildPlugin {

    @Override
    public void apply(Project project) {
        super.apply(project)
        project.plugins.withType(ShadowPlugin).whenPluginAdded {
            /*
             * We've not tested these plugins together and we're fairly sure
             * they aren't going to work properly as is *and* we're not really
             * sure *why* you'd want to shade stuff in plugins. So we throw an
             * exception here to make you come and read this comment. If you
             * have a need for shadow while building plugins then know that you
             * are probably going to have to fight with gradle for a while....
             */
            throw new InvalidUserDataException('elasticsearch.esplugin is not '
                    + 'compatible with com.github.johnrengelman.shadow');
        }
        configureDependencies(project)
        // this afterEvaluate must happen before the afterEvaluate added by integTest creation,
        // so that the file name resolution for installing the plugin will be setup
        project.afterEvaluate {
            boolean isXPackModule = project.path.startsWith(':x-pack:plugin')
            boolean isModule = project.path.startsWith(':modules:') || isXPackModule
            String name = project.pluginProperties.extension.name
            project.archivesBaseName = name

            if (project.pluginProperties.extension.hasClientJar) {
                // for plugins which work with the transport client, we copy the jar
                // file to a new name, copy the nebula generated pom to the same name,
                // and generate a different pom for the zip
                addClientJarPomGeneration(project)
                addClientJarTask(project)
            }
            // while the jar isn't normally published, we still at least build a pom of deps
            // in case it is published, for instance when other plugins extend this plugin
            configureJarPom(project)

            project.integTestCluster.dependsOn(project.bundlePlugin)
            project.tasks.run.dependsOn(project.bundlePlugin)
            if (isModule) {
                project.integTestCluster.module(project)
                project.tasks.run.clusterConfig.module(project)
                project.tasks.run.clusterConfig.distribution = System.getProperty(
                        'run.distribution', 'integ-test-zip'
                )
            } else {
                project.integTestCluster.plugin(project.path)
                project.tasks.run.clusterConfig.plugin(project.path)
            }

            if (isModule == false || isXPackModule) {
                addNoticeGeneration(project)
            }

            project.namingConventions {
                // Plugins declare integration tests as "Tests" instead of IT.
                skipIntegTestInDisguise = true
            }
        }
        createIntegTestTask(project)
        createBundleTask(project)
        project.configurations.getByName('default').extendsFrom(project.configurations.getByName('runtime'))
        project.tasks.create('run', RunTask) // allow running ES with this plugin in the foreground of a build
    }

    private static void configureDependencies(Project project) {
        project.dependencies {
            compileOnly "org.elasticsearch:elasticsearch:${project.versions.elasticsearch}"
            testCompile "org.elasticsearch.test:framework:${project.versions.elasticsearch}"
            // we "upgrade" these optional deps to provided for plugins, since they will run
            // with a full elasticsearch server that includes optional deps
            compileOnly "org.locationtech.spatial4j:spatial4j:${project.versions.spatial4j}"
            compileOnly "org.locationtech.jts:jts-core:${project.versions.jts}"
            compileOnly "org.apache.logging.log4j:log4j-api:${project.versions.log4j}"
            compileOnly "org.apache.logging.log4j:log4j-core:${project.versions.log4j}"
            compileOnly "org.elasticsearch:jna:${project.versions.jna}"
        }
    }

    /** Adds an integTest task which runs rest tests */
    private static void createIntegTestTask(Project project) {
        RestIntegTestTask integTest = project.tasks.create('integTest', RestIntegTestTask.class)
        integTest.mustRunAfter(project.precommit, project.test)
        project.integTestCluster.distribution = System.getProperty('tests.distribution', 'integ-test-zip')
        project.check.dependsOn(integTest)
    }

    /**
     * Adds a bundlePlugin task which builds the zip containing the plugin jars,
     * metadata, properties, and packaging files
     */
    private static void createBundleTask(Project project) {
        File pluginMetadata = project.file('src/main/plugin-metadata')

        // create a task to build the properties file for this plugin
        PluginPropertiesTask buildProperties = project.tasks.create('pluginProperties', PluginPropertiesTask.class)

        // add the plugin properties and metadata to test resources, so unit tests can
        // know about the plugin (used by test security code to statically initialize the plugin in unit tests)
        SourceSet testSourceSet = project.sourceSets.test
        testSourceSet.output.dir(buildProperties.descriptorOutput.parentFile, builtBy: 'pluginProperties')
        testSourceSet.resources.srcDir(pluginMetadata)

        // create the actual bundle task, which zips up all the files for the plugin
        Zip bundle = project.tasks.create(name: 'bundlePlugin', type: Zip, dependsOn: [project.jar, buildProperties]) {
            from(buildProperties.descriptorOutput.parentFile) {
                // plugin properties file
                include(buildProperties.descriptorOutput.name)
            }
            from pluginMetadata // metadata (eg custom security policy)
            from project.jar // this plugin's jar
            from project.configurations.runtime - project.configurations.compileOnly // the dep jars
            // extra files for the plugin to go into the zip
            from('src/main/packaging') // TODO: move all config/bin/_size/etc into packaging
            from('src/main') {
                include 'config/**'
                include 'bin/**'
            }
        }
        project.assemble.dependsOn(bundle)

        // also make the zip available as a configuration (used when depending on this project)
        project.configurations.create('zip')
        project.artifacts.add('zip', bundle)
    }

    /** Adds a task to move jar and associated files to a "-client" name. */
    protected static void addClientJarTask(Project project) {
        Task clientJar = project.tasks.create('clientJar')
        clientJar.dependsOn(project.jar, project.tasks.generatePomFileForClientJarPublication, project.javadocJar, project.sourcesJar)
        clientJar.doFirst {
            Path jarFile = project.jar.outputs.files.singleFile.toPath()
            String clientFileName = jarFile.fileName.toString().replace(project.version, "client-${project.version}")
            Files.copy(jarFile, jarFile.resolveSibling(clientFileName), StandardCopyOption.REPLACE_EXISTING)

            String clientPomFileName = clientFileName.replace('.jar', '.pom')
            Files.copy(
                    project.tasks.generatePomFileForClientJarPublication.outputs.files.singleFile.toPath(),
                    jarFile.resolveSibling(clientPomFileName),
                    StandardCopyOption.REPLACE_EXISTING
            )

            String sourcesFileName = jarFile.fileName.toString().replace('.jar', '-sources.jar')
            String clientSourcesFileName = clientFileName.replace('.jar', '-sources.jar')
            Files.copy(jarFile.resolveSibling(sourcesFileName), jarFile.resolveSibling(clientSourcesFileName),
                    StandardCopyOption.REPLACE_EXISTING)

            String javadocFileName = jarFile.fileName.toString().replace('.jar', '-javadoc.jar')
            String clientJavadocFileName = clientFileName.replace('.jar', '-javadoc.jar')
            Files.copy(jarFile.resolveSibling(javadocFileName), jarFile.resolveSibling(clientJavadocFileName),
                    StandardCopyOption.REPLACE_EXISTING)
        }
        project.assemble.dependsOn(clientJar)
    }

    static final Pattern GIT_PATTERN = Pattern.compile(/git@([^:]+):([^\.]+)\.git/)

    /** Find the reponame. */
    static String urlFromOrigin(String origin) {
        if (origin == null) {
            return null // best effort, the url doesnt really matter, it is just required by maven central
        }
        if (origin.startsWith('https')) {
            return origin
        }
        Matcher matcher = GIT_PATTERN.matcher(origin)
        if (matcher.matches()) {
            return "https://${matcher.group(1)}/${matcher.group(2)}"
        } else {
            return origin // best effort, the url doesnt really matter, it is just required by maven central
        }
    }

    /** Adds nebula publishing task to generate a pom file for the plugin. */
    protected static void addClientJarPomGeneration(Project project) {
        project.plugins.apply(MavenPublishPlugin.class)

        project.publishing {
            publications {
                clientJar(MavenPublication) {
                    from project.components.java
                    artifactId = project.pluginProperties.extension.name + '-client'
                    pom.withXml { XmlProvider xml ->
                        Node root = xml.asNode()
                        root.appendNode('name', project.pluginProperties.extension.name)
                        root.appendNode('description', project.pluginProperties.extension.description)
                        root.appendNode('url', urlFromOrigin(project.scminfo.origin))
                        Node scmNode = root.appendNode('scm')
                        scmNode.appendNode('url', project.scminfo.origin)
                    }
                }
            }
        }
    }

    /** Configure the pom for the main jar of this plugin */
    protected static void configureJarPom(Project project) {
        project.plugins.apply(ScmInfoPlugin.class)
        project.plugins.apply(MavenPublishPlugin.class)

        project.publishing {
            publications {
                nebula(MavenPublication) {
                    artifactId project.pluginProperties.extension.name
                }
            }
        }
    }

    protected void addNoticeGeneration(Project project) {
        File licenseFile = project.pluginProperties.extension.licenseFile
        if (licenseFile != null) {
            project.bundlePlugin.from(licenseFile.parentFile) {
                include(licenseFile.name)
                rename { 'LICENSE.txt' }
            }
        }
        File noticeFile = project.pluginProperties.extension.noticeFile
        if (noticeFile != null) {
            NoticeTask generateNotice = project.tasks.create('generateNotice', NoticeTask.class)
            generateNotice.inputFile = noticeFile
            project.bundlePlugin.from(generateNotice)
        }
    }
}
