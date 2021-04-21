/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.gradle;

import org.gradle.api.DefaultTask;
import org.gradle.api.artifacts.Configuration;
import org.gradle.api.file.FileCollection;
import org.gradle.api.model.ObjectFactory;
import org.gradle.api.tasks.InputFiles;
import org.gradle.api.tasks.TaskAction;
import org.gradle.internal.deprecation.DeprecatableConfiguration;

import javax.inject.Inject;
import java.util.Collection;
import java.util.stream.Collectors;

import static org.elasticsearch.gradle.DistributionDownloadPlugin.DISTRO_EXTRACTED_CONFIG_PREFIX;
import static org.elasticsearch.gradle.internal.rest.compat.YamlRestCompatTestPlugin.BWC_MINOR_CONFIG_NAME;

public class ResolveAllDependencies extends DefaultTask {

    private final ObjectFactory objectFactory;

    Collection<Configuration> configs;

    @Inject
    public ResolveAllDependencies(ObjectFactory objectFactory) {
        this.objectFactory = objectFactory;
    }

    @InputFiles
    public FileCollection getResolvedArtifacts() {
        return objectFactory.fileCollection()
            .from(configs.stream().filter(ResolveAllDependencies::canBeResolved).collect(Collectors.toList()));
    }

    @TaskAction
    void resolveAll() {
        // do nothing, dependencies are resolved when snapshotting task inputs
    }

    private static boolean canBeResolved(Configuration configuration) {
        if (configuration.isCanBeResolved() == false) {
            return false;
        }
        if (configuration instanceof org.gradle.internal.deprecation.DeprecatableConfiguration) {
            var deprecatableConfiguration = (DeprecatableConfiguration) configuration;
            if (deprecatableConfiguration.canSafelyBeResolved() == false) {
                return false;
            }
        }
        return configuration.getName().startsWith(DISTRO_EXTRACTED_CONFIG_PREFIX) == false
            && configuration.getName().equals(BWC_MINOR_CONFIG_NAME) == false;
    }
}
