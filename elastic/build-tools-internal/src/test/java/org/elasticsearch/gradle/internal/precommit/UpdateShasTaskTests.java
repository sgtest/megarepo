/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.gradle.internal.precommit;

import org.apache.commons.io.FileUtils;
import org.gradle.api.GradleException;
import org.gradle.api.Project;
import org.gradle.api.artifacts.Dependency;
import org.gradle.api.file.FileCollection;
import org.gradle.api.plugins.JavaPlugin;
import org.gradle.api.tasks.TaskProvider;
import org.gradle.testfixtures.ProjectBuilder;
import org.junit.Before;
import org.junit.Rule;
import org.junit.Test;
import org.junit.rules.ExpectedException;

import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.security.NoSuchAlgorithmException;

import static org.hamcrest.CoreMatchers.containsString;
import static org.hamcrest.CoreMatchers.equalTo;
import static org.hamcrest.MatcherAssert.assertThat;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class UpdateShasTaskTests {

    public static final String GROOVY_JAR_REGEX = "groovy-\\d\\.\\d+\\.\\d+\\.jar";
    @Rule
    public ExpectedException expectedException = ExpectedException.none();

    private UpdateShasTask task;

    private Project project;

    private Dependency dependency;

    @Before
    public void prepare() throws IOException {
        project = createProject();
        task = createUpdateShasTask(project);
        dependency = project.getDependencies().localGroovy();

    }

    @Test
    public void whenDependencyDoesntExistThenShouldDeleteDependencySha() throws IOException, NoSuchAlgorithmException {
        File unusedSha = createFileIn(getLicensesDir(project), "test.sha1", "");
        task.updateShas();

        assertFalse(unusedSha.exists());
    }

    @Test
    public void whenDependencyExistsButShaNotThenShouldCreateNewShaFile() throws IOException, NoSuchAlgorithmException {
        project.getDependencies().add("implementation", dependency);

        getLicensesDir(project).mkdir();
        task.updateShas();
        Path groovySha = Files.list(getLicensesDir(project).toPath())
            .filter(p -> p.toFile().getName().matches(GROOVY_JAR_REGEX + ".sha1"))
            .findFirst()
            .get();
        assertTrue(groovySha.toFile().getName().startsWith("groovy"));
    }

    @Test
    public void whenDependencyAndWrongShaExistsThenShouldNotOverwriteShaFile() throws IOException, NoSuchAlgorithmException {
        project.getDependencies().add("implementation", dependency);
        File groovyJar = task.getParentTask()
            .getDependencies()
            .getFiles()
            .stream()
            .filter(f -> f.getName().matches(GROOVY_JAR_REGEX))
            .findFirst()
            .get();
        String groovyShaName = groovyJar.getName() + ".sha1";
        File groovySha = createFileIn(getLicensesDir(project), groovyShaName, "content");
        task.updateShas();
        assertThat(FileUtils.readFileToString(groovySha), equalTo("content"));
    }

    @Test
    public void whenLicensesDirDoesntExistThenShouldThrowException() throws IOException, NoSuchAlgorithmException {
        expectedException.expect(GradleException.class);
        expectedException.expectMessage(containsString("isn't a valid directory"));

        task.updateShas();
    }

    private Project createProject() {
        Project project = ProjectBuilder.builder().build();
        project.getPlugins().apply(JavaPlugin.class);

        return project;
    }

    private File getLicensesDir(Project project) {
        return getFile(project, "licenses");
    }

    private File getFile(Project project, String fileName) {
        return project.getProjectDir().toPath().resolve(fileName).toFile();
    }

    private File createFileIn(File parent, String name, String content) throws IOException {
        parent.mkdir();

        Path path = parent.toPath().resolve(name);
        File file = path.toFile();

        Files.write(path, content.getBytes(), StandardOpenOption.CREATE);

        return file;
    }

    private UpdateShasTask createUpdateShasTask(Project project) {
        UpdateShasTask task = project.getTasks().register("updateShas", UpdateShasTask.class).get();

        task.setParentTask(createDependencyLicensesTask(project));
        return task;
    }

    private TaskProvider<DependencyLicensesTask> createDependencyLicensesTask(Project project) {
        return project.getTasks()
            .register(
                "dependencyLicenses",
                DependencyLicensesTask.class,
                dependencyLicensesTask -> dependencyLicensesTask.setDependencies(getDependencies(project))
            );
    }

    private FileCollection getDependencies(Project project) {
        return project.getConfigurations().getByName("compileClasspath");
    }
}
