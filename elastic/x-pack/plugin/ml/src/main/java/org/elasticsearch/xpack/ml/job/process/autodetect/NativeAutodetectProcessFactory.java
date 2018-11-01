/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.job.process.autodetect;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.EsRejectedExecutionException;
import org.elasticsearch.core.internal.io.IOUtils;
import org.elasticsearch.env.Environment;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.ml.MachineLearning;
import org.elasticsearch.xpack.ml.process.NativeController;
import org.elasticsearch.xpack.ml.process.ProcessPipes;
import org.elasticsearch.xpack.ml.job.process.autodetect.output.AutodetectResultsParser;
import org.elasticsearch.xpack.ml.job.process.autodetect.output.AutodetectStateProcessor;
import org.elasticsearch.xpack.ml.job.process.autodetect.params.AutodetectParams;
import org.elasticsearch.xpack.ml.utils.NamedPipeHelper;

import java.io.IOException;
import java.nio.file.Path;
import java.time.Duration;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.concurrent.ExecutorService;

public class NativeAutodetectProcessFactory implements AutodetectProcessFactory {

    private static final Logger LOGGER = LogManager.getLogger(NativeAutodetectProcessFactory.class);
    private static final NamedPipeHelper NAMED_PIPE_HELPER = new NamedPipeHelper();
    public static final Duration PROCESS_STARTUP_TIMEOUT = Duration.ofSeconds(10);

    private final Client client;
    private final Environment env;
    private final Settings settings;
    private final NativeController nativeController;
    private final ClusterService clusterService;

    public NativeAutodetectProcessFactory(Environment env, Settings settings, NativeController nativeController, Client client,
                                          ClusterService clusterService) {
        this.env = Objects.requireNonNull(env);
        this.settings = Objects.requireNonNull(settings);
        this.nativeController = Objects.requireNonNull(nativeController);
        this.client = client;
        this.clusterService = clusterService;
    }

    @Override
    public AutodetectProcess createAutodetectProcess(Job job,
                                                     AutodetectParams params,
                                                     ExecutorService executorService,
                                                     Runnable onProcessCrash) {
        List<Path> filesToDelete = new ArrayList<>();
        ProcessPipes processPipes = new ProcessPipes(env, NAMED_PIPE_HELPER, AutodetectBuilder.AUTODETECT, job.getId(),
                true, false, true, true, params.modelSnapshot() != null,
                !AutodetectBuilder.DONT_PERSIST_MODEL_STATE_SETTING.get(settings));
        createNativeProcess(job, params, processPipes, filesToDelete);
        boolean includeTokensField = MachineLearning.CATEGORIZATION_TOKENIZATION_IN_JAVA
                && job.getAnalysisConfig().getCategorizationFieldName() != null;
        // The extra 1 is the control field
        int numberOfFields = job.allInputFields().size() + (includeTokensField ? 1 : 0) + 1;

        AutodetectStateProcessor stateProcessor = new AutodetectStateProcessor(client, job.getId());
        AutodetectResultsParser resultsParser = new AutodetectResultsParser();
        NativeAutodetectProcess autodetect = new NativeAutodetectProcess(
                job.getId(), processPipes.getLogStream().get(), processPipes.getProcessInStream().get(),
                processPipes.getProcessOutStream().get(), processPipes.getRestoreStream().orElse(null), numberOfFields,
                filesToDelete, resultsParser, onProcessCrash);
        try {
            autodetect.start(executorService, stateProcessor, processPipes.getPersistStream().get());
            return autodetect;
        } catch (EsRejectedExecutionException e) {
            try {
                IOUtils.close(autodetect);
            } catch (IOException ioe) {
                LOGGER.error("Can't close autodetect", ioe);
            }
            throw e;
        }
    }

    private void createNativeProcess(Job job, AutodetectParams autodetectParams, ProcessPipes processPipes,
                                     List<Path> filesToDelete) {
        try {

            Settings updatedSettings = Settings.builder()
                .put(settings)
                .put(AutodetectBuilder.MAX_ANOMALY_RECORDS_SETTING_DYNAMIC.getKey(),
                    clusterService.getClusterSettings().get(AutodetectBuilder.MAX_ANOMALY_RECORDS_SETTING_DYNAMIC))
                .build();

            AutodetectBuilder autodetectBuilder = new AutodetectBuilder(job, filesToDelete, LOGGER, env,
                updatedSettings, nativeController, processPipes)
                    .referencedFilters(autodetectParams.filters())
                    .scheduledEvents(autodetectParams.scheduledEvents());

            // if state is null or empty it will be ignored
            // else it is used to restore the quantiles
            if (autodetectParams.quantiles() != null) {
                autodetectBuilder.quantiles(autodetectParams.quantiles());
            }
            autodetectBuilder.build();
            processPipes.connectStreams(PROCESS_STARTUP_TIMEOUT);
        } catch (IOException e) {
            String msg = "Failed to launch autodetect for job " + job.getId();
            LOGGER.error(msg);
            throw ExceptionsHelper.serverError(msg, e);
        }
    }

}

