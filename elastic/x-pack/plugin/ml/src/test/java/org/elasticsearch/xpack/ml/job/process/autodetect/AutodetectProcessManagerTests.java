/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.job.process.autodetect;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.client.Client;
import org.elasticsearch.common.CheckedConsumer;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.EsExecutors;
import org.elasticsearch.common.util.concurrent.EsRejectedExecutionException;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.env.Environment;
import org.elasticsearch.env.TestEnvironment;
import org.elasticsearch.index.analysis.AnalysisRegistry;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.junit.annotations.TestLogging;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.ml.job.config.AnalysisConfig;
import org.elasticsearch.xpack.core.ml.job.config.DataDescription;
import org.elasticsearch.xpack.core.ml.job.config.DetectionRule;
import org.elasticsearch.xpack.core.ml.job.config.Detector;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.config.JobState;
import org.elasticsearch.xpack.core.ml.job.config.JobTaskStatus;
import org.elasticsearch.xpack.core.ml.job.config.JobUpdate;
import org.elasticsearch.xpack.core.ml.job.config.MlFilter;
import org.elasticsearch.xpack.core.ml.job.config.ModelPlotConfig;
import org.elasticsearch.xpack.ml.job.process.autodetect.params.AutodetectParams;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.DataCounts;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSizeStats;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSnapshot;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.Quantiles;
import org.elasticsearch.xpack.ml.action.TransportOpenJobAction.JobTask;
import org.elasticsearch.xpack.ml.job.JobManager;
import org.elasticsearch.xpack.ml.job.categorization.CategorizationAnalyzerTests;
import org.elasticsearch.xpack.ml.job.persistence.JobDataCountsPersister;
import org.elasticsearch.xpack.ml.job.persistence.JobProvider;
import org.elasticsearch.xpack.ml.job.persistence.JobResultsPersister;
import org.elasticsearch.xpack.ml.job.process.autodetect.params.DataLoadParams;
import org.elasticsearch.xpack.ml.job.process.autodetect.params.FlushJobParams;
import org.elasticsearch.xpack.ml.job.process.autodetect.params.TimeRange;
import org.elasticsearch.xpack.ml.job.process.normalizer.NormalizerFactory;
import org.elasticsearch.xpack.ml.notifications.Auditor;
import org.junit.Before;
import org.mockito.Mockito;

import java.io.ByteArrayInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.Collections;
import java.util.Date;
import java.util.HashSet;
import java.util.List;
import java.util.Optional;
import java.util.Set;
import java.util.concurrent.Callable;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.BiConsumer;
import java.util.function.Consumer;

import static org.elasticsearch.mock.orig.Mockito.doAnswer;
import static org.elasticsearch.mock.orig.Mockito.doReturn;
import static org.elasticsearch.mock.orig.Mockito.doThrow;
import static org.elasticsearch.mock.orig.Mockito.times;
import static org.elasticsearch.mock.orig.Mockito.verify;
import static org.elasticsearch.mock.orig.Mockito.verifyNoMoreInteractions;
import static org.elasticsearch.mock.orig.Mockito.when;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;
import static org.hamcrest.core.IsEqual.equalTo;
import static org.mockito.Matchers.any;
import static org.mockito.Matchers.anyBoolean;
import static org.mockito.Matchers.anyString;
import static org.mockito.Matchers.eq;
import static org.mockito.Matchers.same;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.spy;

/**
 * Calling the
 * {@link AutodetectProcessManager#processData(JobTask, AnalysisRegistry, InputStream, XContentType, DataLoadParams, BiConsumer)}
 * method causes an AutodetectCommunicator to be created on demand. Most of
 * these tests have to do that before they can assert other things
 */
public class AutodetectProcessManagerTests extends ESTestCase {

    private Environment environment;
    private AnalysisRegistry analysisRegistry;
    private JobManager jobManager;
    private JobProvider jobProvider;
    private JobResultsPersister jobResultsPersister;
    private JobDataCountsPersister jobDataCountsPersister;
    private NormalizerFactory normalizerFactory;
    private Auditor auditor;

    private DataCounts dataCounts = new DataCounts("foo");
    private ModelSizeStats modelSizeStats = new ModelSizeStats.Builder("foo").build();
    private ModelSnapshot modelSnapshot = new ModelSnapshot.Builder("foo").build();
    private Quantiles quantiles = new Quantiles("foo", new Date(), "state");
    private Set<MlFilter> filters = new HashSet<>();

    @Before
    public void setup() throws Exception {
        Settings settings = Settings.builder().put(Environment.PATH_HOME_SETTING.getKey(), createTempDir()).build();
        environment = TestEnvironment.newEnvironment(settings);
        analysisRegistry = CategorizationAnalyzerTests.buildTestAnalysisRegistry(environment);
        jobManager = mock(JobManager.class);
        jobProvider = mock(JobProvider.class);
        jobResultsPersister = mock(JobResultsPersister.class);
        when(jobResultsPersister.bulkPersisterBuilder(any())).thenReturn(mock(JobResultsPersister.Builder.class));
        jobDataCountsPersister = mock(JobDataCountsPersister.class);
        normalizerFactory = mock(NormalizerFactory.class);
        auditor = mock(Auditor.class);

        when(jobManager.getJobOrThrowIfUnknown("foo")).thenReturn(createJobDetails("foo"));
        doAnswer(invocationOnMock -> {
            @SuppressWarnings("unchecked")
            Consumer<AutodetectParams> handler = (Consumer<AutodetectParams>) invocationOnMock.getArguments()[1];
            handler.accept(buildAutodetectParams());
            return null;
        }).when(jobProvider).getAutodetectParams(any(), any(), any());
    }

    public void testMaxOpenJobsSetting_givenDefault() {
        int maxOpenJobs = AutodetectProcessManager.MAX_OPEN_JOBS_PER_NODE.get(Settings.EMPTY);
        assertEquals(20, maxOpenJobs);
    }

    public void testMaxOpenJobsSetting_givenNewSettingOnly() {
        Settings.Builder settings = Settings.builder();
        settings.put(AutodetectProcessManager.MAX_OPEN_JOBS_PER_NODE.getKey(), 7);
        int maxOpenJobs = AutodetectProcessManager.MAX_OPEN_JOBS_PER_NODE.get(settings.build());
        assertEquals(7, maxOpenJobs);
    }

    public void testMaxOpenJobsSetting_givenOldSettingOnly() {
        Settings.Builder settings = Settings.builder();
        settings.put(AutodetectProcessManager.MAX_RUNNING_JOBS_PER_NODE.getKey(), 9);
        int maxOpenJobs = AutodetectProcessManager.MAX_OPEN_JOBS_PER_NODE.get(settings.build());
        assertEquals(9, maxOpenJobs);
        assertWarnings("[max_running_jobs] setting was deprecated in Elasticsearch and will be removed in a future release! "
                + "See the breaking changes documentation for the next major version.");
    }

    public void testMaxOpenJobsSetting_givenOldAndNewSettings() {
        Settings.Builder settings = Settings.builder();
        settings.put(AutodetectProcessManager.MAX_OPEN_JOBS_PER_NODE.getKey(), 7);
        settings.put(AutodetectProcessManager.MAX_RUNNING_JOBS_PER_NODE.getKey(), 9);
        int maxOpenJobs = AutodetectProcessManager.MAX_OPEN_JOBS_PER_NODE.get(settings.build());
        assertEquals(7, maxOpenJobs);
        assertWarnings("[max_running_jobs] setting was deprecated in Elasticsearch and will be removed in a future release! "
                + "See the breaking changes documentation for the next major version.");
    }

    public void testOpenJob_withoutVersion() {
        Client client = mock(Client.class);
        AutodetectCommunicator communicator = mock(AutodetectCommunicator.class);
        Job.Builder jobBuilder = new Job.Builder(createJobDetails("no_version"));
        jobBuilder.setJobVersion(null);
        Job job = jobBuilder.build();
        assertThat(job.getJobVersion(), is(nullValue()));

        when(jobManager.getJobOrThrowIfUnknown(job.getId())).thenReturn(job);
        AutodetectProcessManager manager = createManager(communicator, client);

        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn(job.getId());

        AtomicReference<Exception> errorHolder = new AtomicReference<>();
        manager.openJob(jobTask, errorHolder::set);

        Exception error = errorHolder.get();
        assertThat(error, is(notNullValue()));
        assertThat(error.getMessage(), equalTo("Cannot open job [no_version] because jobs created prior to version 5.5 are not supported"));
    }

    public void testOpenJob() {
        Client client = mock(Client.class);
        AutodetectCommunicator communicator = mock(AutodetectCommunicator.class);
        when(jobManager.getJobOrThrowIfUnknown("foo")).thenReturn(createJobDetails("foo"));
        AutodetectProcessManager manager = createManager(communicator, client);

        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        when(jobTask.getAllocationId()).thenReturn(1L);
        manager.openJob(jobTask, e -> {});
        assertEquals(1, manager.numberOfOpenJobs());
        assertTrue(manager.jobHasActiveAutodetectProcess(jobTask));
        verify(jobTask).updatePersistentStatus(eq(new JobTaskStatus(JobState.OPENED, 1L)), any());
    }

    public void testOpenJob_exceedMaxNumJobs() {
        when(jobManager.getJobOrThrowIfUnknown("foo")).thenReturn(createJobDetails("foo"));
        when(jobManager.getJobOrThrowIfUnknown("bar")).thenReturn(createJobDetails("bar"));
        when(jobManager.getJobOrThrowIfUnknown("baz")).thenReturn(createJobDetails("baz"));
        when(jobManager.getJobOrThrowIfUnknown("foobar")).thenReturn(createJobDetails("foobar"));

        Client client = mock(Client.class);
        ThreadPool threadPool = mock(ThreadPool.class);
        when(threadPool.getThreadContext()).thenReturn(new ThreadContext(Settings.EMPTY));
        ThreadPool.Cancellable cancellable = mock(ThreadPool.Cancellable.class);
        when(threadPool.scheduleWithFixedDelay(any(), any(), any())).thenReturn(cancellable);
        ExecutorService executorService = mock(ExecutorService.class);
        Future future = mock(Future.class);
        when(executorService.submit(any(Callable.class))).thenReturn(future);
        when(threadPool.executor(anyString())).thenReturn(EsExecutors.newDirectExecutorService());
        AutodetectProcess autodetectProcess = mock(AutodetectProcess.class);
        when(autodetectProcess.isProcessAlive()).thenReturn(true);
        when(autodetectProcess.readAutodetectResults()).thenReturn(Collections.emptyIterator());
        AutodetectProcessFactory autodetectProcessFactory =
                (j, autodetectParams, e, onProcessCrash) -> autodetectProcess;
        Settings.Builder settings = Settings.builder();
        settings.put(AutodetectProcessManager.MAX_OPEN_JOBS_PER_NODE.getKey(), 3);
        AutodetectProcessManager manager = spy(new AutodetectProcessManager(environment, settings.build(), client, threadPool,
                jobManager, jobProvider, jobResultsPersister, jobDataCountsPersister, autodetectProcessFactory,
                normalizerFactory, new NamedXContentRegistry(Collections.emptyList()), auditor));
        doReturn(executorService).when(manager).createAutodetectExecutorService(any());

        doAnswer(invocationOnMock -> {
            @SuppressWarnings("unchecked")
            CheckedConsumer<Exception, IOException> consumer = (CheckedConsumer<Exception, IOException>) invocationOnMock.getArguments()[2];
            consumer.accept(null);
            return null;
        }).when(manager).setJobState(any(), eq(JobState.FAILED), any());

        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        manager.openJob(jobTask, e -> {});
        jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("bar");
        when(jobTask.getAllocationId()).thenReturn(1L);
        manager.openJob(jobTask, e -> {});
        jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("baz");
        when(jobTask.getAllocationId()).thenReturn(2L);
        manager.openJob(jobTask, e -> {});
        assertEquals(3, manager.numberOfOpenJobs());

        Exception[] holder = new Exception[1];
        jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foobar");
        when(jobTask.getAllocationId()).thenReturn(3L);
        manager.openJob(jobTask, e -> holder[0] = e);
        Exception e = holder[0];
        assertEquals("max running job capacity [3] reached", e.getMessage());

        jobTask = mock(JobTask.class);
        when(jobTask.getAllocationId()).thenReturn(2L);
        when(jobTask.getJobId()).thenReturn("baz");
        manager.closeJob(jobTask, false, null);
        assertEquals(2, manager.numberOfOpenJobs());
        manager.openJob(jobTask, e1 -> {});
        assertEquals(3, manager.numberOfOpenJobs());
    }

    public void testProcessData()  {
        AutodetectCommunicator communicator = mock(AutodetectCommunicator.class);
        AutodetectProcessManager manager = createManager(communicator);
        assertEquals(0, manager.numberOfOpenJobs());

        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        DataLoadParams params = new DataLoadParams(TimeRange.builder().build(), Optional.empty());
        manager.openJob(jobTask, e -> {});
        manager.processData(jobTask, analysisRegistry, createInputStream(""), randomFrom(XContentType.values()),
                params, (dataCounts1, e) -> {});
        assertEquals(1, manager.numberOfOpenJobs());
    }

    public void testProcessDataThrowsElasticsearchStatusException_onIoException() {
        AutodetectCommunicator communicator = Mockito.mock(AutodetectCommunicator.class);
        AutodetectProcessManager manager = createManager(communicator);

        DataLoadParams params = mock(DataLoadParams.class);
        InputStream inputStream = createInputStream("");
        XContentType xContentType = randomFrom(XContentType.values());
        doAnswer(invocationOnMock -> {
            @SuppressWarnings("unchecked")
            BiConsumer<DataCounts, Exception> handler = (BiConsumer<DataCounts, Exception>) invocationOnMock.getArguments()[4];
            handler.accept(null, new IOException("blah"));
            return null;
        }).when(communicator).writeToJob(eq(inputStream), same(analysisRegistry), same(xContentType), eq(params), any());


        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        manager.openJob(jobTask, e -> {});
        Exception[] holder = new Exception[1];
        manager.processData(jobTask, analysisRegistry, inputStream, xContentType, params, (dataCounts1, e) -> holder[0] = e);
        assertNotNull(holder[0]);
    }

    public void testCloseJob() {
        AutodetectCommunicator communicator = mock(AutodetectCommunicator.class);
        AutodetectProcessManager manager = createManager(communicator);
        assertEquals(0, manager.numberOfOpenJobs());

        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        manager.openJob(jobTask, e -> {});
        manager.processData(jobTask, analysisRegistry, createInputStream(""), randomFrom(XContentType.values()),
                mock(DataLoadParams.class), (dataCounts1, e) -> {});

        // job is created
        assertEquals(1, manager.numberOfOpenJobs());
        manager.closeJob(jobTask, false, null);
        assertEquals(0, manager.numberOfOpenJobs());
    }

    // DEBUG logging makes it possible to see exactly how the two threads
    // interleaved in the AutodetectProcessManager.close() call
    @TestLogging("org.elasticsearch.xpack.ml.job.process.autodetect:DEBUG")
    public void testCanCloseClosingJob() throws Exception {
        AutodetectCommunicator communicator = mock(AutodetectCommunicator.class);
        AtomicInteger numberOfCommunicatorCloses = new AtomicInteger(0);
        doAnswer(invocationOnMock -> {
            numberOfCommunicatorCloses.incrementAndGet();
            // This increases the chance of the two threads both getting into
            // the middle of the AutodetectProcessManager.close() method
            Thread.yield();
            return null;
        }).when(communicator).close(anyBoolean(), anyString());
        AutodetectProcessManager manager = createManager(communicator);
        assertEquals(0, manager.numberOfOpenJobs());

        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        manager.openJob(jobTask, e -> {});
        manager.processData(jobTask, analysisRegistry, createInputStream(""), randomFrom(XContentType.values()),
                mock(DataLoadParams.class), (dataCounts1, e) -> {});

        assertEquals(1, manager.numberOfOpenJobs());

        // Close the job in a separate thread
        Thread closeThread = new Thread(() -> manager.closeJob(jobTask, false, "in separate thread"));
        closeThread.start();
        Thread.yield();

        // Also close the job in the current thread, so that we have two simultaneous close requests
        manager.closeJob(jobTask, false, "in main test thread");

        // The 10 second timeout here is usually far in excess of what is required.  In the vast
        // majority of cases the other thread will exit within a few milliseconds.  However, it
        // has been observed that on some VMs the test can fail because the VM stalls at the
        // wrong moment.  A 10 second timeout is on a par with the length of time assertBusy()
        // would wait under these circumstances.
        closeThread.join(10000);
        assertFalse(closeThread.isAlive());

        // Only one of the threads should have called AutodetectCommunicator.close()
        assertEquals(1, numberOfCommunicatorCloses.get());
        assertEquals(0, manager.numberOfOpenJobs());
    }

    public void testCanKillClosingJob() throws Exception {
        CountDownLatch closeStartedLatch = new CountDownLatch(1);
        CountDownLatch killLatch = new CountDownLatch(1);
        CountDownLatch closeInterruptedLatch = new CountDownLatch(1);
        AutodetectCommunicator communicator = mock(AutodetectCommunicator.class);
        doAnswer(invocationOnMock -> {
            closeStartedLatch.countDown();
            if (killLatch.await(3, TimeUnit.SECONDS)) {
                closeInterruptedLatch.countDown();
            }
            return null;
        }).when(communicator).close(anyBoolean(), anyString());
        doAnswer(invocationOnMock -> {
            killLatch.countDown();
            return null;
        }).when(communicator).killProcess(anyBoolean(), anyBoolean());
        AutodetectProcessManager manager = createManager(communicator);
        assertEquals(0, manager.numberOfOpenJobs());

        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        manager.openJob(jobTask, e -> {});
        manager.processData(jobTask, analysisRegistry, createInputStream(""), randomFrom(XContentType.values()),
                mock(DataLoadParams.class), (dataCounts1, e) -> {});

        // Close the job in a separate thread so that it can simulate taking a long time to close
        Thread closeThread = new Thread(() -> manager.closeJob(jobTask, false, null));
        closeThread.start();
        assertTrue(closeStartedLatch.await(3, TimeUnit.SECONDS));

        // Kill the job in the current thread, which will be while the job is "closing"
        manager.killProcess(jobTask, false, null);
        assertEquals(0, killLatch.getCount());

        // Assert close method was awoken by the kill
        assertTrue(closeInterruptedLatch.await(3, TimeUnit.SECONDS));

        closeThread.join(500);
        assertFalse(closeThread.isAlive());
    }

    public void testBucketResetMessageIsSent() {
        AutodetectCommunicator communicator = mock(AutodetectCommunicator.class);
        AutodetectProcessManager manager = createManager(communicator);
        XContentType xContentType = randomFrom(XContentType.values());

        DataLoadParams params = new DataLoadParams(TimeRange.builder().startTime("1000").endTime("2000").build(), Optional.empty());
        InputStream inputStream = createInputStream("");
        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        manager.openJob(jobTask, e -> {});
        manager.processData(jobTask, analysisRegistry, inputStream, xContentType, params, (dataCounts1, e) -> {});
        verify(communicator).writeToJob(same(inputStream), same(analysisRegistry), same(xContentType), same(params), any());
    }

    public void testFlush() {
        AutodetectCommunicator communicator = mock(AutodetectCommunicator.class);
        AutodetectProcessManager manager = createManager(communicator);

        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        InputStream inputStream = createInputStream("");
        manager.openJob(jobTask, e -> {});
        manager.processData(jobTask, analysisRegistry, inputStream, randomFrom(XContentType.values()),
                mock(DataLoadParams.class), (dataCounts1, e) -> {});

        FlushJobParams params = FlushJobParams.builder().build();
        manager.flushJob(jobTask, params, ActionListener.wrap(flushAcknowledgement -> {}, e -> fail(e.getMessage())));

        verify(communicator).flushJob(same(params), any());
    }

    public void testFlushThrows() {
        AutodetectCommunicator communicator = mock(AutodetectCommunicator.class);
        AutodetectProcessManager manager = createManagerAndCallProcessData(communicator, "foo");

        FlushJobParams params = FlushJobParams.builder().build();
        doAnswer(invocationOnMock -> {
            @SuppressWarnings("unchecked")
            BiConsumer<Void, Exception> handler = (BiConsumer<Void, Exception>) invocationOnMock.getArguments()[1];
            handler.accept(null, new IOException("blah"));
            return null;
        }).when(communicator).flushJob(same(params), any());

        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        Exception[] holder = new Exception[1];
        manager.flushJob(jobTask, params, ActionListener.wrap(flushAcknowledgement -> {}, e -> holder[0] = e));
        assertEquals("[foo] exception while flushing job", holder[0].getMessage());
    }

    public void testCloseThrows() {
        AutodetectCommunicator communicator = mock(AutodetectCommunicator.class);
        AutodetectProcessManager manager = createManager(communicator);

        // let the communicator throw, simulating a problem with the underlying
        // autodetect, e.g. a crash
        doThrow(Exception.class).when(communicator).close(anyBoolean(), anyString());

        // create a jobtask
        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        manager.openJob(jobTask, e -> {});
        manager.processData(jobTask, analysisRegistry, createInputStream(""), randomFrom(XContentType.values()), mock(DataLoadParams.class),
                (dataCounts1, e) -> {
                });
        verify(manager).setJobState(any(), eq(JobState.OPENED));
        // job is created
        assertEquals(1, manager.numberOfOpenJobs());
        expectThrows(ElasticsearchException.class, () -> manager.closeJob(jobTask, false, null));
        assertEquals(0, manager.numberOfOpenJobs());

        verify(manager).setJobState(any(), eq(JobState.FAILED));
    }

    public void testWriteUpdateProcessMessage() {
        AutodetectCommunicator communicator = mock(AutodetectCommunicator.class);
        AutodetectProcessManager manager = createManagerAndCallProcessData(communicator, "foo");
        ModelPlotConfig modelConfig = mock(ModelPlotConfig.class);
        List<DetectionRule> rules = Collections.singletonList(mock(DetectionRule.class));
        List<JobUpdate.DetectorUpdate> detectorUpdates = Collections.singletonList(new JobUpdate.DetectorUpdate(2, null, rules));
        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        UpdateParams updateParams = UpdateParams.builder("foo").modelPlotConfig(modelConfig).detectorUpdates(detectorUpdates).build();
        manager.writeUpdateProcessMessage(jobTask, updateParams, e -> {});
        verify(communicator).writeUpdateProcessMessage(same(updateParams), eq(null), any());
    }

    public void testJobHasActiveAutodetectProcess() {
        AutodetectCommunicator communicator = mock(AutodetectCommunicator.class);
        AutodetectProcessManager manager = createManager(communicator);
        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        assertFalse(manager.jobHasActiveAutodetectProcess(jobTask));

        manager.openJob(jobTask, e -> {});
        manager.processData(jobTask, analysisRegistry, createInputStream(""), randomFrom(XContentType.values()),
                mock(DataLoadParams.class), (dataCounts1, e) -> {});

        assertTrue(manager.jobHasActiveAutodetectProcess(jobTask));
        jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("bar");
        when(jobTask.getAllocationId()).thenReturn(1L);
        assertFalse(manager.jobHasActiveAutodetectProcess(jobTask));
    }

    public void testKillKillsAutodetectProcess() throws IOException {
        AutodetectCommunicator communicator = mock(AutodetectCommunicator.class);
        AutodetectProcessManager manager = createManager(communicator);
        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        assertFalse(manager.jobHasActiveAutodetectProcess(jobTask));

        manager.openJob(jobTask, e -> {});
        manager.processData(jobTask, analysisRegistry, createInputStream(""), randomFrom(XContentType.values()),
                mock(DataLoadParams.class), (dataCounts1, e) -> {});

        assertTrue(manager.jobHasActiveAutodetectProcess(jobTask));

        manager.killAllProcessesOnThisNode();

        verify(communicator).killProcess(false, false);
    }

    public void testKillingAMissingJobFinishesTheTask() throws IOException {
        AutodetectCommunicator communicator = mock(AutodetectCommunicator.class);
        AutodetectProcessManager manager = createManager(communicator);
        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");

        manager.killProcess(jobTask, false, null);

        verify(jobTask).markAsCompleted();
    }

    public void testProcessData_GivenStateNotOpened() {
        AutodetectCommunicator communicator = mock(AutodetectCommunicator.class);
        doAnswer(invocationOnMock -> {
            @SuppressWarnings("unchecked")
            BiConsumer<DataCounts, Exception> handler = (BiConsumer<DataCounts, Exception>) invocationOnMock.getArguments()[4];
            handler.accept(new DataCounts("foo"), null);
            return null;
        }).when(communicator).writeToJob(any(), any(), any(), any(), any());
        AutodetectProcessManager manager = createManager(communicator);

        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        manager.openJob(jobTask, e -> {});
        InputStream inputStream = createInputStream("");
        DataCounts[] dataCounts = new DataCounts[1];
        manager.processData(jobTask, analysisRegistry, inputStream,
                randomFrom(XContentType.values()), mock(DataLoadParams.class), (dataCounts1, e) -> dataCounts[0] = dataCounts1);

        assertThat(dataCounts[0], equalTo(new DataCounts("foo")));
    }

    public void testCreate_notEnoughThreads() throws IOException {
        Client client = mock(Client.class);
        ThreadPool threadPool = mock(ThreadPool.class);
        when(threadPool.getThreadContext()).thenReturn(new ThreadContext(Settings.EMPTY));
        ExecutorService executorService = mock(ExecutorService.class);
        doThrow(new EsRejectedExecutionException("")).when(executorService).submit(any(Runnable.class));
        when(threadPool.executor(anyString())).thenReturn(executorService);
        when(threadPool.scheduleWithFixedDelay(any(), any(), any())).thenReturn(mock(ThreadPool.Cancellable.class));
        when(jobManager.getJobOrThrowIfUnknown("my_id")).thenReturn(createJobDetails("my_id"));
        AutodetectProcess autodetectProcess = mock(AutodetectProcess.class);
        AutodetectProcessFactory autodetectProcessFactory =
                (j, autodetectParams, e, onProcessCrash) -> autodetectProcess;
        AutodetectProcessManager manager = new AutodetectProcessManager(environment, Settings.EMPTY,
                client, threadPool, jobManager, jobProvider, jobResultsPersister, jobDataCountsPersister, autodetectProcessFactory,
                normalizerFactory, new NamedXContentRegistry(Collections.emptyList()), auditor);

        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("my_id");
        expectThrows(EsRejectedExecutionException.class,
                () -> manager.create(jobTask, buildAutodetectParams(), e -> {}));
        verify(autodetectProcess, times(1)).close();
    }

    public void testCreate_givenFirstTime() {
        modelSnapshot = null;
        AutodetectProcessManager manager = createNonSpyManager("foo");

        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        manager.create(jobTask, buildAutodetectParams(), e -> {});

        String expectedNotification = "Loading model snapshot [N/A], job latest_record_timestamp [N/A]";
        verify(auditor).info("foo", expectedNotification);
        verifyNoMoreInteractions(auditor);
    }

    public void testCreate_givenExistingModelSnapshot() {
        modelSnapshot = new ModelSnapshot.Builder("foo").setSnapshotId("snapshot-1")
                .setLatestRecordTimeStamp(new Date(0L)).build();
        dataCounts = new DataCounts("foo");
        dataCounts.setLatestRecordTimeStamp(new Date(1L));
        AutodetectProcessManager manager = createNonSpyManager("foo");

        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        manager.create(jobTask, buildAutodetectParams(), e -> {});

        String expectedNotification = "Loading model snapshot [snapshot-1] with " +
                "latest_record_timestamp [1970-01-01T00:00:00.000Z], " +
                "job latest_record_timestamp [1970-01-01T00:00:00.001Z]";
        verify(auditor).info("foo", expectedNotification);
        verifyNoMoreInteractions(auditor);
    }

    public void testCreate_givenNonZeroCountsAndNoModelSnapshotNorQuantiles() {
        modelSnapshot = null;
        quantiles = null;
        dataCounts = new DataCounts("foo");
        dataCounts.setLatestRecordTimeStamp(new Date(0L));
        dataCounts.incrementProcessedRecordCount(42L);
        AutodetectProcessManager manager = createNonSpyManager("foo");

        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn("foo");
        manager.create(jobTask, buildAutodetectParams(), e -> {});

        String expectedNotification = "Loading model snapshot [N/A], " +
                "job latest_record_timestamp [1970-01-01T00:00:00.000Z]";
        verify(auditor).info("foo", expectedNotification);
        verify(auditor).warning("foo", "No model snapshot could be found for a job with processed records");
        verify(auditor).warning("foo", "No quantiles could be found for a job with processed records");
        verifyNoMoreInteractions(auditor);
    }

    private AutodetectProcessManager createNonSpyManager(String jobId) {
        Client client = mock(Client.class);
        ThreadPool threadPool = mock(ThreadPool.class);
        when(threadPool.getThreadContext()).thenReturn(new ThreadContext(Settings.EMPTY));
        ExecutorService executorService = mock(ExecutorService.class);
        when(threadPool.executor(anyString())).thenReturn(executorService);
        when(threadPool.scheduleWithFixedDelay(any(), any(), any())).thenReturn(mock(ThreadPool.Cancellable.class));
        when(jobManager.getJobOrThrowIfUnknown(jobId)).thenReturn(createJobDetails(jobId));
        AutodetectProcess autodetectProcess = mock(AutodetectProcess.class);
        AutodetectProcessFactory autodetectProcessFactory =
                (j, autodetectParams, e, onProcessCrash) -> autodetectProcess;
        return new AutodetectProcessManager(environment, Settings.EMPTY, client, threadPool, jobManager,
                jobProvider, jobResultsPersister, jobDataCountsPersister, autodetectProcessFactory,
                normalizerFactory, new NamedXContentRegistry(Collections.emptyList()), auditor);
    }

    private AutodetectParams buildAutodetectParams() {
        return new AutodetectParams.Builder("foo")
                .setDataCounts(dataCounts)
                .setModelSizeStats(modelSizeStats)
                .setModelSnapshot(modelSnapshot)
                .setQuantiles(quantiles)
                .setFilters(filters)
                .build();
    }

    private AutodetectProcessManager createManager(AutodetectCommunicator communicator) {
        Client client = mock(Client.class);
        return createManager(communicator, client);
    }

    private AutodetectProcessManager createManager(AutodetectCommunicator communicator, Client client) {
        ThreadPool threadPool = mock(ThreadPool.class);
        when(threadPool.getThreadContext()).thenReturn(new ThreadContext(Settings.EMPTY));
        when(threadPool.executor(anyString())).thenReturn(EsExecutors.newDirectExecutorService());
        AutodetectProcessFactory autodetectProcessFactory = mock(AutodetectProcessFactory.class);
        AutodetectProcessManager manager = new AutodetectProcessManager(environment, Settings.EMPTY,
                client, threadPool, jobManager, jobProvider, jobResultsPersister, jobDataCountsPersister,
                autodetectProcessFactory, normalizerFactory,
                new NamedXContentRegistry(Collections.emptyList()), auditor);
        manager = spy(manager);
        doReturn(communicator).when(manager).create(any(), eq(buildAutodetectParams()), any());
        return manager;
    }

    private AutodetectProcessManager createManagerAndCallProcessData(AutodetectCommunicator communicator, String jobId) {
        AutodetectProcessManager manager = createManager(communicator);
        JobTask jobTask = mock(JobTask.class);
        when(jobTask.getJobId()).thenReturn(jobId);
        manager.openJob(jobTask, e -> {});
        manager.processData(jobTask, analysisRegistry, createInputStream(""), randomFrom(XContentType.values()),
                mock(DataLoadParams.class), (dataCounts, e) -> {});
        return manager;
    }

    private Job createJobDetails(String jobId) {
        DataDescription.Builder dd = new DataDescription.Builder();
        dd.setFormat(DataDescription.DataFormat.DELIMITED);
        dd.setTimeFormat("epoch");
        dd.setFieldDelimiter(',');

        Detector d = new Detector.Builder("metric", "value").build();

        AnalysisConfig.Builder ac = new AnalysisConfig.Builder(Collections.singletonList(d));

        Job.Builder builder = new Job.Builder(jobId);
        builder.setDataDescription(dd);
        builder.setAnalysisConfig(ac);

        return builder.build(new Date());
    }

    private static InputStream createInputStream(String input) {
        return new ByteArrayInputStream(input.getBytes(StandardCharsets.UTF_8));
    }
}
