/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.reservedstate.service;

import org.elasticsearch.Version;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.routing.RerouteService;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.env.Environment;
import org.elasticsearch.reservedstate.action.ReservedClusterSettingsAction;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.threadpool.TestThreadPool;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xcontent.XContentParser;
import org.junit.After;
import org.junit.Before;
import org.mockito.Mockito;
import org.mockito.stubbing.Answer;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.attribute.FileTime;
import java.time.Instant;
import java.time.LocalDateTime;
import java.time.ZoneId;
import java.time.ZoneOffset;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.function.Consumer;

import static org.hamcrest.Matchers.allOf;
import static org.hamcrest.Matchers.hasToString;
import static org.hamcrest.Matchers.instanceOf;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.clearInvocations;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.spy;
import static org.mockito.Mockito.times;
import static org.mockito.Mockito.verify;

public class FileSettingsServiceTests extends ESTestCase {
    private Environment env;
    private ClusterService clusterService;
    private FileSettingsService fileSettingsService;
    private ThreadPool threadpool;

    @Before
    public void setUp() throws Exception {
        super.setUp();

        threadpool = new TestThreadPool("file_settings_service_tests");

        clusterService = spy(
            new ClusterService(
                Settings.EMPTY,
                new ClusterSettings(Settings.EMPTY, ClusterSettings.BUILT_IN_CLUSTER_SETTINGS),
                threadpool,
                null
            )
        );

        final DiscoveryNode localNode = new DiscoveryNode("node", buildNewFakeTransportAddress(), Version.CURRENT);
        final ClusterState clusterState = ClusterState.builder(ClusterName.DEFAULT)
            .nodes(DiscoveryNodes.builder().add(localNode).localNodeId(localNode.getId()).masterNodeId(localNode.getId()))
            .build();
        doAnswer((Answer<ClusterState>) invocation -> clusterState).when(clusterService).state();

        clusterService.setRerouteService(mock(RerouteService.class));
        env = newEnvironment(Settings.EMPTY);

        Files.createDirectories(env.configFile());

        ClusterSettings clusterSettings = new ClusterSettings(Settings.EMPTY, ClusterSettings.BUILT_IN_CLUSTER_SETTINGS);

        ReservedClusterStateService controller = new ReservedClusterStateService(
            clusterService,
            List.of(new ReservedClusterSettingsAction(clusterSettings))
        );

        fileSettingsService = new FileSettingsService(clusterService, controller, env);
    }

    @After
    public void tearDown() throws Exception {
        super.tearDown();
        threadpool.shutdownNow();
    }

    public void testOperatorDirName() {
        Path operatorPath = fileSettingsService.operatorSettingsDir();
        assertTrue(operatorPath.startsWith(env.configFile()));
        assertTrue(operatorPath.endsWith("operator"));

        Path operatorSettingsFile = fileSettingsService.operatorSettingsFile();
        assertTrue(operatorSettingsFile.startsWith(operatorPath));
        assertTrue(operatorSettingsFile.endsWith("settings.json"));
    }

    public void testWatchedFile() throws Exception {
        Path tmpFile = createTempFile();
        Path tmpFile1 = createTempFile();
        Path otherFile = tmpFile.getParent().resolve("other.json");
        // we return false on non-existent paths, we don't remember state
        assertFalse(fileSettingsService.watchedFileChanged(otherFile));

        // we remember the previous state
        assertTrue(fileSettingsService.watchedFileChanged(tmpFile));
        assertFalse(fileSettingsService.watchedFileChanged(tmpFile));

        // we modify the timestamp of the file, it should trigger a change
        Instant now = LocalDateTime.now(ZoneId.systemDefault()).toInstant(ZoneOffset.ofHours(0));
        Files.setLastModifiedTime(tmpFile, FileTime.from(now));

        assertTrue(fileSettingsService.watchedFileChanged(tmpFile));
        assertFalse(fileSettingsService.watchedFileChanged(tmpFile));

        // we change to another real file, it should be changed
        assertTrue(fileSettingsService.watchedFileChanged(tmpFile1));
        assertFalse(fileSettingsService.watchedFileChanged(tmpFile1));
    }

    public void testStartStop() {
        fileSettingsService.start();
        assertTrue(fileSettingsService.watching());
        fileSettingsService.stop();
        assertFalse(fileSettingsService.watching());
        fileSettingsService.close();
    }

    public void testCallsProcessing() throws Exception {
        FileSettingsService service = spy(fileSettingsService);
        CountDownLatch processFileLatch = new CountDownLatch(1);

        doAnswer((Answer<Void>) invocation -> {
            processFileLatch.countDown();
            return null;
        }).when(service).processFileSettings(any(), any());

        service.start();
        assertTrue(service.watching());

        Files.createDirectories(service.operatorSettingsDir());

        Files.write(service.operatorSettingsFile(), "{}".getBytes(StandardCharsets.UTF_8));

        // we need to wait a bit, on MacOS it may take up to 10 seconds for the Java watcher service to notice the file,
        // on Linux is instantaneous. Windows is instantaneous too.
        processFileLatch.await(30, TimeUnit.SECONDS);

        verify(service, Mockito.atLeast(1)).watchedFileChanged(any());
        verify(service, times(1)).processFileSettings(any(), any());

        service.stop();
        assertFalse(service.watching());
        service.close();
    }

    @SuppressWarnings("unchecked")
    public void testInitialFile() throws Exception {
        ReservedClusterStateService stateService = mock(ReservedClusterStateService.class);

        doAnswer((Answer<Void>) invocation -> {
            ((Consumer<Exception>) invocation.getArgument(2)).accept(new IllegalStateException("Some exception"));
            return null;
        }).when(stateService).process(any(), (XContentParser) any(), any());

        FileSettingsService service = spy(new FileSettingsService(clusterService, stateService, env));

        Files.createDirectories(service.operatorSettingsDir());

        // contents of the JSON don't matter, we just need a file to exist
        Files.write(service.operatorSettingsFile(), "{}".getBytes(StandardCharsets.UTF_8));

        Exception startupException = expectThrows(IllegalStateException.class, () -> service.start());
        assertThat(
            startupException.getCause(),
            allOf(
                instanceOf(FileSettingsService.FileSettingsStartupException.class),
                hasToString(
                    "org.elasticsearch.reservedstate.service.FileSettingsService$FileSettingsStartupException: "
                        + "Error applying operator settings"
                )
            )
        );

        verify(service, times(1)).processFileSettings(any(), any());

        service.stop();

        clearInvocations(service);

        // Let's check that if we didn't throw an error that everything works
        doAnswer((Answer<Void>) invocation -> {
            ((Consumer<Exception>) invocation.getArgument(2)).accept(null);
            return null;
        }).when(stateService).process(any(), (XContentParser) any(), any());

        service.start();
        service.startWatcher(true);

        verify(service, times(1)).processFileSettings(any(), any());

        service.stop();
        service.close();
    }
}
