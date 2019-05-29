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

import com.github.jengelman.gradle.plugins.shadow.ShadowPlugin
import com.github.jengelman.gradle.plugins.shadow.tasks.ShadowJar
import groovy.transform.CompileDynamic
import groovy.transform.CompileStatic
import org.apache.commons.io.IOUtils
import org.eclipse.jgit.lib.Constants
import org.eclipse.jgit.lib.RepositoryBuilder
import org.elasticsearch.gradle.info.GlobalBuildInfoPlugin
import org.elasticsearch.gradle.info.GlobalInfoExtension
import org.elasticsearch.gradle.info.JavaHome
import org.elasticsearch.gradle.precommit.DependencyLicensesTask
import org.elasticsearch.gradle.precommit.PrecommitTasks
import org.elasticsearch.gradle.test.ErrorReportingTestListener
import org.elasticsearch.gradle.testclusters.ElasticsearchCluster
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
import org.gradle.api.artifacts.ModuleVersionIdentifier
import org.gradle.api.artifacts.ProjectDependency
import org.gradle.api.artifacts.ResolvedArtifact
import org.gradle.api.artifacts.dsl.RepositoryHandler
import org.gradle.api.artifacts.repositories.ArtifactRepository
import org.gradle.api.artifacts.repositories.IvyArtifactRepository
import org.gradle.api.artifacts.repositories.IvyPatternRepositoryLayout
import org.gradle.api.artifacts.repositories.MavenArtifactRepository
import org.gradle.api.credentials.HttpHeaderCredentials
import org.gradle.api.execution.TaskActionListener
import org.gradle.api.execution.TaskExecutionGraph
import org.gradle.api.file.CopySpec
import org.gradle.api.plugins.BasePlugin
import org.gradle.api.plugins.BasePluginConvention
import org.gradle.api.plugins.ExtraPropertiesExtension
import org.gradle.api.plugins.JavaPlugin
import org.gradle.api.plugins.JavaPluginExtension
import org.gradle.api.publish.PublishingExtension
import org.gradle.api.publish.maven.MavenPublication
import org.gradle.api.publish.maven.plugins.MavenPublishPlugin
import org.gradle.api.publish.maven.tasks.GenerateMavenPom
import org.gradle.api.tasks.SourceSet
import org.gradle.api.tasks.SourceSetContainer
import org.gradle.api.tasks.bundling.Jar
import org.gradle.api.tasks.compile.GroovyCompile
import org.gradle.api.tasks.compile.JavaCompile
import org.gradle.api.tasks.javadoc.Javadoc
import org.gradle.api.tasks.testing.Test
import org.gradle.api.tasks.testing.logging.TestLoggingContainer
import org.gradle.authentication.http.HttpHeaderAuthentication
import org.gradle.external.javadoc.CoreJavadocOptions
import org.gradle.internal.jvm.Jvm
import org.gradle.language.base.plugins.LifecycleBasePlugin
import org.gradle.process.CommandLineArgumentProvider
import org.gradle.process.ExecResult
import org.gradle.process.ExecSpec
import org.gradle.util.GradleVersion

import java.nio.charset.StandardCharsets
import java.time.ZoneOffset
import java.time.ZonedDateTime
import java.util.regex.Matcher

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
        try { minimumGradleVersion = IOUtils.toString(is, StandardCharsets.UTF_8.toString()) } finally { is.close() }
        if (GradleVersion.current() < GradleVersion.version(minimumGradleVersion.trim())) {
            throw new GradleException(
                    "Gradle ${minimumGradleVersion}+ is required to use elasticsearch.build plugin"
            )
        }
        project.pluginManager.apply('java')
        configureConfigurations(project)
        configureJars(project) // jar config must be added before info broker
        // these plugins add lots of info to our jars
        project.pluginManager.apply('nebula.info-broker')
        project.pluginManager.apply('nebula.info-basic')
        project.pluginManager.apply('nebula.info-java')
        project.pluginManager.apply('nebula.info-scm')
        project.pluginManager.apply('nebula.info-jar')

        // apply global test task failure listener
        project.rootProject.pluginManager.apply(TestFailureReportingPlugin)

        project.getTasks().create("buildResources", ExportElasticsearchBuildResourcesTask)

        setupSeed(project)
        configureRepositories(project)
        project.extensions.getByType(ExtraPropertiesExtension).set('versions', VersionProperties.versions)
        configureInputNormalization(project)
        configureSourceSets(project)
        configureCompile(project)
        configureJavadoc(project)
        configureSourcesJar(project)
        configurePomGeneration(project)
        configureTestTasks(project)
        configurePrecommit(project)
        configureDependenciesInfo(project)

        // Common config when running with a FIPS-140 runtime JVM
        // Need to do it here to support external plugins
        if (project == project.rootProject) {
            GlobalInfoExtension globalInfo = project.extensions.getByType(GlobalInfoExtension)

            // wait until global info is populated because we don't know if we are running in a fips jvm until execution time
            globalInfo.ready {
                project.subprojects { Project subproject ->
                    ExtraPropertiesExtension ext = subproject.extensions.getByType(ExtraPropertiesExtension)
                    // Common config when running with a FIPS-140 runtime JVM
                    if (ext.has('inFipsJvm') && ext.get('inFipsJvm')) {
                        subproject.tasks.withType(Test) { Test task ->
                            task.systemProperty 'javax.net.ssl.trustStorePassword', 'password'
                            task.systemProperty 'javax.net.ssl.keyStorePassword', 'password'
                        }
                        project.pluginManager.withPlugin("elasticsearch.testclusters") {
                            NamedDomainObjectContainer<ElasticsearchCluster> testClusters = subproject.extensions.getByName('testClusters') as NamedDomainObjectContainer<ElasticsearchCluster>
                            testClusters.all { ElasticsearchCluster cluster ->
                                cluster.systemProperty 'javax.net.ssl.trustStorePassword', 'password'
                                cluster.systemProperty 'javax.net.ssl.keyStorePassword', 'password'
                            }
                        }
                    }
                }
            }
        }
    }

    static void requireDocker(final Task task) {
        final Project rootProject = task.project.rootProject
        ExtraPropertiesExtension ext = rootProject.extensions.getByType(ExtraPropertiesExtension)

        if (rootProject.hasProperty('requiresDocker') == false) {
            /*
             * This is our first time encountering a task that requires Docker. We will add an extension that will let us track the tasks
             * that register as requiring Docker. We will add a delayed execution that when the task graph is ready if any such tasks are
             * in the task graph, then we check two things:
             *  - the Docker binary is available
             *  - we can execute a Docker command that requires privileges
             *
             *  If either of these fail, we fail the build.
             */

            // check if the Docker binary exists and record its path
            final List<String> maybeDockerBinaries = ['/usr/bin/docker', '/usr/local/bin/docker']
            final String dockerBinary = maybeDockerBinaries.find { it -> new File(it).exists() }

            final boolean buildDocker
            final String buildDockerProperty = System.getProperty("build.docker")
            if (buildDockerProperty == null) {
                buildDocker = dockerBinary != null
            } else if (buildDockerProperty == "true") {
                buildDocker = true
            } else if (buildDockerProperty == "false") {
                buildDocker = false
            } else {
                throw new IllegalArgumentException(
                        "expected build.docker to be unset or one of \"true\" or \"false\" but was [" + buildDockerProperty + "]")
            }

            ext.set('buildDocker', buildDocker)
            ext.set('requiresDocker', [])
            rootProject.gradle.taskGraph.whenReady { TaskExecutionGraph taskGraph ->
                final List<String> tasks = taskGraph.allTasks.intersect(ext.get('requiresDocker') as List<Task>).collect { "  ${it.path}".toString()}
                if (tasks.isEmpty() == false) {
                    /*
                     * There are tasks in the task graph that require Docker. Now we are failing because either the Docker binary does not
                     * exist or because execution of a privileged Docker command failed.
                     */
                    if (dockerBinary == null) {
                        final String message = String.format(
                                Locale.ROOT,
                                "Docker (checked [%s]) is required to run the following task%s: \n%s",
                                maybeDockerBinaries.join(","),
                                tasks.size() > 1 ? "s" : "",
                                tasks.join('\n'))
                        throwDockerRequiredException(message)
                    }

                    // we use a multi-stage Docker build, check the Docker version since 17.05
                    final ByteArrayOutputStream dockerVersionOutput = new ByteArrayOutputStream()
                    LoggedExec.exec(
                            rootProject,
                            { ExecSpec it ->
                                it.commandLine = [dockerBinary, '--version']
                                it.standardOutput = dockerVersionOutput
                            })
                    final String dockerVersion = dockerVersionOutput.toString().trim()
                    checkDockerVersionRecent(dockerVersion)

                    final ByteArrayOutputStream dockerImagesErrorOutput = new ByteArrayOutputStream()
                    // the Docker binary executes, check that we can execute a privileged command
                    final ExecResult dockerImagesResult = LoggedExec.exec(
                            rootProject,
                            { ExecSpec it ->
                                it.commandLine = [dockerBinary, "images"]
                                it.errorOutput = dockerImagesErrorOutput
                                it.ignoreExitValue = true
                            })

                    if (dockerImagesResult.exitValue != 0) {
                        final String message = String.format(
                                Locale.ROOT,
                                "a problem occurred running Docker from [%s] yet it is required to run the following task%s: \n%s\n" +
                                        "the problem is that Docker exited with exit code [%d] with standard error output [%s]",
                                dockerBinary,
                                tasks.size() > 1 ? "s" : "",
                                tasks.join('\n'),
                                dockerImagesResult.exitValue,
                                dockerImagesErrorOutput.toString().trim())
                        throwDockerRequiredException(message)
                    }

                }
            }
        }

        if (ext.get('buildDocker')) {
            (ext.get('requiresDocker') as List<Task>).add(task)
        } else {
            task.enabled = false
        }
    }

    protected static void checkDockerVersionRecent(String dockerVersion) {
        final Matcher matcher = dockerVersion =~ /Docker version (\d+\.\d+)\.\d+(?:-ce)?, build [0-9a-f]{7,40}/
        assert matcher.matches(): dockerVersion
        final dockerMajorMinorVersion = matcher.group(1)
        final String[] majorMinor = dockerMajorMinorVersion.split("\\.")
        if (Integer.parseInt(majorMinor[0]) < 17
                || (Integer.parseInt(majorMinor[0]) == 17 && Integer.parseInt(majorMinor[1]) < 5)) {
            final String message = String.format(
                    Locale.ROOT,
                    "building Docker images requires Docker version 17.05+ due to use of multi-stage builds yet was [%s]",
                    dockerVersion)
            throwDockerRequiredException(message)
        }
    }

    private static void throwDockerRequiredException(final String message) {
        throw new GradleException(
                message + "\nyou can address this by attending to the reported issue, "
                        + "removing the offending tasks from being executed, "
                        + "or by passing -Dbuild.docker=false")
    }

    /** Add a check before gradle execution phase which ensures java home for the given java version is set. */
    static void requireJavaHome(Task task, int version) {
        // use root project for global accounting
        Project rootProject = task.project.rootProject
        ExtraPropertiesExtension ext = rootProject.extensions.getByType(ExtraPropertiesExtension)

        if (rootProject.hasProperty('requiredJavaVersions') == false) {
            ext.set('requiredJavaVersions', [:])
            rootProject.gradle.taskGraph.whenReady({ TaskExecutionGraph taskGraph ->
                List<String> messages = []
                Map<Integer, List<Task>> requiredJavaVersions = (Map<Integer, List<Task>>) ext.get('requiredJavaVersions')
                for (Map.Entry<Integer, List<Task>> entry : requiredJavaVersions) {
                    List<JavaHome> javaVersions = ext.get('javaVersions') as List<JavaHome>
                    if (javaVersions.find { it.version == entry.key } != null) {
                        continue
                    }
                    List<String> tasks = entry.value.findAll { taskGraph.hasTask(it) }.collect { "  ${it.path}".toString() }
                    if (tasks.isEmpty() == false) {
                        messages.add("JAVA${entry.key}_HOME required to run tasks:\n${tasks.join('\n')}".toString())
                    }
                }
                if (messages.isEmpty() == false) {
                    throw new GradleException(messages.join('\n'))
                }
                ext.set('requiredJavaVersions', null) // reset to null to indicate the pre-execution checks have executed
            })
        } else if (ext.has('requiredJavaVersions') == false || ext.get('requiredJavaVersions') == null) {
            // check directly if the version is present since we are already executing
            List<JavaHome> javaVersions = ext.get('javaVersions') as List<JavaHome>
            if (javaVersions.find { it.version == version } == null) {
                throw new GradleException("JAVA${version}_HOME required to run task:\n${task}")
            }
        } else {
            (ext.get('requiredJavaVersions') as Map<Integer, List<Task>>).getOrDefault(version, []).add(task)
        }
    }

    /** A convenience method for getting java home for a version of java and requiring that version for the given task to execute */
    static String getJavaHome(final Task task, final int version) {
        requireJavaHome(task, version)
        List<JavaHome> javaVersions = task.project.property('javaVersions') as List<JavaHome>
        return javaVersions.find { it.version == version }.javaHome.absolutePath
    }

    /** Return the configuration name used for finding transitive deps of the given dependency. */
    private static String transitiveDepConfigName(String groupId, String artifactId, String version) {
        return "_transitive_${groupId}_${artifactId}_${version}"
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
            if (configuration.name.startsWith('_transitive_')) {
                // don't force transitive configurations to not conflict with themselves, since
                // we just have them to find *what* transitive deps exist
                return
            }
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

                // also create a configuration just for this dependency version, so that later
                // we can determine which transitive dependencies it has
                String depConfig = transitiveDepConfigName(dep.group, dep.name, dep.version)
                if (project.configurations.findByName(depConfig) == null) {
                    project.configurations.create(depConfig)
                    project.dependencies.add(depConfig, "${dep.group}:${dep.name}:${dep.version}")
                }
            }
        }

        project.configurations.getByName(JavaPlugin.COMPILE_CONFIGURATION_NAME).dependencies.all(disableTransitiveDeps)
        project.configurations.getByName(JavaPlugin.TEST_COMPILE_CONFIGURATION_NAME).dependencies.all(disableTransitiveDeps)
        project.configurations.getByName(JavaPlugin.COMPILE_ONLY_CONFIGURATION_NAME).dependencies.all(disableTransitiveDeps)

        project.plugins.withType(ShadowPlugin).whenPluginAdded {
            Configuration bundle = project.configurations.create('bundle')
            bundle.dependencies.all(disableTransitiveDeps)
        }
    }

    /** Adds repositories used by ES dependencies */
    static void configureRepositories(Project project) {
        project.getRepositories().all { repository ->
            if (repository instanceof MavenArtifactRepository) {
                final MavenArtifactRepository maven = (MavenArtifactRepository) repository
                assertRepositoryURIUsesHttps(maven, project, maven.getUrl())
                repository.getArtifactUrls().each { uri -> assertRepositoryURIUsesHttps(maven, project, uri) }
            } else if (repository instanceof IvyArtifactRepository) {
                final IvyArtifactRepository ivy = (IvyArtifactRepository) repository
                assertRepositoryURIUsesHttps(ivy, project, ivy.getUrl())
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
            repos.maven { MavenArtifactRepository repo ->
                repo.name = 'lucene-snapshots'
                repo.url = "https://s3.amazonaws.com/download.elasticsearch.org/lucenesnapshots/${revision}"
            }
        }
    }

    private static void assertRepositoryURIUsesHttps(final ArtifactRepository repository, final Project project, final URI uri) {
        if (uri != null && uri.toURL().getProtocol().equals("http")) {
            throw new GradleException("repository [${repository.name}] on project with path [${project.path}] is using http for artifacts on [${uri.toURL()}]")
        }
    }

    /**
     * Returns a closure which can be used with a MavenPom for fixing problems with gradle generated poms.
     *
     * <ul>
     *     <li>Remove transitive dependencies. We currently exclude all artifacts explicitly instead of using wildcards
     *         as Ivy incorrectly translates POMs with * excludes to Ivy XML with * excludes which results in the main artifact
     *         being excluded as well (see https://issues.apache.org/jira/browse/IVY-1531). Note that Gradle 2.14+ automatically
     *         translates non-transitive dependencies to * excludes. We should revisit this when upgrading Gradle.</li>
     *     <li>Set compile time deps back to compile from runtime (known issue with maven-publish plugin)</li>
     * </ul>
     */
    @CompileDynamic
    private static Closure fixupDependencies(Project project) {
        return { XmlProvider xml ->
            // first find if we have dependencies at all, and grab the node
            NodeList depsNodes = xml.asNode().get('dependencies')
            if (depsNodes.isEmpty()) {
                return
            }

            // check each dependency for any transitive deps
            for (Node depNode : depsNodes.get(0).children()) {
                String groupId = depNode.get('groupId').get(0).text()
                String artifactId = depNode.get('artifactId').get(0).text()
                String version = depNode.get('version').get(0).text()

                // fix deps incorrectly marked as runtime back to compile time deps
                // see https://discuss.gradle.org/t/maven-publish-plugin-generated-pom-making-dependency-scope-runtime/7494/4
                boolean isCompileDep = project.configurations.compile.allDependencies.find { dep ->
                    dep.name == depNode.artifactId.text()
                }
                if (depNode.scope.text() == 'runtime' && isCompileDep) {
                    depNode.scope*.value = 'compile'
                }

                // remove any exclusions added by gradle, they contain wildcards and systems like ivy have bugs with wildcards
                // see https://github.com/elastic/elasticsearch/issues/24490
                NodeList exclusionsNode = depNode.get('exclusions')
                if (exclusionsNode.size() > 0) {
                    depNode.remove(exclusionsNode.get(0))
                }

                // collect the transitive deps now that we know what this dependency is
                String depConfig = transitiveDepConfigName(groupId, artifactId, version)
                Configuration configuration = project.configurations.findByName(depConfig)
                if (configuration == null) {
                    continue // we did not make this dep non-transitive
                }
                Set<ResolvedArtifact> artifacts = configuration.resolvedConfiguration.resolvedArtifacts
                if (artifacts.size() <= 1) {
                    // this dep has no transitive deps (or the only artifact is itself)
                    continue
                }

                // we now know we have something to exclude, so add exclusions for all artifacts except the main one
                Node exclusions = depNode.appendNode('exclusions')
                for (ResolvedArtifact artifact : artifacts) {
                    ModuleVersionIdentifier moduleVersionIdentifier = artifact.moduleVersion.id;
                    String depGroupId = moduleVersionIdentifier.group
                    String depArtifactId = moduleVersionIdentifier.name
                    // add exclusions for all artifacts except the main one
                    if (depGroupId != groupId || depArtifactId != artifactId) {
                        Node exclusion = exclusions.appendNode('exclusion')
                        exclusion.appendNode('groupId', depGroupId)
                        exclusion.appendNode('artifactId', depArtifactId)
                    }
                }
            }
        }
    }

    /**Configuration generation of maven poms. */
    static void configurePomGeneration(Project project) {
        // Only works with  `enableFeaturePreview('STABLE_PUBLISHING')`
        // https://github.com/gradle/gradle/issues/5696#issuecomment-396965185
        project.tasks.withType(GenerateMavenPom.class) { GenerateMavenPom generatePOMTask ->
            // The GenerateMavenPom task is aggressive about setting the destination, instead of fighting it,
            // just make a copy.
            ExtraPropertiesExtension ext = generatePOMTask.extensions.getByType(ExtraPropertiesExtension)
            ext.set('pomFileName', null)
            generatePOMTask.doLast {
                project.copy { CopySpec spec ->
                    spec.from generatePOMTask.destination
                    spec.into "${project.buildDir}/distributions"
                    spec.rename {
                        ext.has('pomFileName') && ext.get('pomFileName') == null ?
                            "${project.convention.getPlugin(BasePluginConvention).archivesBaseName}-${project.version}.pom" :
                            ext.get('pomFileName')
                    }
                }
            }
            // build poms with assemble (if the assemble task exists)
            Task assemble = project.tasks.findByName('assemble')
            if (assemble && assemble.enabled) {
                assemble.dependsOn(generatePOMTask)
            }
        }
        project.plugins.withType(MavenPublishPlugin).whenPluginAdded {
            PublishingExtension publishing = project.extensions.getByType(PublishingExtension)
            publishing.publications.all { MavenPublication publication -> // we only deal with maven
                // add exclusions to the pom directly, for each of the transitive deps of this project's deps
                publication.pom.withXml(fixupDependencies(project))
            }
            project.plugins.withType(ShadowPlugin).whenPluginAdded {
                MavenPublication publication = publishing.publications.maybeCreate('nebula', MavenPublication)
                publication.with {
                    artifacts = [ project.tasks.getByName('shadowJar') ]
                }
            }
        }
    }

    /**
     * Add dependencies that we are going to bundle to the compile classpath.
     */
    static void configureSourceSets(Project project) {
        project.plugins.withType(ShadowPlugin).whenPluginAdded {
            ['main', 'test'].each {name ->
                SourceSet sourceSet = project.extensions.getByType(SourceSetContainer).findByName(name)
                if (sourceSet != null) {
                    sourceSet.compileClasspath += project.configurations.getByName('bundle')
                }
            }
        }
    }

    /**
     * Apply runtime classpath input normalization so that changes in JAR manifests don't break build cacheability
     */
    static void configureInputNormalization(Project project) {
        project.normalization.runtimeClasspath.ignore('META-INF/MANIFEST.MF')
    }

    /** Adds compiler settings to the project */
    static void configureCompile(Project project) {
        ExtraPropertiesExtension ext = project.extensions.getByType(ExtraPropertiesExtension)
        ext.set('compactProfile', 'full')

        project.extensions.getByType(JavaPluginExtension).sourceCompatibility = ext.get('minimumRuntimeVersion') as JavaVersion
        project.extensions.getByType(JavaPluginExtension).targetCompatibility = ext.get('minimumRuntimeVersion') as JavaVersion

        project.afterEvaluate {
            File compilerJavaHome = ext.get('compilerJavaHome') as File

            project.tasks.withType(JavaCompile) { JavaCompile compileTask ->
                final JavaVersion targetCompatibilityVersion = JavaVersion.toVersion(compileTask.targetCompatibility)
                // we only fork if the Gradle JDK is not the same as the compiler JDK
                if (compilerJavaHome.canonicalPath == Jvm.current().javaHome.canonicalPath) {
                    compileTask.options.fork = false
                } else {
                    compileTask.options.fork = true
                    compileTask.options.forkOptions.javaHome = compilerJavaHome
                }
                /*
                 * -path because gradle will send in paths that don't always exist.
                 * -missing because we have tons of missing @returns and @param.
                 * -serial because we don't use java serialization.
                 */
                // don't even think about passing args with -J-xxx, oracle will ask you to submit a bug report :)
                // fail on all javac warnings
                compileTask.options.compilerArgs << '-Werror' << '-Xlint:all,-path,-serial,-options,-deprecation,-try' << '-Xdoclint:all' << '-Xdoclint:-missing'

                // either disable annotation processor completely (default) or allow to enable them if an annotation processor is explicitly defined
                if (compileTask.options.compilerArgs.contains("-processor") == false) {
                    compileTask.options.compilerArgs << '-proc:none'
                }

                compileTask.options.encoding = 'UTF-8'
                compileTask.options.incremental = true

                // TODO: use native Gradle support for --release when available (cf. https://github.com/gradle/gradle/issues/2510)
                compileTask.options.compilerArgs << '--release' << targetCompatibilityVersion.majorVersion
            }
            // also apply release flag to groovy, which is used in build-tools
            project.tasks.withType(GroovyCompile) { GroovyCompile compileTask ->
                // we only fork if the Gradle JDK is not the same as the compiler JDK
                if (compilerJavaHome.canonicalPath == Jvm.current().javaHome.canonicalPath) {
                    compileTask.options.fork = false
                } else {
                    compileTask.options.fork = true
                    compileTask.options.forkOptions.javaHome = compilerJavaHome
                    compileTask.options.compilerArgs << '--release' << JavaVersion.toVersion(compileTask.targetCompatibility).majorVersion
                }
            }
        }
    }

    static void configureJavadoc(Project project) {
        // remove compiled classes from the Javadoc classpath: http://mail.openjdk.java.net/pipermail/javadoc-dev/2018-January/000400.html
        final List<File> classes = new ArrayList<>()
        project.tasks.withType(JavaCompile) { JavaCompile javaCompile ->
            classes.add(javaCompile.destinationDir)
        }
        project.tasks.withType(Javadoc) { Javadoc javadoc ->
            File compilerJavaHome = project.extensions.getByType(ExtraPropertiesExtension).get('compilerJavaHome') as File
            javadoc.executable = new File(compilerJavaHome, 'bin/javadoc')
            javadoc.classpath = javadoc.getClasspath().filter { f ->
                return classes.contains(f) == false
            }
            /*
             * Generate docs using html5 to suppress a warning from `javadoc`
             * that the default will change to html5 in the future.
             */
            (javadoc.options as CoreJavadocOptions).addBooleanOption('html5', true)
        }
        configureJavadocJar(project)
    }

    /** Adds a javadocJar task to generate a jar containing javadocs. */
    static void configureJavadocJar(Project project) {
        Jar javadocJarTask = project.tasks.create('javadocJar', Jar)
        javadocJarTask.classifier = 'javadoc'
        javadocJarTask.group = 'build'
        javadocJarTask.description = 'Assembles a jar containing javadocs.'
        javadocJarTask.from(project.tasks.getByName(JavaPlugin.JAVADOC_TASK_NAME))
        project.tasks.getByName(BasePlugin.ASSEMBLE_TASK_NAME).dependsOn(javadocJarTask)
    }

    static void configureSourcesJar(Project project) {
        Jar sourcesJarTask = project.tasks.create('sourcesJar', Jar)
        sourcesJarTask.classifier = 'sources'
        sourcesJarTask.group = 'build'
        sourcesJarTask.description = 'Assembles a jar containing source files.'
        sourcesJarTask.from(project.extensions.getByType(SourceSetContainer).getByName(SourceSet.MAIN_SOURCE_SET_NAME).allSource)
        project.tasks.getByName(BasePlugin.ASSEMBLE_TASK_NAME).dependsOn(sourcesJarTask)
    }

    /** Adds additional manifest info to jars */
    static void configureJars(Project project) {
        ExtraPropertiesExtension ext = project.extensions.getByType(ExtraPropertiesExtension)
        ext.set('licenseFile',  null)
        ext.set('noticeFile', null)
        project.tasks.withType(Jar) { Jar jarTask ->
            // we put all our distributable files under distributions
            jarTask.destinationDir = new File(project.buildDir, 'distributions')
            // fixup the jar manifest
            jarTask.doFirst {
                // this doFirst is added before the info plugin, therefore it will run
                // after the doFirst added by the info plugin, and we can override attributes
                JavaVersion compilerJavaVersion = ext.get('compilerJavaVersion') as JavaVersion
                jarTask.manifest.attributes(
                        'X-Compile-Elasticsearch-Version': VersionProperties.elasticsearch,
                        'X-Compile-Lucene-Version': VersionProperties.lucene,
                        'X-Compile-Elasticsearch-Snapshot': VersionProperties.isElasticsearchSnapshot(),
                        'Build-Date': ZonedDateTime.now(ZoneOffset.UTC),
                        'Build-Java-Version': compilerJavaVersion)
                if (jarTask.manifest.attributes.containsKey('Change') == false) {
                    jarTask.logger.warn('Building without git revision id.')
                    jarTask.manifest.attributes('Change': 'Unknown')
                } else {
                    /*
                     * The info-scm plugin assumes that if GIT_COMMIT is set it was set by Jenkins to the commit hash for this build.
                     * However, that assumption is wrong as this build could be a sub-build of another Jenkins build for which GIT_COMMIT
                     * is the commit hash for that build. Therefore, if GIT_COMMIT is set we calculate the commit hash ourselves.
                     */
                    if (System.getenv("GIT_COMMIT") != null) {
                        final String hash = new RepositoryBuilder().findGitDir(project.buildDir).build().resolve(Constants.HEAD).name
                        final String shortHash = hash?.substring(0, 7)
                        jarTask.manifest.attributes('Change': shortHash)
                    }
                }
                // Force manifest entries that change by nature to a constant to be able to compare builds more effectively
                if (System.properties.getProperty("build.compare_friendly", "false") == "true") {
                    jarTask.manifest.getAttributes().clear()
                }
            }

            // add license/notice files
            project.afterEvaluate {
                if (ext.has('licenseFile') == false || ext.get('licenseFile') == null || ext.has('noticeFile') == false || ext.get('noticeFile') == null) {
                    throw new GradleException("Must specify license and notice file for project ${project.path}")
                }

                File licenseFile = ext.get('licenseFile') as File
                File noticeFile = ext.get('noticeFile') as File

                jarTask.metaInf { CopySpec spec ->
                    spec.from(licenseFile.parent) { CopySpec from ->
                        from.include licenseFile.name
                        from.rename { 'LICENSE.txt' }
                    }
                    spec.from(noticeFile.parent) { CopySpec from ->
                        from.include noticeFile.name
                        from.rename { 'NOTICE.txt' }
                    }
                }
            }
        }
        project.plugins.withType(ShadowPlugin).whenPluginAdded {
            /*
             * When we use the shadow plugin we entirely replace the
             * normal jar with the shadow jar so we no longer want to run
             * the jar task.
             */
            project.tasks.getByName(JavaPlugin.JAR_TASK_NAME).enabled = false
            project.tasks.getByName('shadowJar').configure { ShadowJar shadowJar ->
                /*
                 * Replace the default "shadow" classifier with null
                 * which will leave the classifier off of the file name.
                 */
                shadowJar.classifier = null
                /*
                 * Not all cases need service files merged but it is
                 * better to be safe
                 */
                shadowJar.mergeServiceFiles()
                /*
                 * Bundle dependencies of the "bundled" configuration.
                 */
                shadowJar.configurations = [project.configurations.getByName('bundle')]
            }
            // Make sure we assemble the shadow jar
            project.tasks.getByName(BasePlugin.ASSEMBLE_TASK_NAME).dependsOn project.tasks.getByName('shadowJar')
            project.artifacts.add('apiElements', project.tasks.getByName('shadowJar'))
        }
    }

    static void configureTestTasks(Project project) {
        ExtraPropertiesExtension ext = project.extensions.getByType(ExtraPropertiesExtension)

        // Default test task should run only unit tests
        project.tasks.withType(Test).matching { Test task -> task.name == 'test' }.all { Test task ->
            task.include '**/*Tests.class'
        }

        // none of this stuff is applicable to the `:buildSrc` project tests
        if (project.path != ':build-tools') {
            File heapdumpDir = new File(project.buildDir, 'heapdump')

            project.tasks.withType(Test) { Test test ->
                File testOutputDir = new File(test.reports.junitXml.getDestination(), "output")

                ErrorReportingTestListener listener = new ErrorReportingTestListener(test.testLogging, testOutputDir)
                test.extensions.add(ErrorReportingTestListener, 'errorReportingTestListener', listener)
                test.addTestOutputListener(listener)
                test.addTestListener(listener)

                /*
                 * We use lazy-evaluated strings in order to configure system properties whose value will not be known until
                 * execution time (e.g. cluster port numbers). Adding these via the normal DSL doesn't work as these get treated
                 * as task inputs and therefore Gradle attempts to snapshot them before/after task execution. This fails due
                 * to the GStrings containing references to non-serializable objects.
                 *
                 * We bypass this by instead passing this system properties vi a CommandLineArgumentProvider. This has the added
                 * side-effect that these properties are NOT treated as inputs, therefore they don't influence things like the
                 * build cache key or up to date checking.
                 */
                SystemPropertyCommandLineArgumentProvider nonInputProperties = new SystemPropertyCommandLineArgumentProvider()

                test.doFirst {
                    project.mkdir(testOutputDir)
                    project.mkdir(heapdumpDir)
                    project.mkdir(test.workingDir)

                    if (project.property('inFipsJvm')) {
                        nonInputProperties.systemProperty('runtime.java', "${-> (ext.get('runtimeJavaVersion') as JavaVersion).getMajorVersion()}FIPS")
                    } else {
                        nonInputProperties.systemProperty('runtime.java', "${-> (ext.get('runtimeJavaVersion') as JavaVersion).getMajorVersion()}")
                    }
                }

                test.jvmArgumentProviders.add(nonInputProperties)
                test.extensions.add('nonInputProperties', nonInputProperties)

                test.executable = "${ext.get('runtimeJavaHome')}/bin/java"
                test.workingDir = project.file("${project.buildDir}/testrun/${test.name}")
                test.maxParallelForks = project.rootProject.extensions.getByType(ExtraPropertiesExtension).get('defaultParallel') as Integer

                test.exclude '**/*$*.class'

                test.jvmArgs "-Xmx${System.getProperty('tests.heap.size', '512m')}",
                        "-Xms${System.getProperty('tests.heap.size', '512m')}",
                        '-XX:+HeapDumpOnOutOfMemoryError',
                        "-XX:HeapDumpPath=$heapdumpDir",
                        '--illegal-access=warn'


                if (System.getProperty('tests.jvm.argline')) {
                    test.jvmArgs System.getProperty('tests.jvm.argline').split(" ")
                }

                if (Boolean.parseBoolean(System.getProperty('tests.asserts', 'true'))) {
                    test.jvmArgs '-ea', '-esa'
                }

                // we use './temp' since this is per JVM and tests are forbidden from writing to CWD
                test.systemProperties 'gradle.dist.lib': new File(project.class.location.toURI()).parent,
                        'java.io.tmpdir': './temp',
                        'java.awt.headless': 'true',
                        'tests.gradle': 'true',
                        'tests.artifact': project.name,
                        'tests.task': test.path,
                        'tests.security.manager': 'true',
                        'jna.nosys': 'true'

                // ignore changing test seed when build is passed -Dignore.tests.seed for cacheability experimentation
                if (System.getProperty('ignore.tests.seed') != null) {
                    nonInputProperties.systemProperty('tests.seed', project.property('testSeed'))
                } else {
                    test.systemProperty('tests.seed', project.property('testSeed'))
                }

                // don't track these as inputs since they contain absolute paths and break cache relocatability
                nonInputProperties.systemProperty('gradle.worker.jar', "${project.gradle.getGradleUserHomeDir()}/caches/${project.gradle.gradleVersion}/workerMain/gradle-worker.jar")
                nonInputProperties.systemProperty('gradle.user.home', project.gradle.getGradleUserHomeDir())

                nonInputProperties.systemProperty('compiler.java', "${-> (ext.get('compilerJavaVersion') as JavaVersion).getMajorVersion()}")

                // TODO: remove setting logging level via system property
                test.systemProperty 'tests.logger.level', 'WARN'
                System.getProperties().each { key, value ->
                    if ((key.toString().startsWith('tests.') || key.toString().startsWith('es.'))) {
                        test.systemProperty key.toString(), value
                    }
                }

                // TODO: remove this once ctx isn't added to update script params in 7.0
                test.systemProperty 'es.scripting.update.ctx_in_params', 'false'

                test.testLogging { TestLoggingContainer logging ->
                    logging.showExceptions = true
                    logging.showCauses = true
                    logging.exceptionFormat = 'full'
                }

                project.plugins.withType(ShadowPlugin).whenPluginAdded {
                    // Test against a shadow jar if we made one
                    test.classpath -= project.tasks.getByName('compileJava').outputs.files
                    test.classpath += project.tasks.getByName('shadowJar').outputs.files

                    test.dependsOn project.tasks.getByName('shadowJar')
                }
            }
        }
    }

    private static configurePrecommit(Project project) {
        Task precommit = PrecommitTasks.create(project, true)
        project.tasks.getByName(LifecycleBasePlugin.CHECK_TASK_NAME).dependsOn(precommit)
        project.tasks.getByName(JavaPlugin.TEST_TASK_NAME).mustRunAfter(precommit)
        // only require dependency licenses for non-elasticsearch deps
        (project.tasks.getByName('dependencyLicenses') as DependencyLicensesTask).dependencies = project.configurations.getByName(JavaPlugin.RUNTIME_CONFIGURATION_NAME).fileCollection { Dependency dependency ->
            dependency.group.startsWith('org.elasticsearch') == false
        } - project.configurations.getByName(JavaPlugin.COMPILE_ONLY_CONFIGURATION_NAME)
        project.plugins.withType(ShadowPlugin).whenPluginAdded {
            (project.tasks.getByName('dependencyLicenses') as DependencyLicensesTask).dependencies += project.configurations.getByName('bundle').fileCollection { Dependency dependency ->
                dependency.group.startsWith('org.elasticsearch') == false
            }
        }
    }

    private static configureDependenciesInfo(Project project) {
        DependenciesInfoTask deps = project.tasks.create("dependenciesInfo", DependenciesInfoTask)
        deps.runtimeConfiguration = project.configurations.getByName(JavaPlugin.RUNTIME_CONFIGURATION_NAME)
        project.plugins.withType(ShadowPlugin).whenPluginAdded {
            deps.runtimeConfiguration = project.configurations.create('infoDeps')
            deps.runtimeConfiguration.extendsFrom(project.configurations.getByName(JavaPlugin.RUNTIME_CONFIGURATION_NAME), project.configurations.getByName('bundle'))
        }
        deps.compileOnlyConfiguration = project.configurations.getByName(JavaPlugin.COMPILE_ONLY_CONFIGURATION_NAME)
        project.afterEvaluate {
            deps.mappings = (project.tasks.getByName('dependencyLicenses') as DependencyLicensesTask).mappings
        }
    }

    /**
     * Pins the test seed at configuration time so it isn't different on every
     * {@link Test} execution. This is useful if random
     * decisions in one run of {@linkplain Test} influence the
     * outcome of subsequent runs. Pinning the seed up front like this makes
     * the reproduction line from one run be useful on another run.
     */
    static String setupSeed(Project project) {
        ExtraPropertiesExtension ext = project.rootProject.extensions.getByType(ExtraPropertiesExtension)
        if (ext.has('testSeed')) {
            /* Skip this if we've already pinned the testSeed. It is important
             * that this checks the rootProject so that we know we've only ever
             * initialized one time. */
            return ext.get('testSeed')
        }

        String testSeed = System.getProperty('tests.seed')
        if (testSeed == null) {
            long seed = new Random(System.currentTimeMillis()).nextLong()
            testSeed = Long.toUnsignedString(seed, 16).toUpperCase(Locale.ROOT)
        }

        ext.set('testSeed', testSeed)
        return testSeed
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
}
