/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.inference.deployment;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.apache.lucene.util.SetOnce;
import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.search.SearchAction;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.util.concurrent.AbstractRunnable;
import org.elasticsearch.common.xcontent.LoggingDeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.index.query.IdsQueryBuilder;
import org.elasticsearch.persistent.PersistentTasksCustomMetadata;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.ml.inference.deployment.TrainedModelDeploymentState;
import org.elasticsearch.xpack.core.ml.inference.deployment.TrainedModelDeploymentTaskState;
import org.elasticsearch.xpack.core.ml.inference.results.InferenceResults;
import org.elasticsearch.xpack.core.ml.job.messages.Messages;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.ml.MachineLearning;
import org.elasticsearch.xpack.ml.inference.nlp.NlpTask;
import org.elasticsearch.xpack.ml.inference.nlp.NlpTaskConfig;
import org.elasticsearch.xpack.ml.inference.pytorch.process.NativePyTorchProcess;
import org.elasticsearch.xpack.ml.inference.pytorch.process.PyTorchProcessFactory;
import org.elasticsearch.xpack.ml.inference.pytorch.process.PyTorchResultProcessor;
import org.elasticsearch.xpack.ml.inference.pytorch.process.PyTorchStateStreamer;

import java.io.IOException;
import java.io.InputStream;
import java.util.Locale;
import java.util.Objects;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Consumer;

import static org.elasticsearch.xpack.core.ClientHelper.ML_ORIGIN;
import static org.elasticsearch.xpack.core.ClientHelper.executeAsyncWithOrigin;

public class DeploymentManager {

    private static final Logger logger = LogManager.getLogger(DeploymentManager.class);
    private static final AtomicLong requestIdCounter = new AtomicLong(1);

    private final Client client;
    private final NamedXContentRegistry xContentRegistry;
    private final PyTorchProcessFactory pyTorchProcessFactory;
    private final ExecutorService executorServiceForDeployment;
    private final ExecutorService executorServiceForProcess;
    private final ConcurrentMap<Long, ProcessContext> processContextByAllocation = new ConcurrentHashMap<>();

    public DeploymentManager(Client client, NamedXContentRegistry xContentRegistry,
                             ThreadPool threadPool, PyTorchProcessFactory pyTorchProcessFactory) {
        this.client = Objects.requireNonNull(client);
        this.xContentRegistry = Objects.requireNonNull(xContentRegistry);
        this.pyTorchProcessFactory = Objects.requireNonNull(pyTorchProcessFactory);
        this.executorServiceForDeployment = threadPool.executor(MachineLearning.UTILITY_THREAD_POOL_NAME);
        this.executorServiceForProcess = threadPool.executor(MachineLearning.JOB_COMMS_THREAD_POOL_NAME);
    }

    public void startDeployment(TrainedModelDeploymentTask task) {
        executorServiceForDeployment.execute(() -> doStartDeployment(task));
    }

    private void doStartDeployment(TrainedModelDeploymentTask task) {
        logger.debug("[{}] Starting model deployment", task.getModelId());

        ProcessContext processContext = new ProcessContext(task.getModelId(), task.getIndex(), executorServiceForProcess);

        if (processContextByAllocation.putIfAbsent(task.getAllocationId(), processContext) != null) {
            throw ExceptionsHelper.serverError("[{}] Could not create process as one already exists", task.getModelId());
        }

        String taskConfigDocId = NlpTaskConfig.documentId(task.getModelId());

        ActionListener<Boolean> modelLoadedListener = ActionListener.wrap(
            success -> {
                executorServiceForProcess.execute(() -> processContext.resultProcessor.process(processContext.process.get()));

                setTaskStateToStarted(task, ActionListener.wrap(
                    response -> logger.info("[{}] trained model loaded", task.getModelId()),
                    e -> failTask(task,
                        String.format(Locale.ROOT, "[%s] error setting task state to [%s] [%s]",
                            task.getModelId(), TrainedModelDeploymentState.STARTED, e))
                ));
            },
            e -> failTask(task,
                String.format(Locale.ROOT, "[%s] error loading model [%s]", task.getModelId(), e))
        );

        ActionListener<SearchResponse> configListener = ActionListener.wrap(
            searchResponse -> {
                if (searchResponse.getHits().getHits().length == 0) {
                    failTask(task, Messages.getMessage(Messages.TASK_CONFIG_NOT_FOUND, task.getModelId(), taskConfigDocId));
                    return;
                }

                NlpTaskConfig config = parseConfigDocLeniently(searchResponse.getHits().getAt(0));
                NlpTask nlpTask = NlpTask.fromConfig(config);
                NlpTask.Processor processor = nlpTask.createProcessor();
                processContext.nlpTaskProcessor.set(processor);
                startAndLoad(task, processContext, modelLoadedListener);
            },
            e -> failTask(task,
                String.format(Locale.ROOT, "[%s] creating NLP task from configuration failed with error [%s]", task.getModelId(), e))
        );

        SearchRequest searchRequest = taskConfigSearchRequest(taskConfigDocId, task.getIndex());
        executeAsyncWithOrigin(client, ML_ORIGIN, SearchAction.INSTANCE, searchRequest, configListener);
    }

    private SearchRequest taskConfigSearchRequest(String documentId, String index) {
        return client.prepareSearch(index)
            .setQuery(new IdsQueryBuilder().addIds(documentId))
            .setSize(1)
            .setTrackTotalHits(false)
            .request();
    }

    NlpTaskConfig parseConfigDocLeniently(SearchHit hit) throws IOException {

        try (InputStream stream = hit.getSourceRef().streamInput();
             XContentParser parser = XContentFactory.xContent(XContentType.JSON)
                 .createParser(xContentRegistry, LoggingDeprecationHandler.INSTANCE, stream)) {
            return NlpTaskConfig.fromXContent(parser, true);
        } catch (IOException e) {
            logger.error(new ParameterizedMessage("failed to parse NLP task config [{}]", hit.getId()), e);
            throw e;
        }
    }

    private void startAndLoad(TrainedModelDeploymentTask task,
                              ProcessContext processContext,
                              ActionListener<Boolean> loadedListener) {
        try {
            processContext.startProcess();
            processContext.loadModel(loadedListener);
        } catch (Exception e) {
            failTask(task,
                String.format(Locale.ROOT, "[%s] loading the model failed with error [%s]", task.getModelId(), e));
        }
    }

    public void stopDeployment(TrainedModelDeploymentTask task) {
        ProcessContext processContext;
        synchronized (processContextByAllocation) {
            processContext = processContextByAllocation.get(task.getAllocationId());
        }
        if (processContext != null) {
            logger.info("[{}] Stopping deployment", task.getModelId());
            processContext.stopProcess();
        } else {
            logger.info("[{}] No process context to stop", task.getModelId());
        }
    }

    public void infer(TrainedModelDeploymentTask task,
                      String input, TimeValue timeout,
                      ActionListener<InferenceResults> listener) {
        ProcessContext processContext = processContextByAllocation.get(task.getAllocationId());

        if (processContext == null) {
            listener.onFailure(new IllegalStateException("[" + task.getModelId() + "] process context missing"));
            return;
        }

        final String requestId = String.valueOf(requestIdCounter.getAndIncrement());

        executorServiceForProcess.execute(new AbstractRunnable() {
            @Override
            public void onFailure(Exception e) {
                listener.onFailure(e);
            }

            @Override
            protected void doRun() {
                try {
                    NlpTask.Processor processor = processContext.nlpTaskProcessor.get();
                    processor.validateInputs(input);
                    BytesReference request = processor.getRequestBuilder().buildRequest(input, requestId);
                    logger.trace(() -> "Inference Request "+ request.utf8ToString());
                    processContext.process.get().writeInferenceRequest(request);

                    waitForResult(processContext, requestId, timeout, processor.getResultProcessor(), listener);
                } catch (IOException e) {
                    logger.error(new ParameterizedMessage("[{}] error writing to process", processContext.modelId), e);
                    onFailure(ExceptionsHelper.serverError("error writing to process", e));
                }
            }
        });
    }

    private void waitForResult(ProcessContext processContext,
                               String requestId,
                               TimeValue timeout,
                               NlpTask.ResultProcessor inferenceResultsProcessor,
                               ActionListener<InferenceResults> listener) {
        try {
            PyTorchResult pyTorchResult = processContext.resultProcessor.waitForResult(requestId, timeout);
            if (pyTorchResult == null) {
                listener.onFailure(new ElasticsearchStatusException("timeout [{}] waiting for inference result",
                    RestStatus.TOO_MANY_REQUESTS, timeout));
                return;
            }

            if (pyTorchResult.isError()) {
                listener.onFailure(new ElasticsearchStatusException(pyTorchResult.getError(),
                    RestStatus.INTERNAL_SERVER_ERROR));
                return;
            }

            logger.debug(() -> new ParameterizedMessage("[{}] retrieved result for request [{}]", processContext.modelId, requestId));
            InferenceResults results = inferenceResultsProcessor.processResult(pyTorchResult);
            logger.debug(() -> new ParameterizedMessage("[{}] processed result for request [{}]", processContext.modelId, requestId));
            listener.onResponse(results);
        } catch (InterruptedException e) {
            listener.onFailure(e);
        }
    }

    private void setTaskStateToStarted(TrainedModelDeploymentTask task,
                                     ActionListener<PersistentTasksCustomMetadata.PersistentTask<?>> listener) {
        TrainedModelDeploymentTaskState startedState = new TrainedModelDeploymentTaskState(
            TrainedModelDeploymentState.STARTED, task.getAllocationId(), null);
        task.updatePersistentTaskState(startedState, listener);
    }
    private void failTask(TrainedModelDeploymentTask task,
                          String reason) {

        logger.error("[{}] failed with reason [{}]", task.getModelId(), reason);

        TrainedModelDeploymentTaskState taskState =
            new TrainedModelDeploymentTaskState(TrainedModelDeploymentState.FAILED, task.getAllocationId(), reason);

        task.updatePersistentTaskState(taskState, ActionListener.wrap(
            persistentTask -> {},
            e -> logger.error(new ParameterizedMessage("[{}] error setting model deployment state to failed. " +
                "Failure reason: [{}]", task.getModelId(), reason), e)
        ));
    }

    class ProcessContext {

        private final String modelId;
        private final String index;
        private final SetOnce<NativePyTorchProcess> process = new SetOnce<>();
        private final SetOnce<NlpTask.Processor> nlpTaskProcessor = new SetOnce<>();
        private final PyTorchResultProcessor resultProcessor;
        private final PyTorchStateStreamer stateStreamer;

        ProcessContext(String modelId, String index, ExecutorService executorService) {
            this.modelId = Objects.requireNonNull(modelId);
            this.index = Objects.requireNonNull(index);
            resultProcessor = new PyTorchResultProcessor(modelId);
            this.stateStreamer = new PyTorchStateStreamer(client, executorService, xContentRegistry);
        }

        synchronized void startProcess() {
            process.set(pyTorchProcessFactory.createProcess(modelId, executorServiceForProcess, onProcessCrash()));
        }

        synchronized void stopProcess() {
            resultProcessor.stop();
            if (process.get() == null) {
                return;
            }
            try {
                stateStreamer.cancel();
                process.get().kill(true);
            } catch (IOException e) {
                logger.error(new ParameterizedMessage("[{}] Failed to kill process", modelId), e);
            }
        }

        private Consumer<String> onProcessCrash() {
            return reason -> logger.error("[{}] process crashed due to reason [{}]", modelId, reason);
        }

        void loadModel(ActionListener<Boolean> listener) {
            process.get().loadModel(modelId, index, stateStreamer, listener);
        }
    }
}
