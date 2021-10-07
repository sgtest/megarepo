/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.gradle.internal.conventions;

import org.gradle.api.Plugin;
import org.gradle.api.Project;
import org.gradle.api.provider.Provider;
import org.gradle.initialization.layout.BuildLayout;

import javax.inject.Inject;
import java.io.File;

public class VersionPropertiesPlugin implements Plugin<Project> {

    private BuildLayout buildLayout;

    @Inject
    VersionPropertiesPlugin(BuildLayout buildLayout) {
        this.buildLayout = buildLayout;
    }

    @Override
    public void apply(Project project) {
        // Register the service if not done yet
        File infoPath = new File(buildLayout.getRootDirectory(), "build-tools-internal");
        Provider<VersionPropertiesBuildService> serviceProvider = project.getGradle().getSharedServices()
                .registerIfAbsent("versions", VersionPropertiesBuildService.class, spec -> {
            spec.getParameters().getInfoPath().set(infoPath);
        });
        project.getExtensions().add("versions", serviceProvider.forUseAtConfigurationTime().get().getProperties());
    }
}
