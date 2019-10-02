/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.slm;

import org.elasticsearch.action.admin.cluster.snapshots.status.SnapshotStatus;
import org.elasticsearch.action.admin.cluster.snapshots.status.SnapshotsStatusResponse;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.SnapshotsInProgress;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.plugins.Plugin;
import org.elasticsearch.repositories.RepositoriesService;
import org.elasticsearch.snapshots.ConcurrentSnapshotExecutionException;
import org.elasticsearch.snapshots.SnapshotMissingException;
import org.elasticsearch.snapshots.mockstore.MockRepository;
import org.elasticsearch.test.ESIntegTestCase;
import org.elasticsearch.test.junit.annotations.TestLogging;
import org.elasticsearch.xpack.core.LocalStateCompositeXPackPlugin;
import org.elasticsearch.xpack.core.slm.SnapshotLifecyclePolicy;
import org.elasticsearch.xpack.core.slm.SnapshotLifecyclePolicyItem;
import org.elasticsearch.xpack.core.slm.SnapshotRetentionConfiguration;
import org.elasticsearch.xpack.core.slm.action.ExecuteSnapshotLifecycleAction;
import org.elasticsearch.xpack.core.slm.action.ExecuteSnapshotRetentionAction;
import org.elasticsearch.xpack.core.slm.action.GetSnapshotLifecycleAction;
import org.elasticsearch.xpack.core.slm.action.PutSnapshotLifecycleAction;
import org.elasticsearch.xpack.ilm.IndexLifecycle;
import org.junit.After;

import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.Map;

import static org.hamcrest.Matchers.anyOf;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;

/**
 * Tests for Snapshot Lifecycle Management that require a slow or blocked snapshot repo (using {@link MockRepository}
 */
@TestLogging(value = "org.elasticsearch.xpack.slm:TRACE,org.elasticsearch.xpack.core.slm:TRACE",
    reason = "https://github.com/elastic/elasticsearch/issues/46508")
public class SLMSnapshotBlockingIntegTests extends ESIntegTestCase {

    static final String REPO = "my-repo";

    @After
    public void resetSLMSettings() throws Exception {
        // Cancel/delete all snapshots
        assertBusy(() -> {
            logger.info("--> wiping all snapshots after test");
            client().admin().cluster().prepareGetSnapshots(REPO).get().getSnapshots(REPO)
                .forEach(snapshotInfo -> {
                    try {
                        client().admin().cluster().prepareDeleteSnapshot(REPO, snapshotInfo.snapshotId().getName()).get();
                    } catch (Exception e) {
                        logger.warn("exception cleaning up snapshot " + snapshotInfo.snapshotId().getName(), e);
                        fail("exception cleanup up snapshot");
                    }
                });
        });
    }

    @Override
    protected Collection<Class<? extends Plugin>> nodePlugins() {
        return Arrays.asList(MockRepository.Plugin.class, LocalStateCompositeXPackPlugin.class, IndexLifecycle.class);
    }

    public void testSnapshotInProgress() throws Exception {
        final String indexName = "test";
        final String policyName = "test-policy";
        int docCount = 20;
        for (int i = 0; i < docCount; i++) {
            index(indexName, "_doc", i + "", Collections.singletonMap("foo", "bar"));
        }

        // Create a snapshot repo
        initializeRepo(REPO);

        logger.info("--> creating policy {}", policyName);
        createSnapshotPolicy(policyName, "snap", "1 2 3 4 5 ?", REPO, indexName, true);

        logger.info("--> blocking master from completing snapshot");
        blockMasterFromFinalizingSnapshotOnIndexFile(REPO);

        logger.info("--> executing snapshot lifecycle");
        final String snapshotName = executePolicy(policyName);

        // Check that the executed snapshot shows up in the SLM output
        assertBusy(() -> {
            GetSnapshotLifecycleAction.Response getResp =
                client().execute(GetSnapshotLifecycleAction.INSTANCE, new GetSnapshotLifecycleAction.Request(policyName)).get();
            logger.info("--> checking for in progress snapshot...");

            assertThat(getResp.getPolicies().size(), greaterThan(0));
            SnapshotLifecyclePolicyItem item = getResp.getPolicies().get(0);
            assertNotNull(item.getSnapshotInProgress());
            SnapshotLifecyclePolicyItem.SnapshotInProgress inProgress = item.getSnapshotInProgress();
            assertThat(inProgress.getSnapshotId().getName(), equalTo(snapshotName));
            assertThat(inProgress.getStartTime(), greaterThan(0L));
            assertThat(inProgress.getState(), anyOf(equalTo(SnapshotsInProgress.State.INIT), equalTo(SnapshotsInProgress.State.STARTED)));
            assertNull(inProgress.getFailure());
        });

        logger.info("--> unblocking snapshots");
        unblockRepo(REPO);

        // Cancel/delete the snapshot
        try {
            client().admin().cluster().prepareDeleteSnapshot(REPO, snapshotName).get();
        } catch (SnapshotMissingException e) {
            // ignore
        }
    }

    public void testRetentionWhileSnapshotInProgress() throws Exception {
        final String indexName = "test";
        final String policyId = "slm-policy";
        int docCount = 20;
        for (int i = 0; i < docCount; i++) {
            index(indexName, "_doc", i + "", Collections.singletonMap("foo", "bar"));
        }

        initializeRepo(REPO);

        logger.info("--> creating policy {}", policyId);
        createSnapshotPolicy(policyId, "snap", "1 2 3 4 5 ?", REPO, indexName, true,
            new SnapshotRetentionConfiguration(TimeValue.timeValueSeconds(0), null, null));

        // Create a snapshot and wait for it to be complete (need something that can be deleted)
        final String completedSnapshotName = executePolicy(policyId);
        logger.info("--> kicked off snapshot {}", completedSnapshotName);
        assertBusy(() -> {
            try {
                SnapshotsStatusResponse s =
                    client().admin().cluster().prepareSnapshotStatus(REPO).setSnapshots(completedSnapshotName).get();
                assertThat("expected a snapshot but none were returned", s.getSnapshots().size(), equalTo(1));
                SnapshotStatus status = s.getSnapshots().get(0);
                logger.info("--> waiting for snapshot {} to be completed, got: {}", completedSnapshotName, status.getState());
                assertThat(status.getState(), equalTo(SnapshotsInProgress.State.SUCCESS));
            } catch (SnapshotMissingException e) {
                logger.error("expected a snapshot but it was missing", e);
                fail("expected a snapshot with name " + completedSnapshotName + " but it does not exist");
            }
        });

        // Wait for all running snapshots to be cleared from cluster state
        assertBusy(() -> {
            logger.info("--> waiting for cluster state to be clear of snapshots");
            ClusterState state = client().admin().cluster().prepareState().setCustoms(true).get().getState();
            assertTrue("cluster state was not ready for deletion " + state, SnapshotRetentionTask.okayToDeleteSnapshots(state));
        });

        // Take another snapshot, but before doing that, block it from completing
        logger.info("--> blocking nodes from completing snapshot");
        try {
            blockAllDataNodes(REPO);
            final String secondSnapName = executePolicy(policyId);

            // Check that the executed snapshot shows up in the SLM output as in_progress
            assertBusy(() -> {
                GetSnapshotLifecycleAction.Response getResp =
                    client().execute(GetSnapshotLifecycleAction.INSTANCE, new GetSnapshotLifecycleAction.Request(policyId)).get();
                logger.info("--> checking for in progress snapshot...");

                assertThat(getResp.getPolicies().size(), greaterThan(0));
                SnapshotLifecyclePolicyItem item = getResp.getPolicies().get(0);
                assertNotNull(item.getSnapshotInProgress());
                SnapshotLifecyclePolicyItem.SnapshotInProgress inProgress = item.getSnapshotInProgress();
                assertThat(inProgress.getSnapshotId().getName(), equalTo(secondSnapName));
                assertThat(inProgress.getStartTime(), greaterThan(0L));
                assertThat(inProgress.getState(), anyOf(equalTo(SnapshotsInProgress.State.INIT),
                    equalTo(SnapshotsInProgress.State.STARTED)));
                assertNull(inProgress.getFailure());
            });

            // Run retention
            logger.info("--> triggering retention");
            assertTrue(client().execute(ExecuteSnapshotRetentionAction.INSTANCE,
                new ExecuteSnapshotRetentionAction.Request()).get().isAcknowledged());

            logger.info("--> unblocking snapshots");
            unblockRepo(REPO);
            unblockAllDataNodes(REPO);

            // Check that the snapshot created by the policy has been removed by retention
            assertBusy(() -> {
                // Trigger a cluster state update so that it re-checks for a snapshot in progress
                client().admin().cluster().prepareReroute().get();
                logger.info("--> waiting for snapshot to be deleted");
                try {
                    SnapshotsStatusResponse s =
                        client().admin().cluster().prepareSnapshotStatus(REPO).setSnapshots(completedSnapshotName).get();
                    assertNull("expected no snapshot but one was returned", s.getSnapshots().get(0));
                } catch (SnapshotMissingException e) {
                    // Great, we wanted it to be deleted!
                }
            });

            // Cancel the ongoing snapshot to cancel it
            assertBusy(() -> {
                try {
                    logger.info("--> cancelling snapshot {}", secondSnapName);
                    client().admin().cluster().prepareDeleteSnapshot(REPO, secondSnapName).get();
                } catch (ConcurrentSnapshotExecutionException e) {
                    logger.info("--> attempted to stop second snapshot", e);
                    // just wait and retry
                    fail("attempted to stop second snapshot but a snapshot or delete was in progress");
                }
            });

            // Assert that the history document has been written for taking the snapshot and deleting it
            assertBusy(() -> {
                SearchResponse resp = client().prepareSearch(".slm-history*")
                    .setQuery(QueryBuilders.matchQuery("snapshot_name", completedSnapshotName)).get();
                logger.info("--> checking history written for {}, got: {}",
                    completedSnapshotName, Strings.arrayToCommaDelimitedString(resp.getHits().getHits()));
                assertThat(resp.getHits().getTotalHits().value, equalTo(2L));
            });
        } finally {
            unblockRepo(REPO);
            unblockAllDataNodes(REPO);
        }
    }

    private void initializeRepo(String repoName) {
        client().admin().cluster().preparePutRepository(repoName)
            .setType("mock")
            .setSettings(Settings.builder()
                .put("compress", randomBoolean())
                .put("location", randomAlphaOfLength(6))
                .build())
            .get();
    }

    private void createSnapshotPolicy(String policyName, String snapshotNamePattern, String schedule, String repoId,
                                      String indexPattern, boolean ignoreUnavailable) {
        createSnapshotPolicy(policyName, snapshotNamePattern, schedule, repoId, indexPattern,
            ignoreUnavailable, SnapshotRetentionConfiguration.EMPTY);
    }

    private void createSnapshotPolicy(String policyName, String snapshotNamePattern, String schedule, String repoId,
                                      String indexPattern, boolean ignoreUnavailable,
                                      SnapshotRetentionConfiguration retention) {
        Map<String, Object> snapConfig = new HashMap<>();
        snapConfig.put("indices", Collections.singletonList(indexPattern));
        snapConfig.put("ignore_unavailable", ignoreUnavailable);
        if (randomBoolean()) {
            Map<String, Object> metadata = new HashMap<>();
            int fieldCount = randomIntBetween(2,5);
            for (int i = 0; i < fieldCount; i++) {
                metadata.put(randomValueOtherThanMany(key -> "policy".equals(key) || metadata.containsKey(key),
                    () -> randomAlphaOfLength(5)), randomAlphaOfLength(4));
            }
        }
        SnapshotLifecyclePolicy policy = new SnapshotLifecyclePolicy(policyName, snapshotNamePattern, schedule,
            repoId, snapConfig, retention);

        PutSnapshotLifecycleAction.Request putLifecycle = new PutSnapshotLifecycleAction.Request(policyName, policy);
        try {
            client().execute(PutSnapshotLifecycleAction.INSTANCE, putLifecycle).get();
        } catch (Exception e) {
            logger.error("failed to create slm policy", e);
            fail("failed to create policy " + policy + " got: " + e);
        }
    }

    /**
     * Execute the given policy and return the generated snapshot name
     */
    private String executePolicy(String policyId) {
        ExecuteSnapshotLifecycleAction.Request executeReq = new ExecuteSnapshotLifecycleAction.Request(policyId);
        ExecuteSnapshotLifecycleAction.Response resp = null;
        try {
            resp = client().execute(ExecuteSnapshotLifecycleAction.INSTANCE, executeReq).get();
            return resp.getSnapshotName();
        } catch (Exception e) {
            logger.error("failed to execute policy", e);
            fail("failed to execute policy " + policyId + " got: " + e);
            return "bad";
        }
    }

    public static String blockMasterFromFinalizingSnapshotOnIndexFile(final String repositoryName) {
        final String masterName = internalCluster().getMasterName();
        ((MockRepository)internalCluster().getInstance(RepositoriesService.class, masterName)
            .repository(repositoryName)).setBlockOnWriteIndexFile(true);
        return masterName;
    }

    public static String unblockRepo(final String repositoryName) {
        final String masterName = internalCluster().getMasterName();
        ((MockRepository)internalCluster().getInstance(RepositoriesService.class, masterName)
            .repository(repositoryName)).unblock();
        return masterName;
    }

    public static void blockAllDataNodes(String repository) {
        for(RepositoriesService repositoriesService : internalCluster().getDataNodeInstances(RepositoriesService.class)) {
            ((MockRepository)repositoriesService.repository(repository)).blockOnDataFiles(true);
        }
    }

    public static void unblockAllDataNodes(String repository) {
        for(RepositoriesService repositoriesService : internalCluster().getDataNodeInstances(RepositoriesService.class)) {
            ((MockRepository)repositoriesService.repository(repository)).unblock();
        }
    }
}
