/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.ml.datafeed;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.client.Client;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.ml.datafeed.DatafeedConfig;
import org.elasticsearch.xpack.core.ml.datafeed.DatafeedTimingStats;
import org.elasticsearch.xpack.core.ml.job.config.DataDescription;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.messages.Messages;
import org.elasticsearch.xpack.ml.action.TransportStartDatafeedAction;
import org.elasticsearch.xpack.ml.annotations.AnnotationPersister;
import org.elasticsearch.xpack.ml.job.persistence.JobResultsPersister;
import org.elasticsearch.xpack.ml.job.persistence.RestartTimeInfo;
import org.elasticsearch.xpack.ml.notifications.AnomalyDetectionAuditor;
import org.junit.Before;

import java.util.Collections;
import java.util.Date;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.Consumer;

import static org.elasticsearch.test.NodeRoles.nonRemoteClusterClientNode;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.nullValue;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class DatafeedJobBuilderTests extends ESTestCase {

    private Client client;
    private AnomalyDetectionAuditor auditor;
    private AnnotationPersister annotationPersister;
    private Consumer<Exception> taskHandler;
    private JobResultsPersister jobResultsPersister;

    private DatafeedJobBuilder datafeedJobBuilder;

    @Before
    @SuppressWarnings("unchecked")
    public void init() {
        client = mock(Client.class);
        ThreadPool threadPool = mock(ThreadPool.class);
        when(client.threadPool()).thenReturn(threadPool);
        when(threadPool.getThreadContext()).thenReturn(new ThreadContext(Settings.EMPTY));
        when(client.settings()).thenReturn(Settings.EMPTY);
        auditor = mock(AnomalyDetectionAuditor.class);
        annotationPersister = mock(AnnotationPersister.class);
        taskHandler = mock(Consumer.class);
        jobResultsPersister = mock(JobResultsPersister.class);

        datafeedJobBuilder =
            new DatafeedJobBuilder(
                client,
                xContentRegistry(),
                auditor,
                annotationPersister,
                System::currentTimeMillis,
                jobResultsPersister,
                Settings.EMPTY,
                "test_node");
    }

    public void testBuild_GivenScrollDatafeedAndNewJob() throws Exception {
        DataDescription.Builder dataDescription = new DataDescription.Builder();
        dataDescription.setTimeField("time");
        Job.Builder jobBuilder = DatafeedManagerTests.createDatafeedJob();
        jobBuilder.setDataDescription(dataDescription);
        jobBuilder.setCreateTime(new Date());
        DatafeedConfig.Builder datafeed = DatafeedManagerTests.createDatafeedConfig("datafeed1", jobBuilder.getId());

        AtomicBoolean wasHandlerCalled = new AtomicBoolean(false);
        ActionListener<DatafeedJob> datafeedJobHandler = ActionListener.wrap(
                datafeedJob -> {
                    assertThat(datafeedJob.isRunning(), is(true));
                    assertThat(datafeedJob.isIsolated(), is(false));
                    assertThat(datafeedJob.lastEndTimeMs(), is(nullValue()));
                    wasHandlerCalled.compareAndSet(false, true);
                }, e -> fail()
        );

        DatafeedContext datafeedContext = DatafeedContext.builder()
            .setDatafeedConfig(datafeed.build())
            .setJob(jobBuilder.build())
            .setRestartTimeInfo(new RestartTimeInfo(null, null, false))
            .setTimingStats(new DatafeedTimingStats(jobBuilder.getId()))
            .build();

        TransportStartDatafeedAction.DatafeedTask datafeedTask = newDatafeedTask("datafeed1");

        datafeedJobBuilder.build(datafeedTask, datafeedContext, datafeedJobHandler);

        assertBusy(() -> wasHandlerCalled.get());
    }

    public void testBuild_GivenScrollDatafeedAndOldJobWithLatestRecordTimestampAfterLatestBucket() throws Exception {
        DataDescription.Builder dataDescription = new DataDescription.Builder();
        dataDescription.setTimeField("time");
        Job.Builder jobBuilder = DatafeedManagerTests.createDatafeedJob();
        jobBuilder.setDataDescription(dataDescription);
        jobBuilder.setCreateTime(new Date());
        DatafeedConfig.Builder datafeed = DatafeedManagerTests.createDatafeedConfig("datafeed1", jobBuilder.getId());

        AtomicBoolean wasHandlerCalled = new AtomicBoolean(false);
        ActionListener<DatafeedJob> datafeedJobHandler = ActionListener.wrap(
                datafeedJob -> {
                    assertThat(datafeedJob.isRunning(), is(true));
                    assertThat(datafeedJob.isIsolated(), is(false));
                    assertThat(datafeedJob.lastEndTimeMs(), equalTo(7_200_000L));
                    wasHandlerCalled.compareAndSet(false, true);
                }, e -> fail()
        );

        DatafeedContext datafeedContext = DatafeedContext.builder()
            .setDatafeedConfig(datafeed.build())
            .setJob(jobBuilder.build())
            .setRestartTimeInfo(new RestartTimeInfo(3_600_000L, 7_200_000L, false))
            .setTimingStats(new DatafeedTimingStats(jobBuilder.getId()))
            .build();

        TransportStartDatafeedAction.DatafeedTask datafeedTask = newDatafeedTask("datafeed1");

        datafeedJobBuilder.build(datafeedTask, datafeedContext, datafeedJobHandler);

        assertBusy(() -> wasHandlerCalled.get());
    }

    public void testBuild_GivenScrollDatafeedAndOldJobWithLatestBucketAfterLatestRecordTimestamp() throws Exception {
        DataDescription.Builder dataDescription = new DataDescription.Builder();
        dataDescription.setTimeField("time");
        Job.Builder jobBuilder = DatafeedManagerTests.createDatafeedJob();
        jobBuilder.setDataDescription(dataDescription);
        jobBuilder.setCreateTime(new Date());
        DatafeedConfig.Builder datafeed = DatafeedManagerTests.createDatafeedConfig("datafeed1", jobBuilder.getId());

        AtomicBoolean wasHandlerCalled = new AtomicBoolean(false);
        ActionListener<DatafeedJob> datafeedJobHandler = ActionListener.wrap(
                datafeedJob -> {
                    assertThat(datafeedJob.isRunning(), is(true));
                    assertThat(datafeedJob.isIsolated(), is(false));
                    assertThat(datafeedJob.lastEndTimeMs(), equalTo(7_199_999L));
                    wasHandlerCalled.compareAndSet(false, true);
                }, e -> fail()
        );

        DatafeedContext datafeedContext = DatafeedContext.builder()
            .setDatafeedConfig(datafeed.build())
            .setJob(jobBuilder.build())
            .setRestartTimeInfo(new RestartTimeInfo(3_800_000L, 3_600_000L, false))
            .setTimingStats(new DatafeedTimingStats(jobBuilder.getId()))
            .build();

        TransportStartDatafeedAction.DatafeedTask datafeedTask = newDatafeedTask("datafeed1");

        datafeedJobBuilder.build(datafeedTask, datafeedContext, datafeedJobHandler);

        assertBusy(() -> wasHandlerCalled.get());
    }

    public void testBuildGivenRemoteIndicesButNoRemoteSearching() throws Exception {
        datafeedJobBuilder =
            new DatafeedJobBuilder(
                client,
                xContentRegistry(),
                auditor,
                annotationPersister,
                System::currentTimeMillis,
                jobResultsPersister,
                nonRemoteClusterClientNode(),
                "test_node");
        DataDescription.Builder dataDescription = new DataDescription.Builder();
        dataDescription.setTimeField("time");
        Job.Builder jobBuilder = DatafeedManagerTests.createDatafeedJob();
        jobBuilder.setDataDescription(dataDescription);
        jobBuilder.setCreateTime(new Date());
        DatafeedConfig.Builder datafeed = DatafeedManagerTests.createDatafeedConfig("datafeed1", jobBuilder.getId());
        datafeed.setIndices(Collections.singletonList("remotecluster:index-*"));

        AtomicBoolean wasHandlerCalled = new AtomicBoolean(false);
        ActionListener<DatafeedJob> datafeedJobHandler = ActionListener.wrap(
            datafeedJob -> fail("datafeed builder did not fail when remote index was given and remote clusters were not enabled"),
            e -> {
                assertThat(e.getMessage(), equalTo(Messages.getMessage(Messages.DATAFEED_NEEDS_REMOTE_CLUSTER_SEARCH,
                    "datafeed1",
                    "[remotecluster:index-*]",
                    "test_node")));
                wasHandlerCalled.compareAndSet(false, true);
            }
        );

        DatafeedContext datafeedContext = DatafeedContext.builder()
            .setDatafeedConfig(datafeed.build())
            .setJob(jobBuilder.build())
            .setRestartTimeInfo(new RestartTimeInfo(null, null, false))
            .setTimingStats(new DatafeedTimingStats(jobBuilder.getId()))
            .build();

        TransportStartDatafeedAction.DatafeedTask datafeedTask = newDatafeedTask("datafeed1");

        datafeedJobBuilder.build(datafeedTask, datafeedContext, datafeedJobHandler);
        assertBusy(() -> wasHandlerCalled.get());
    }

    private static TransportStartDatafeedAction.DatafeedTask newDatafeedTask(String datafeedId) {
        TransportStartDatafeedAction.DatafeedTask task = mock(TransportStartDatafeedAction.DatafeedTask.class);
        when(task.getDatafeedId()).thenReturn(datafeedId);
        TaskId parentTaskId = new TaskId("");
        when(task.getParentTaskId()).thenReturn(parentTaskId);
        return task;
    }
}
