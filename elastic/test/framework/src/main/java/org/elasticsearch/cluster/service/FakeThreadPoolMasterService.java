/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.cluster.service;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStatePublicationEvent;
import org.elasticsearch.cluster.coordination.ClusterStatePublisher.AckListener;
import org.elasticsearch.common.Priority;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.PrioritizedEsThreadPoolExecutor;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.node.Node;
import org.elasticsearch.tasks.TaskManager;
import org.elasticsearch.threadpool.ThreadPool;

import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.concurrent.TimeUnit;
import java.util.function.Consumer;

import static org.apache.lucene.tests.util.LuceneTestCase.random;
import static org.elasticsearch.test.ESTestCase.randomInt;

public class FakeThreadPoolMasterService extends MasterService {
    private static final Logger logger = LogManager.getLogger(FakeThreadPoolMasterService.class);

    private final String name;
    private final List<Runnable> pendingTasks = new ArrayList<>();
    private final Consumer<Runnable> onTaskAvailableToRun;
    private boolean scheduledNextTask = false;
    private boolean taskInProgress = false;
    private boolean waitForPublish = false;

    public FakeThreadPoolMasterService(
        String nodeName,
        String serviceName,
        ThreadPool threadPool,
        Consumer<Runnable> onTaskAvailableToRun
    ) {
        this(
            Settings.builder().put(Node.NODE_NAME_SETTING.getKey(), nodeName).build(),
            new ClusterSettings(Settings.EMPTY, ClusterSettings.BUILT_IN_CLUSTER_SETTINGS),
            threadPool,
            serviceName,
            onTaskAvailableToRun
        );
    }

    private FakeThreadPoolMasterService(
        Settings settings,
        ClusterSettings clusterSettings,
        ThreadPool threadPool,
        String serviceName,
        Consumer<Runnable> onTaskAvailableToRun
    ) {
        super(settings, clusterSettings, threadPool, new TaskManager(settings, threadPool, Set.of()));
        this.name = serviceName;
        this.onTaskAvailableToRun = onTaskAvailableToRun;
    }

    @Override
    protected PrioritizedEsThreadPoolExecutor createThreadPoolExecutor() {
        return new PrioritizedEsThreadPoolExecutor(
            name,
            1,
            1,
            1,
            TimeUnit.SECONDS,
            r -> { throw new AssertionError("should not create new threads"); },
            null,
            null
        ) {

            @Override
            public void execute(Runnable command, final TimeValue timeout, final Runnable timeoutCallback) {
                execute(command);
            }

            @Override
            public void execute(Runnable command) {
                if (command.toString().equals("awaitsfix thread keepalive") == false) {
                    // TODO remove this temporary fix
                    pendingTasks.add(command);
                    scheduleNextTaskIfNecessary();
                }
            }

            @Override
            public Pending[] getPending() {
                return pendingTasks.stream().map(r -> new Pending(r, Priority.NORMAL, 0L, false)).toArray(Pending[]::new);
            }
        };
    }

    private void scheduleNextTaskIfNecessary() {
        if (taskInProgress == false && pendingTasks.isEmpty() == false && scheduledNextTask == false) {
            scheduledNextTask = true;
            onTaskAvailableToRun.accept(new Runnable() {
                @Override
                public String toString() {
                    return "master service scheduling next task";
                }

                @Override
                public void run() {
                    assert taskInProgress == false;
                    assert waitForPublish == false;
                    assert scheduledNextTask;
                    final int taskIndex = randomInt(pendingTasks.size() - 1);
                    logger.debug("next master service task: choosing task {} of {}", taskIndex, pendingTasks.size());
                    final Runnable task = pendingTasks.remove(taskIndex);
                    taskInProgress = true;
                    scheduledNextTask = false;
                    final ThreadContext threadContext = threadPool.getThreadContext();
                    try (ThreadContext.StoredContext ignored = threadContext.stashContext()) {
                        threadContext.markAsSystemContext();
                        task.run();
                    }
                    if (waitForPublish == false) {
                        taskInProgress = false;
                    }
                    FakeThreadPoolMasterService.this.scheduleNextTaskIfNecessary();
                }
            });
        }
    }

    @Override
    public ClusterState.Builder incrementVersion(ClusterState clusterState) {
        // generate cluster UUID deterministically for repeatable tests
        return ClusterState.builder(clusterState).incrementVersion().stateUUID(UUIDs.randomBase64UUID(random()));
    }

    @Override
    protected void publish(
        ClusterStatePublicationEvent clusterStatePublicationEvent,
        AckListener ackListener,
        ActionListener<Void> publicationListener
    ) {
        assert waitForPublish == false;
        waitForPublish = true;
        final ActionListener<Void> publishListener = new ActionListener<>() {

            private boolean listenerCalled = false;

            @Override
            public void onResponse(Void aVoid) {
                assert listenerCalled == false;
                listenerCalled = true;
                assert waitForPublish;
                waitForPublish = false;
                try {
                    publicationListener.onResponse(null);
                } finally {
                    taskInProgress = false;
                    scheduleNextTaskIfNecessary();
                }
            }

            @Override
            public void onFailure(Exception e) {
                assert listenerCalled == false;
                listenerCalled = true;
                assert waitForPublish;
                waitForPublish = false;
                try {
                    publicationListener.onFailure(e);
                } finally {
                    taskInProgress = false;
                    scheduleNextTaskIfNecessary();
                }
            }
        };
        threadPool.generic().execute(threadPool.getThreadContext().preserveContext(new Runnable() {
            @Override
            public void run() {
                clusterStatePublisher.publish(clusterStatePublicationEvent, publishListener, wrapAckListener(ackListener));
            }

            @Override
            public String toString() {
                return "publish change of cluster state from version ["
                    + clusterStatePublicationEvent.getOldState().version()
                    + "] in term ["
                    + clusterStatePublicationEvent.getOldState().term()
                    + "] to version ["
                    + clusterStatePublicationEvent.getNewState().version()
                    + "] in term ["
                    + clusterStatePublicationEvent.getNewState().term()
                    + "]";
            }
        }));
    }

    protected AckListener wrapAckListener(AckListener ackListener) {
        return ackListener;
    }
}
