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
package org.elasticsearch.gradle

import com.github.jengelman.gradle.plugins.shadow.ShadowBasePlugin
import com.github.jengelman.gradle.plugins.shadow.ShadowExtension
import com.github.jengelman.gradle.plugins.shadow.ShadowJavaPlugin
import com.github.jengelman.gradle.plugins.shadow.tasks.ShadowJar
import groovy.transform.CompileStatic
import nebula.plugin.info.InfoBrokerPlugin
import org.apache.commons.io.IOUtils
import org.elasticsearch.gradle.info.BuildParams
import org.elasticsearch.gradle.info.GlobalBuildInfoPlugin
import org.elasticsearch.gradle.plugin.PluginBuildPlugin
import org.elasticsearch.gradle.precommit.DependencyLicensesTask
import org.elasticsearch.gradle.precommit.PrecommitTasks
import org.elasticsearch.gradle.test.ErrorReportingTestListener
import org.elasticsearch.gradle.testclusters.ElasticsearchCluster
import org.elasticsearch.gradle.testclusters.TestClustersPlugin
import org.elasticsearch.gradle.util.GradleUtils
import org.gradle.api.Action
import org.gradle.api.GradleException
import org.gradle.api.InvalidUserDataException
import org.gradle.api.JavaVersion
import org.gradle.api.NamedDomainObjectContainer
import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.api.Task
import org.gradle.api.XmlProvider
import org.gradle.api.artifacts.Configuration
import org.gradle.api.artifacts.Dependency
import org.gradle.api.artifacts.ModuleDependency
import org.gradle.api.artifacts.ProjectDependency
import org.gradle.api.artifacts.dsl.RepositoryHandler
import org.gradle.api.artifacts.repositories.ExclusiveContentRepository
import org.gradle.api.artifacts.repositories.IvyArtifactRepository
import org.gradle.api.artifacts.repositories.IvyPatternRepositoryLayout
import org.gradle.api.artifacts.repositories.MavenArtifactRepository
import org.gradle.api.credentials.HttpHeaderCredentials
import org.gradle.api.execution.TaskActionListener
import org.gradle.api.file.CopySpec
import org.gradle.api.plugins.BasePlugin
import org.gradle.api.plugins.BasePluginConvention
import org.gradle.api.plugins.ExtraPropertiesExtension
import org.gradle.api.plugins.JavaPlugin
import org.gradle.api.publish.PublishingExtension
import org.gradle.api.publish.maven.MavenPublication
import org.gradle.api.publish.maven.plugins.MavenPublishPlugin
import org.gradle.api.publish.maven.tasks.GenerateMavenPom
import org.gradle.api.tasks.SourceSet
import org.gradle.api.tasks.SourceSetContainer
import org.gradle.api.tasks.TaskProvider
import org.gradle.api.tasks.bundling.Jar
import org.gradle.api.tasks.compile.JavaCompile
import org.gradle.api.tasks.javadoc.Javadoc
import org.gradle.api.tasks.testing.Test
import org.gradle.authentication.http.HttpHeaderAuthentication
import org.gradle.external.javadoc.CoreJavadocOptions
import org.gradle.internal.jvm.Jvm
import org.gradle.language.base.plugins.LifecycleBasePlugin
import org.gradle.util.GradleVersion

import java.nio.charset.StandardCharsets
import java.nio.file.Files

import static org.elasticsearch.gradle.util.GradleUtils.maybeConfigure

/**
 * Encapsulates build configuration for elasticsearch projects.
 */
@CompileStatic
class BuildPlugin implements Plugin<Project> {

    @Override
    void apply(Project project) {
        // make sure the global build info plugin is applied to the root project
        project.rootProject.pluginManager.apply(GlobalBuildInfoPlugin)

        if (project.pluginManager.hasPlugin('elasticsearch.standalone-rest-test')) {
            throw new InvalidUserDataException('elasticsearch.standalone-test, '
                    + 'elasticsearch.standalone-rest-test, and elasticsearch.build '
                    + 'are mutually exclusive')
        }
        String minimumGradleVersion = null
        InputStream is = getClass().getResourceAsStream("/minimumGradleVersion")
        try {
            minimumGradleVersion = IOUtils.toString(is, StandardCharsets.UTF_8.toString())
        } finally {
            is.close()
        }
        if (GradleVersion.current() < GradleVersion.version(minimumGradleVersion.trim())) {
            throw new GradleException(
                    "Gradle ${minimumGradleVersion}+ is required to use elasticsearch.build plugin"
            )
        }
        project.pluginManager.apply('elasticsearch.java')
        project.pluginManager.apply('elasticsearch.publish')

        // apply global test task failure listener
        project.rootProject.pluginManager.apply(TestFailureReportingPlugin)

        project.getTasks().register("buildResources", ExportElasticsearchBuildResourcesTask)

        configureRepositories(project)
        project.extensions.getByType(ExtraPropertiesExtension).set('versions', VersionProperties.versions)
        configurePrecommit(project)
        configureDependenciesInfo(project)
        configureFips140(project)
    }

    static void configureFips140(Project project) {
        // Common config when running with a FIPS-140 runtime JVM
        if (inFipsJvm()) {
            ExportElasticsearchBuildResourcesTask buildResources = project.tasks.getByName('buildResources') as ExportElasticsearchBuildResourcesTask
            File securityProperties = buildResources.copy("fips_java.security")
            File securityPolicy = buildResources.copy("fips_java.policy")
            File bcfksKeystore = buildResources.copy("cacerts.bcfks")
            // This configuration can be removed once system modules are available
            GradleUtils.maybeCreate(project.configurations, 'extraJars') {
                project.dependencies.add('extraJars', "org.bouncycastle:bc-fips:1.0.1")
                project.dependencies.add('extraJars', "org.bouncycastle:bctls-fips:1.0.9")
            }
            project.pluginManager.withPlugin("elasticsearch.testclusters") {
                NamedDomainObjectContainer<ElasticsearchCluster> testClusters = project.extensions.findByName(TestClustersPlugin.EXTENSION_NAME) as NamedDomainObjectContainer<ElasticsearchCluster>
                testClusters.all { ElasticsearchCluster cluster ->
                    for (File dep : project.getConfigurations().getByName("extraJars").getFiles()){
                        cluster.extraJarFile(dep)
                    }
                    cluster.extraConfigFile("fips_java.security", securityProperties)
                    cluster.extraConfigFile("fips_java.policy", securityPolicy)
                    cluster.extraConfigFile("cacerts.bcfks", bcfksKeystore)
                    cluster.systemProperty('java.security.properties', '=${ES_PATH_CONF}/fips_java.security')
                    cluster.systemProperty('java.security.policy', '=${ES_PATH_CONF}/fips_java.policy')
                    cluster.systemProperty('javax.net.ssl.trustStore', '${ES_PATH_CONF}/cacerts.bcfks')
                    cluster.systemProperty('javax.net.ssl.trustStorePassword', 'password')
                    cluster.systemProperty('javax.net.ssl.keyStorePassword', 'password')
                    cluster.systemProperty('javax.net.ssl.keyStoreType', 'BCFKS')
                }
            }
            project.tasks.withType(Test).configureEach { Test task ->
                task.dependsOn(buildResources)
                task.systemProperty('javax.net.ssl.trustStorePassword', 'password')
                task.systemProperty('javax.net.ssl.keyStorePassword', 'password')
                task.systemProperty('javax.net.ssl.trustStoreType', 'BCFKS')
                // Using the key==value format to override default JVM security settings and policy
                // see also: https://docs.oracle.com/javase/8/docs/technotes/guides/security/PolicyFiles.html
                task.systemProperty('java.security.properties', String.format(Locale.ROOT, "=%s", securityProperties.toString()))
                task.systemProperty('java.security.policy', String.format(Locale.ROOT, "=%s", securityPolicy.toString()))
                task.systemProperty('javax.net.ssl.trustStore', bcfksKeystore.toString())
            }

        }
    }

    /**
     * Makes dependencies non-transitive.
     *
     * Gradle allows setting all dependencies as non-transitive very easily.
     * Sadly this mechanism does not translate into maven pom generation. In order
     * to effectively make the pom act as if it has no transitive dependencies,
     * we must exclude each transitive dependency of each direct dependency.
     *
     * Determining the transitive deps of a dependency which has been resolved as
     * non-transitive is difficult because the process of resolving removes the
     * transitive deps. To sidestep this issue, we create a configuration per
     * direct dependency version. This specially named and unique configuration
     * will contain all of the transitive dependencies of this particular
     * dependency. We can then use this configuration during pom generation
     * to iterate the transitive dependencies and add excludes.
     */
    static void configureConfigurations(Project project) {
        // we want to test compileOnly deps!
        project.configurations.getByName(JavaPlugin.TEST_COMPILE_CONFIGURATION_NAME).extendsFrom(project.configurations.getByName(JavaPlugin.COMPILE_ONLY_CONFIGURATION_NAME))

        // we are not shipping these jars, we act like dumb consumers of these things
        if (project.path.startsWith(':test:fixtures') || project.path == ':build-tools') {
            return
        }
        // fail on any conflicting dependency versions
        project.configurations.all({ Configuration configuration ->
            if (configuration.name.endsWith('Fixture')) {
                // just a self contained test-fixture configuration, likely transitive and hellacious
                return
            }
            configuration.resolutionStrategy {
                failOnVersionConflict()
            }
        })

        // force all dependencies added directly to compile/testCompile to be non-transitive, except for ES itself
        Closure disableTransitiveDeps = { Dependency dep ->
            if (dep instanceof ModuleDependency && !(dep instanceof ProjectDependency)
                    && dep.group.startsWith('org.elasticsearch') == false) {
                dep.transitive = false
            }
        }

        project.configurations.getByName(JavaPlugin.COMPILE_CONFIGURATION_NAME).dependencies.all(disableTransitiveDeps)
        project.configurations.getByName(JavaPlugin.TEST_COMPILE_CONFIGURATION_NAME).dependencies.all(disableTransitiveDeps)
        project.configurations.getByName(JavaPlugin.COMPILE_ONLY_CONFIGURATION_NAME).dependencies.all(disableTransitiveDeps)
        project.configurations.getByName(JavaPlugin.RUNTIME_ONLY_CONFIGURATION_NAME).dependencies.all(disableTransitiveDeps)
    }

    /** Adds repositories used by ES dependencies */
    static void configureRepositories(Project project) {
        project.getRepositories().all { repository ->
            if (repository instanceof MavenArtifactRepository) {
                final MavenArtifactRepository maven = (MavenArtifactRepository) repository
                assertRepositoryURIIsSecure(maven.name, project.path, maven.getUrl())
                repository.getArtifactUrls().each { uri -> assertRepositoryURIIsSecure(maven.name, project.path, uri) }
            } else if (repository instanceof IvyArtifactRepository) {
                final IvyArtifactRepository ivy = (IvyArtifactRepository) repository
                assertRepositoryURIIsSecure(ivy.name, project.path, ivy.getUrl())
            }
        }
        RepositoryHandler repos = project.repositories
        if (System.getProperty('repos.mavenLocal') != null) {
            // with -Drepos.mavenLocal=true we can force checking the local .m2 repo which is
            // useful for development ie. bwc tests where we install stuff in the local repository
            // such that we don't have to pass hardcoded files to gradle
            repos.mavenLocal()
        }
        repos.jcenter()
        repos.ivy { IvyArtifactRepository repo ->
            repo.name = 'elasticsearch'
            repo.url = 'https://artifacts.elastic.co/downloads'
            repo.patternLayout { IvyPatternRepositoryLayout layout ->
                layout.artifact 'elasticsearch/[module]-[revision](-[classifier]).[ext]'
            }
            // this header is not a credential but we hack the capability to send this header to avoid polluting our download stats
            repo.credentials(HttpHeaderCredentials, { HttpHeaderCredentials creds ->
                creds.name = 'X-Elastic-No-KPI'
                creds.value = '1'
            } as Action<HttpHeaderCredentials>)
            repo.authentication.create('header', HttpHeaderAuthentication)
        }
        repos.maven { MavenArtifactRepository repo ->
            repo.name = 'elastic'
            repo.url = 'https://artifacts.elastic.co/maven'
        }
        String luceneVersion = VersionProperties.lucene
        if (luceneVersion.contains('-snapshot')) {
            // extract the revision number from the version with a regex matcher
            List<String> matches = (luceneVersion =~ /\w+-snapshot-([a-z0-9]+)/).getAt(0) as List<String>
            String revision = matches.get(1)
            MavenArtifactRepository luceneRepo = repos.maven { MavenArtifactRepository repo ->
                repo.name = 'lucene-snapshots'
                repo.url = "https://s3.amazonaws.com/download.elasticsearch.org/lucenesnapshots/${revision}"
            }
            repos.exclusiveContent { ExclusiveContentRepository exclusiveRepo ->
                exclusiveRepo.filter {
                    it.includeVersionByRegex(/org\.apache\.lucene/, '.*', ".*-snapshot-${revision}")
                }
                exclusiveRepo.forRepositories(luceneRepo)
            }
        }
    }

    static void assertRepositoryURIIsSecure(final String repositoryName, final String projectPath, final URI uri) {
        if (uri != null && ["file", "https", "s3"].contains(uri.getScheme()) == false) {
            final String message = String.format(
                    Locale.ROOT,
                    "repository [%s] on project with path [%s] is not using a secure protocol for artifacts on [%s]",
                    repositoryName,
                    projectPath,
                    uri.toURL())
            throw new GradleException(message)
        }
    }

    private static configurePrecommit(Project project) {
        TaskProvider precommit = PrecommitTasks.create(project, true)
        project.tasks.named(LifecycleBasePlugin.CHECK_TASK_NAME).configure { it.dependsOn(precommit) }
        project.tasks.named(JavaPlugin.TEST_TASK_NAME).configure { it.mustRunAfter(precommit) }
        // only require dependency licenses for non-elasticsearch deps
        project.tasks.withType(DependencyLicensesTask).named('dependencyLicenses').configure {
            it.dependencies = project.configurations.getByName(JavaPlugin.RUNTIME_CLASSPATH_CONFIGURATION_NAME).fileCollection { Dependency dependency ->
                dependency.group.startsWith('org.elasticsearch') == false
            } - project.configurations.getByName(JavaPlugin.COMPILE_ONLY_CONFIGURATION_NAME)
        }
    }

    private static configureDependenciesInfo(Project project) {
        project.tasks.register("dependenciesInfo", DependenciesInfoTask, { DependenciesInfoTask task ->
            task.runtimeConfiguration = project.configurations.getByName(JavaPlugin.RUNTIME_CLASSPATH_CONFIGURATION_NAME)
            task.compileOnlyConfiguration = project.configurations.getByName(JavaPlugin.COMPILE_ONLY_CONFIGURATION_NAME)
            task.getConventionMapping().map('mappings') {
                (project.tasks.getByName('dependencyLicenses') as DependencyLicensesTask).mappings
            }
        } as Action<DependenciesInfoTask>)
    }

    private static class TestFailureReportingPlugin implements Plugin<Project> {
        @Override
        void apply(Project project) {
            if (project != project.rootProject) {
                throw new IllegalStateException("${this.class.getName()} can only be applied to the root project.")
            }

            project.gradle.addListener(new TaskActionListener() {
                @Override
                void beforeActions(Task task) {

                }

                @Override
                void afterActions(Task task) {
                    if (task instanceof Test) {
                        ErrorReportingTestListener listener = task.extensions.findByType(ErrorReportingTestListener)
                        if (listener != null && listener.getFailedTests().size() > 0) {
                            task.logger.lifecycle("\nTests with failures:")
                            listener.getFailedTests().each {
                                task.logger.lifecycle(" - ${it.getFullName()}")
                            }
                        }
                    }
                }
            })
        }
    }

    private static inFipsJvm(){
        return Boolean.parseBoolean(System.getProperty("tests.fips.enabled"));
    }
}
