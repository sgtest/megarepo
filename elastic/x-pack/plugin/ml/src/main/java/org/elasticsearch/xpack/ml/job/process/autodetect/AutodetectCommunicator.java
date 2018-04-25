/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.job.process.autodetect;

import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.common.CheckedSupplier;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.logging.Loggers;
import org.elasticsearch.common.util.concurrent.AbstractRunnable;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.env.Environment;
import org.elasticsearch.index.analysis.AnalysisRegistry;
import org.elasticsearch.xpack.ml.MachineLearning;
import org.elasticsearch.xpack.core.ml.calendars.ScheduledEvent;
import org.elasticsearch.xpack.ml.job.categorization.CategorizationAnalyzer;
import org.elasticsearch.xpack.core.ml.job.config.AnalysisConfig;
import org.elasticsearch.xpack.core.ml.job.config.CategorizationAnalyzerConfig;
import org.elasticsearch.xpack.core.ml.job.config.DataDescription;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.config.JobUpdate;
import org.elasticsearch.xpack.ml.job.persistence.StateStreamer;
import org.elasticsearch.xpack.ml.job.process.CountingInputStream;
import org.elasticsearch.xpack.ml.job.process.DataCountsReporter;
import org.elasticsearch.xpack.ml.job.process.autodetect.output.AutoDetectResultProcessor;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.output.FlushAcknowledgement;
import org.elasticsearch.xpack.ml.job.process.autodetect.params.DataLoadParams;
import org.elasticsearch.xpack.ml.job.process.autodetect.params.FlushJobParams;
import org.elasticsearch.xpack.ml.job.process.autodetect.params.ForecastParams;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.DataCounts;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSizeStats;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSnapshot;
import org.elasticsearch.xpack.ml.job.process.autodetect.writer.DataToProcessWriter;
import org.elasticsearch.xpack.ml.job.process.autodetect.writer.DataToProcessWriterFactory;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;

import java.io.Closeable;
import java.io.IOException;
import java.io.InputStream;
import java.time.Duration;
import java.time.ZonedDateTime;
import java.util.Collections;
import java.util.List;
import java.util.Locale;
import java.util.Optional;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Future;
import java.util.concurrent.TimeoutException;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.BiConsumer;
import java.util.function.Consumer;

public class AutodetectCommunicator implements Closeable {

    private static final Logger LOGGER = Loggers.getLogger(AutodetectCommunicator.class);
    private static final Duration FLUSH_PROCESS_CHECK_FREQUENCY = Duration.ofSeconds(1);

    private final Job job;
    private final Environment environment;
    private final AutodetectProcess autodetectProcess;
    private final StateStreamer stateStreamer;
    private final DataCountsReporter dataCountsReporter;
    private final AutoDetectResultProcessor autoDetectResultProcessor;
    private final Consumer<Exception> onFinishHandler;
    private final ExecutorService autodetectWorkerExecutor;
    private final NamedXContentRegistry xContentRegistry;
    private final boolean includeTokensField;
    private volatile CategorizationAnalyzer categorizationAnalyzer;
    private volatile boolean processKilled;

    AutodetectCommunicator(Job job, Environment environment, AutodetectProcess process, StateStreamer stateStreamer,
                           DataCountsReporter dataCountsReporter, AutoDetectResultProcessor autoDetectResultProcessor,
                           Consumer<Exception> onFinishHandler, NamedXContentRegistry xContentRegistry,
                           ExecutorService autodetectWorkerExecutor) {
        this.job = job;
        this.environment = environment;
        this.autodetectProcess = process;
        this.stateStreamer = stateStreamer;
        this.dataCountsReporter = dataCountsReporter;
        this.autoDetectResultProcessor = autoDetectResultProcessor;
        this.onFinishHandler = onFinishHandler;
        this.xContentRegistry = xContentRegistry;
        this.autodetectWorkerExecutor = autodetectWorkerExecutor;
        this.includeTokensField = MachineLearning.CATEGORIZATION_TOKENIZATION_IN_JAVA
                && job.getAnalysisConfig().getCategorizationFieldName() != null;
    }

    public void init(ModelSnapshot modelSnapshot) throws IOException {
        autodetectProcess.restoreState(stateStreamer, modelSnapshot);
        createProcessWriter(Optional.empty()).writeHeader();
    }

    private DataToProcessWriter createProcessWriter(Optional<DataDescription> dataDescription) {
        return DataToProcessWriterFactory.create(true, includeTokensField, autodetectProcess,
                dataDescription.orElse(job.getDataDescription()), job.getAnalysisConfig(),
                dataCountsReporter, xContentRegistry);
    }

    public void writeToJob(InputStream inputStream, AnalysisRegistry analysisRegistry, XContentType xContentType,
                           DataLoadParams params, BiConsumer<DataCounts, Exception> handler) {
        submitOperation(() -> {
            if (params.isResettingBuckets()) {
                autodetectProcess.writeResetBucketsControlMessage(params);
            }

            CountingInputStream countingStream = new CountingInputStream(inputStream, dataCountsReporter);
            DataToProcessWriter autoDetectWriter = createProcessWriter(params.getDataDescription());

            if (includeTokensField && categorizationAnalyzer == null) {
                createCategorizationAnalyzer(analysisRegistry);
            }

            CountDownLatch latch = new CountDownLatch(1);
            AtomicReference<DataCounts> dataCountsAtomicReference = new AtomicReference<>();
            AtomicReference<Exception> exceptionAtomicReference = new AtomicReference<>();
            autoDetectWriter.write(countingStream, categorizationAnalyzer, xContentType, (dataCounts, e) -> {
                dataCountsAtomicReference.set(dataCounts);
                exceptionAtomicReference.set(e);
                latch.countDown();
            });

            latch.await();
            autoDetectWriter.flushStream();

            if (exceptionAtomicReference.get() != null) {
                throw exceptionAtomicReference.get();
            } else {
                return dataCountsAtomicReference.get();
            }
        },
        handler);
    }

    @Override
    public void close() throws IOException {
        close(false, null);
    }

    /**
     * Closes job this communicator is encapsulating.
     *
     * @param restart   Whether the job should be restarted by persistent tasks
     * @param reason    The reason for closing the job
     */
    public void close(boolean restart, String reason) {
        Future<?> future = autodetectWorkerExecutor.submit(() -> {
            checkProcessIsAlive();
            try {
                if (autodetectProcess.isReady()) {
                    autodetectProcess.close();
                } else {
                    killProcess(false, false);
                    stateStreamer.cancel();
                }
                autoDetectResultProcessor.awaitCompletion();
            } finally {
                onFinishHandler.accept(restart ? new ElasticsearchException(reason) : null);
            }
            LOGGER.info("[{}] job closed", job.getId());
            return null;
        });
        try {
            future.get();
            autodetectWorkerExecutor.shutdown();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        } catch (ExecutionException e) {
            if (processKilled) {
                // In this case the original exception is spurious and highly misleading
                throw ExceptionsHelper.conflictStatusException("Close job interrupted by kill request");
            } else {
                throw new ElasticsearchException(e);
            }
        } finally {
            destroyCategorizationAnalyzer();
        }
    }

    public void killProcess(boolean awaitCompletion, boolean finish) throws IOException {
        try {
            processKilled = true;
            autoDetectResultProcessor.setProcessKilled();
            autodetectWorkerExecutor.shutdown();
            autodetectProcess.kill();

            if (awaitCompletion) {
                try {
                    autoDetectResultProcessor.awaitCompletion();
                } catch (TimeoutException e) {
                    LOGGER.warn(new ParameterizedMessage("[{}] Timed out waiting for killed job", job.getId()), e);
                }
            }
        } finally {
            if (finish) {
                onFinishHandler.accept(null);
            }
            destroyCategorizationAnalyzer();
        }
    }

    public void writeUpdateProcessMessage(UpdateParams updateParams, List<ScheduledEvent> scheduledEvents,
                                          BiConsumer<Void, Exception> handler) {
        submitOperation(() -> {
            if (updateParams.getModelPlotConfig() != null) {
                autodetectProcess.writeUpdateModelPlotMessage(updateParams.getModelPlotConfig());
            }

            // Filters have to be written before detectors
            if (updateParams.getFilter() != null) {
                autodetectProcess.writeUpdateFiltersMessage(Collections.singletonList(updateParams.getFilter()));
            }

            // Add detector rules
            if (updateParams.getDetectorUpdates() != null) {
                for (JobUpdate.DetectorUpdate update : updateParams.getDetectorUpdates()) {
                    if (update.getRules() != null) {
                        autodetectProcess.writeUpdateDetectorRulesMessage(update.getDetectorIndex(), update.getRules());
                    }
                }
            }

            // Add scheduled events; null means there's no update but an empty list means we should clear any events in the process
            if (scheduledEvents != null) {
                autodetectProcess.writeUpdateScheduledEventsMessage(scheduledEvents, job.getAnalysisConfig().getBucketSpan());
            }

            return null;
        }, handler);
    }

    public void flushJob(FlushJobParams params, BiConsumer<FlushAcknowledgement, Exception> handler) {
        submitOperation(() -> {
            String flushId = autodetectProcess.flushJob(params);
            return waitFlushToCompletion(flushId);
        }, handler);
    }

    public void forecastJob(ForecastParams params, BiConsumer<Void, Exception> handler) {
        BiConsumer<Void, Exception> forecastConsumer = (aVoid, e) -> {
            if (e == null) {
                FlushJobParams flushParams = FlushJobParams.builder().build();
                flushJob(flushParams, (flushAcknowledgement, flushException) -> {
                    if (flushException != null) {
                        String msg = String.format(Locale.ROOT, "[%s] exception while flushing job", job.getId());
                        handler.accept(null, ExceptionsHelper.serverError(msg, e));
                    } else {
                        handler.accept(null, null);
                    }
                });
            } else {
                handler.accept(null, e);
            }
        };
        submitOperation(() -> {
            autodetectProcess.forecastJob(params);
            return null;
        }, forecastConsumer);
    }

    public void persistJob(BiConsumer<Void, Exception> handler) {
        submitOperation(() -> {
            autodetectProcess.persistJob();
            return null;
        }, handler);
    }

    @Nullable
    FlushAcknowledgement waitFlushToCompletion(String flushId) {
        LOGGER.debug("[{}] waiting for flush", job.getId());

        FlushAcknowledgement flushAcknowledgement;
        try {
            flushAcknowledgement = autoDetectResultProcessor.waitForFlushAcknowledgement(flushId, FLUSH_PROCESS_CHECK_FREQUENCY);
            while (flushAcknowledgement == null) {
                checkProcessIsAlive();
                checkResultsProcessorIsAlive();
                flushAcknowledgement = autoDetectResultProcessor.waitForFlushAcknowledgement(flushId, FLUSH_PROCESS_CHECK_FREQUENCY);
            }
        } finally {
            autoDetectResultProcessor.clearAwaitingFlush(flushId);
        }

        if (processKilled == false) {
            // We also have to wait for the normalizer to become idle so that we block
            // clients from querying results in the middle of normalization.
            autoDetectResultProcessor.waitUntilRenormalizerIsIdle();

            LOGGER.debug("[{}] Flush completed", job.getId());
        }

        return flushAcknowledgement;
    }

    /**
     * Throws an exception if the process has exited
     */
    private void checkProcessIsAlive() {
        if (!autodetectProcess.isProcessAlive()) {
            // Don't log here - it just causes double logging when the exception gets logged
            throw new ElasticsearchException("[{}] Unexpected death of autodetect: {}", job.getId(), autodetectProcess.readError());
        }
    }

    private void checkResultsProcessorIsAlive() {
        if (autoDetectResultProcessor.isFailed()) {
            // Don't log here - it just causes double logging when the exception gets logged
            throw new ElasticsearchException("[{}] Unexpected death of the result processor", job.getId());
        }
    }

    public ZonedDateTime getProcessStartTime() {
        return autodetectProcess.getProcessStartTime();
    }

    public ModelSizeStats getModelSizeStats() {
        return autoDetectResultProcessor.modelSizeStats();
    }

    public DataCounts getDataCounts() {
        return dataCountsReporter.runningTotalStats();
    }

    /**
     * Care must be taken to ensure this method is not called while data is being posted.
     * The methods in this class that call it wait for all processing to complete first.
     * The expectation is that external calls are only made when cleaning up after a fatal
     * error.
     */
    void destroyCategorizationAnalyzer() {
        if (categorizationAnalyzer != null) {
            categorizationAnalyzer.close();
            categorizationAnalyzer = null;
        }
    }

    private <T> void submitOperation(CheckedSupplier<T, Exception> operation, BiConsumer<T, Exception> handler) {
        autodetectWorkerExecutor.execute(new AbstractRunnable() {
            @Override
            public void onFailure(Exception e) {
                if (processKilled) {
                    handler.accept(null, ExceptionsHelper.conflictStatusException(
                            "[{}] Could not submit operation to process as it has been killed", job.getId()));
                } else {
                    LOGGER.error(new ParameterizedMessage("[{}] Unexpected exception writing to process", job.getId()), e);
                    handler.accept(null, e);
                }
            }

            @Override
            protected void doRun() throws Exception {
                if (processKilled) {
                    handler.accept(null, ExceptionsHelper.conflictStatusException(
                            "[{}] Could not submit operation to process as it has been killed", job.getId()));
                } else {
                    checkProcessIsAlive();
                    handler.accept(operation.get(), null);
                }
            }
        });
    }

    private void createCategorizationAnalyzer(AnalysisRegistry analysisRegistry) throws IOException {
        AnalysisConfig analysisConfig = job.getAnalysisConfig();
        CategorizationAnalyzerConfig categorizationAnalyzerConfig = analysisConfig.getCategorizationAnalyzerConfig();
        if (categorizationAnalyzerConfig == null) {
            categorizationAnalyzerConfig =
                    CategorizationAnalyzerConfig.buildDefaultCategorizationAnalyzer(analysisConfig.getCategorizationFilters());
        }
        categorizationAnalyzer = new CategorizationAnalyzer(analysisRegistry, environment, categorizationAnalyzerConfig);
    }
}
