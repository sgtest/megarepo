/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.gradle.docker;

import org.elasticsearch.gradle.LoggedExec;
import org.gradle.api.DefaultTask;
import org.gradle.api.GradleException;
import org.gradle.api.file.DirectoryProperty;
import org.gradle.api.file.RegularFileProperty;
import org.gradle.api.logging.Logger;
import org.gradle.api.logging.Logging;
import org.gradle.api.model.ObjectFactory;
import org.gradle.api.provider.ListProperty;
import org.gradle.api.provider.MapProperty;
import org.gradle.api.provider.Property;
import org.gradle.api.tasks.Input;
import org.gradle.api.tasks.InputDirectory;
import org.gradle.api.tasks.OutputFile;
import org.gradle.api.tasks.PathSensitive;
import org.gradle.api.tasks.PathSensitivity;
import org.gradle.api.tasks.TaskAction;
import org.gradle.process.ExecOperations;
import org.gradle.workers.WorkAction;
import org.gradle.workers.WorkParameters;
import org.gradle.workers.WorkerExecutor;

import javax.inject.Inject;
import java.io.IOException;
import java.util.Arrays;

public class DockerBuildTask extends DefaultTask {
    private static final Logger LOGGER = Logging.getLogger(DockerBuildTask.class);

    private final WorkerExecutor workerExecutor;
    private final RegularFileProperty markerFile;
    private final DirectoryProperty dockerContext;

    private String[] tags;
    private boolean pull = true;
    private boolean noCache = true;
    private String[] baseImages;
    private MapProperty<String, String> buildArgs;

    @Inject
    public DockerBuildTask(WorkerExecutor workerExecutor, ObjectFactory objectFactory) {
        this.workerExecutor = workerExecutor;
        this.markerFile = objectFactory.fileProperty();
        this.dockerContext = objectFactory.directoryProperty();
        this.buildArgs = objectFactory.mapProperty(String.class, String.class);

        this.markerFile.set(getProject().getLayout().getBuildDirectory().file("markers/" + this.getName() + ".marker"));
    }

    @TaskAction
    public void build() {
        workerExecutor.noIsolation().submit(DockerBuildAction.class, params -> {
            params.getDockerContext().set(dockerContext);
            params.getMarkerFile().set(markerFile);
            params.getTags().set(Arrays.asList(tags));
            params.getPull().set(pull);
            params.getNoCache().set(noCache);
            params.getBaseImages().set(Arrays.asList(baseImages));
            params.getBuildArgs().set(buildArgs);
        });
    }

    @InputDirectory
    @PathSensitive(PathSensitivity.RELATIVE)
    public DirectoryProperty getDockerContext() {
        return dockerContext;
    }

    @Input
    public String[] getTags() {
        return tags;
    }

    public void setTags(String[] tags) {
        this.tags = tags;
    }

    @Input
    public boolean isPull() {
        return pull;
    }

    public void setPull(boolean pull) {
        this.pull = pull;
    }

    @Input
    public boolean isNoCache() {
        return noCache;
    }

    public void setNoCache(boolean noCache) {
        this.noCache = noCache;
    }

    @Input
    public String[] getBaseImages() {
        return baseImages;
    }

    public void setBaseImages(String[] baseImages) {
        this.baseImages = baseImages;
    }

    @Input
    public MapProperty<String, String> getBuildArgs() {
        return buildArgs;
    }

    public void setBuildArgs(MapProperty<String, String> buildArgs) {
        this.buildArgs = buildArgs;
    }

    @OutputFile
    public RegularFileProperty getMarkerFile() {
        return markerFile;
    }

    public abstract static class DockerBuildAction implements WorkAction<Parameters> {
        private final ExecOperations execOperations;

        @Inject
        public DockerBuildAction(ExecOperations execOperations) {
            this.execOperations = execOperations;
        }

        /**
         * Wraps `docker pull` in a retry loop, to try and provide some resilience against
         * transient errors
         * @param baseImage the image to pull.
         */
        private void pullBaseImage(String baseImage) {
            final int maxAttempts = 10;

            for (int attempt = 1; attempt <= maxAttempts; attempt++) {
                try {
                    LoggedExec.exec(execOperations, spec -> {
                        spec.executable("docker");
                        spec.args("pull");
                        spec.args(baseImage);
                    });

                    return;
                } catch (Exception e) {
                    LOGGER.warn("Attempt {}/{} to pull Docker base image {} failed", attempt, maxAttempts, baseImage);
                }
            }

            // If we successfully ran `docker pull` above, we would have returned before this point.
            throw new GradleException("Failed to pull Docker base image [" + baseImage + "], all attempts failed");
        }

        @Override
        public void execute() {
            final Parameters parameters = getParameters();

            if (parameters.getPull().get()) {
                parameters.getBaseImages().get().forEach(this::pullBaseImage);
            }

            LoggedExec.exec(execOperations, spec -> {
                spec.executable("docker");

                spec.args("build", parameters.getDockerContext().get().getAsFile().getAbsolutePath());

                if (parameters.getNoCache().get()) {
                    spec.args("--no-cache");
                }

                parameters.getTags().get().forEach(tag -> spec.args("--tag", tag));

                parameters.getBuildArgs().get().forEach((k, v) -> spec.args("--build-arg", k + "=" + v));
            });

            try {
                parameters.getMarkerFile().getAsFile().get().createNewFile();
            } catch (IOException e) {
                throw new RuntimeException("Failed to create marker file", e);
            }
        }
    }

    interface Parameters extends WorkParameters {
        DirectoryProperty getDockerContext();

        RegularFileProperty getMarkerFile();

        ListProperty<String> getTags();

        Property<Boolean> getPull();

        Property<Boolean> getNoCache();

        ListProperty<String> getBaseImages();

        MapProperty<String, String> getBuildArgs();
    }
}
