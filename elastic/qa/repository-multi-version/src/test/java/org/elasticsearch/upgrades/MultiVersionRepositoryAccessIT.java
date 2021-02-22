/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.upgrades;

import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.Version;
import org.elasticsearch.action.admin.cluster.repositories.put.PutRepositoryRequest;
import org.elasticsearch.action.admin.cluster.snapshots.delete.DeleteSnapshotRequest;
import org.elasticsearch.action.admin.cluster.snapshots.restore.RestoreSnapshotRequest;
import org.elasticsearch.action.admin.cluster.snapshots.status.SnapshotStatus;
import org.elasticsearch.action.admin.cluster.snapshots.status.SnapshotsStatusRequest;
import org.elasticsearch.action.admin.cluster.snapshots.status.SnapshotsStatusResponse;
import org.elasticsearch.client.Node;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.RequestOptions;
import org.elasticsearch.client.Response;
import org.elasticsearch.client.ResponseException;
import org.elasticsearch.client.RestClient;
import org.elasticsearch.client.RestHighLevelClient;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.DeprecationHandler;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.snapshots.RestoreInfo;
import org.elasticsearch.snapshots.SnapshotsService;
import org.elasticsearch.test.rest.ESRestTestCase;

import java.io.IOException;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

import static org.elasticsearch.repositories.blobstore.BlobStoreRepository.READONLY_SETTING_KEY;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasItem;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.is;

/**
 * Tests that verify that a snapshot repository is not getting corrupted and continues to function properly when accessed by multiple
 * clusters of different versions. Concretely this test suite is simulating the following scenario:
 * <ul>
 *     <li>Start and run against a cluster in an old version: {@link TestStep#STEP1_OLD_CLUSTER}</li>
 *     <li>Start and run against a cluster running the current version: {@link TestStep#STEP2_NEW_CLUSTER}</li>
 *     <li>Run against the old version cluster from the first step: {@link TestStep#STEP3_OLD_CLUSTER}</li>
 *     <li>Run against the current version cluster from the second step: {@link TestStep#STEP4_NEW_CLUSTER}</li>
 * </ul>
 */
public class MultiVersionRepositoryAccessIT extends ESRestTestCase {

    private enum TestStep {
        STEP1_OLD_CLUSTER("step1"),
        STEP2_NEW_CLUSTER("step2"),
        STEP3_OLD_CLUSTER("step3"),
        STEP4_NEW_CLUSTER("step4");

        private final String name;

        TestStep(String name) {
            this.name = name;
        }

        @Override
        public String toString() {
            return name;
        }

        public static TestStep parse(String value) {
            switch (value) {
                case "step1":
                    return STEP1_OLD_CLUSTER;
                case "step2":
                    return STEP2_NEW_CLUSTER;
                case "step3":
                    return STEP3_OLD_CLUSTER;
                case "step4":
                    return STEP4_NEW_CLUSTER;
                default:
                    throw new AssertionError("unknown test step: " + value);
            }
        }
    }

    private static final TestStep TEST_STEP = TestStep.parse(System.getProperty("tests.rest.suite"));

    @Override
    protected boolean preserveSnapshotsUponCompletion() {
        return true;
    }

    @Override
    protected boolean preserveReposUponCompletion() {
        return true;
    }

    public void testCreateAndRestoreSnapshot() throws IOException {
        final String repoName = getTestName();
        try (RestHighLevelClient client = new RestHighLevelClient(RestClient.builder(adminClient().getNodes().toArray(new Node[0])))) {
            final int shards = 3;
            final String index = "test-index";
            createIndex(client, index, shards);
            createRepository(client, repoName, false, true);
            createSnapshot(client, repoName, "snapshot-" + TEST_STEP, index);
            final String snapshotToDeleteName = "snapshot-to-delete";
            // Create a snapshot and delete it right away again to test the impact of each version's cleanup functionality that is run
            // as part of the snapshot delete
            createSnapshot(client, repoName, snapshotToDeleteName, index);
            final List<Map<String, Object>> snapshotsIncludingToDelete = listSnapshots(repoName);
            // Every step creates one snapshot and we have to add one more for the temporary snapshot
            assertThat(snapshotsIncludingToDelete, hasSize(TEST_STEP.ordinal() + 1 + 1));
            assertThat(snapshotsIncludingToDelete.stream().map(
                sn -> (String) sn.get("snapshot")).collect(Collectors.toList()), hasItem(snapshotToDeleteName));
            deleteSnapshot(client, repoName, snapshotToDeleteName);
            final List<Map<String, Object>> snapshots = listSnapshots(repoName);
            assertThat(snapshots, hasSize(TEST_STEP.ordinal() + 1));
            switch (TEST_STEP) {
                case STEP2_NEW_CLUSTER:
                case STEP4_NEW_CLUSTER:
                    assertSnapshotStatusSuccessful(client, repoName,
                        snapshots.stream().map(sn -> (String) sn.get("snapshot")).toArray(String[]::new));
                    break;
                case STEP1_OLD_CLUSTER:
                    assertSnapshotStatusSuccessful(client, repoName, "snapshot-" + TEST_STEP);
                    break;
                case STEP3_OLD_CLUSTER:
                    assertSnapshotStatusSuccessful(
                        client, repoName, "snapshot-" + TEST_STEP, "snapshot-" + TestStep.STEP3_OLD_CLUSTER);
                    break;
            }
            if (TEST_STEP == TestStep.STEP3_OLD_CLUSTER) {
                ensureSnapshotRestoreWorks(client, repoName, "snapshot-" + TestStep.STEP1_OLD_CLUSTER, shards);
            } else if (TEST_STEP == TestStep.STEP4_NEW_CLUSTER) {
                for (TestStep value : TestStep.values()) {
                    ensureSnapshotRestoreWorks(client, repoName, "snapshot-" + value, shards);
                }
            }
        } finally {
            deleteRepository(repoName);
        }
    }

    public void testReadOnlyRepo() throws IOException {
        final String repoName = getTestName();
        try (RestHighLevelClient client = new RestHighLevelClient(RestClient.builder(adminClient().getNodes().toArray(new Node[0])))) {
            final int shards = 3;
            final boolean readOnly = TEST_STEP.ordinal() > 1; // only restore from read-only repo in steps 3 and 4
            createRepository(client, repoName, readOnly, true);
            final String index = "test-index";
            if (readOnly == false) {
                createIndex(client, index, shards);
                createSnapshot(client, repoName, "snapshot-" + TEST_STEP, index);
            }
            final List<Map<String, Object>> snapshots = listSnapshots(repoName);
            switch (TEST_STEP) {
                case STEP1_OLD_CLUSTER:
                    assertThat(snapshots, hasSize(1));
                    break;
                case STEP2_NEW_CLUSTER:
                case STEP4_NEW_CLUSTER:
                case STEP3_OLD_CLUSTER:
                    assertThat(snapshots, hasSize(2));
                    break;
            }
            if (TEST_STEP == TestStep.STEP1_OLD_CLUSTER || TEST_STEP == TestStep.STEP3_OLD_CLUSTER) {
                assertSnapshotStatusSuccessful(client, repoName, "snapshot-" + TestStep.STEP1_OLD_CLUSTER);
            } else {
                assertSnapshotStatusSuccessful(client, repoName,
                    "snapshot-" + TestStep.STEP1_OLD_CLUSTER, "snapshot-" + TestStep.STEP2_NEW_CLUSTER);
            }
            if (TEST_STEP == TestStep.STEP3_OLD_CLUSTER) {
                ensureSnapshotRestoreWorks(client, repoName, "snapshot-" + TestStep.STEP1_OLD_CLUSTER, shards);
            } else if (TEST_STEP == TestStep.STEP4_NEW_CLUSTER) {
                ensureSnapshotRestoreWorks(client, repoName, "snapshot-" + TestStep.STEP1_OLD_CLUSTER, shards);
                ensureSnapshotRestoreWorks(client, repoName, "snapshot-" + TestStep.STEP2_NEW_CLUSTER, shards);
            }
        }
    }

    private static final List<Class<? extends Exception>> EXPECTED_BWC_EXCEPTIONS =
            List.of(ResponseException.class, ElasticsearchStatusException.class);

    public void testUpgradeMovesRepoToNewMetaVersion() throws IOException {
        final String repoName = getTestName();
        try (RestHighLevelClient client = new RestHighLevelClient(RestClient.builder(adminClient().getNodes().toArray(new Node[0])))) {
            final int shards = 3;
            final String index=  "test-index";
            createIndex(client, index, shards);
            final Version minNodeVersion = minimumNodeVersion();
            // 7.12.0+ will try to load RepositoryData during repo creation if verify is true, which is impossible in case of version
            // incompatibility in the downgrade test step. We verify that it is impossible here and then create the repo using verify=false
            // to check behavior on other operations below.
            final boolean verify = TEST_STEP != TestStep.STEP3_OLD_CLUSTER || SnapshotsService.includesUUIDs(minNodeVersion)
                    || minNodeVersion.before(Version.V_7_12_0);
            if (verify == false) {
                expectThrowsAnyOf(EXPECTED_BWC_EXCEPTIONS, () -> createRepository(client, repoName, false, true));
            }
            createRepository(client, repoName, false, verify);
            // only create some snapshots in the first two steps
            if (TEST_STEP == TestStep.STEP1_OLD_CLUSTER || TEST_STEP == TestStep.STEP2_NEW_CLUSTER) {
                createSnapshot(client, repoName, "snapshot-" + TEST_STEP, index);
                final List<Map<String, Object>> snapshots = listSnapshots(repoName);
                // Every step creates one snapshot
                assertThat(snapshots, hasSize(TEST_STEP.ordinal() + 1));
                assertSnapshotStatusSuccessful(client, repoName,
                    snapshots.stream().map(sn -> (String) sn.get("snapshot")).toArray(String[]::new));
                if (TEST_STEP == TestStep.STEP1_OLD_CLUSTER) {
                    ensureSnapshotRestoreWorks(client, repoName, "snapshot-" + TestStep.STEP1_OLD_CLUSTER, shards);
                } else {
                    deleteSnapshot(client, repoName, "snapshot-" + TestStep.STEP1_OLD_CLUSTER);
                    ensureSnapshotRestoreWorks(client, repoName, "snapshot-" + TestStep.STEP2_NEW_CLUSTER, shards);
                    createSnapshot(client, repoName, "snapshot-1", index);
                    ensureSnapshotRestoreWorks(client, repoName, "snapshot-1", shards);
                    deleteSnapshot(client, repoName, "snapshot-" + TestStep.STEP2_NEW_CLUSTER);
                    createSnapshot(client, repoName, "snapshot-2", index);
                    ensureSnapshotRestoreWorks(client, repoName, "snapshot-2", shards);
                }
            } else {
                if (SnapshotsService.includesUUIDs(minNodeVersion) == false) {
                    assertThat(TEST_STEP, is(TestStep.STEP3_OLD_CLUSTER));
                    expectThrowsAnyOf(EXPECTED_BWC_EXCEPTIONS, () -> listSnapshots(repoName));
                    expectThrowsAnyOf(EXPECTED_BWC_EXCEPTIONS, () -> deleteSnapshot(client, repoName, "snapshot-1"));
                    expectThrowsAnyOf(EXPECTED_BWC_EXCEPTIONS, () -> deleteSnapshot(client, repoName, "snapshot-2"));
                    expectThrowsAnyOf(EXPECTED_BWC_EXCEPTIONS, () -> createSnapshot(client, repoName, "snapshot-impossible", index));
                } else {
                    assertThat(listSnapshots(repoName), hasSize(2));
                    if (TEST_STEP == TestStep.STEP4_NEW_CLUSTER) {
                        ensureSnapshotRestoreWorks(client, repoName, "snapshot-1", shards);
                        ensureSnapshotRestoreWorks(client, repoName, "snapshot-2", shards);
                    }
                }
            }
        } finally {
            deleteRepository(repoName);
        }
    }

    private static void assertSnapshotStatusSuccessful(RestHighLevelClient client, String repoName,
                                                       String... snapshots) throws IOException {
        final SnapshotsStatusResponse statusResponse = client.snapshot()
            .status(new SnapshotsStatusRequest(repoName, snapshots), RequestOptions.DEFAULT);
        for (SnapshotStatus status : statusResponse.getSnapshots()) {
            assertThat(status.getShardsStats().getFailedShards(), is(0));
        }
    }

    private void deleteSnapshot(RestHighLevelClient client, String repoName, String name) throws IOException {
        assertThat(client.snapshot().delete(new DeleteSnapshotRequest(repoName, name), RequestOptions.DEFAULT).isAcknowledged(), is(true));
    }

    @SuppressWarnings("unchecked")
    private List<Map<String, Object>> listSnapshots(String repoName) throws IOException {
        try (InputStream entity = client().performRequest(
            new Request("GET", "/_snapshot/" + repoName + "/_all")).getEntity().getContent();
             XContentParser parser = JsonXContent.jsonXContent.createParser(
                 xContentRegistry(), DeprecationHandler.THROW_UNSUPPORTED_OPERATION, entity)) {
            final Map<String, Object> raw = parser.map();
            // Bwc lookup since the format of the snapshots response changed between versions
            if (raw.containsKey("snapshots")) {
                return (List<Map<String, Object>>) raw.get("snapshots");
            } else {
                return (List<Map<String, Object>>) ((List<Map<?, ?>>) raw.get("responses")).get(0).get("snapshots");
            }
        }
    }

    private static void ensureSnapshotRestoreWorks(RestHighLevelClient client, String repoName, String name,
                                                   int shards) throws IOException {
        wipeAllIndices();
        final RestoreInfo restoreInfo =
            client.snapshot().restore(new RestoreSnapshotRequest().repository(repoName).snapshot(name).waitForCompletion(true),
                RequestOptions.DEFAULT).getRestoreInfo();
        assertThat(restoreInfo.failedShards(), is(0));
        assertThat(restoreInfo.successfulShards(), equalTo(shards));
    }

    private static void createRepository(RestHighLevelClient client, String repoName, boolean readOnly,
                                         boolean verify) throws IOException {
        assertThat(client.snapshot().createRepository(new PutRepositoryRequest(repoName).type("fs").settings(
            Settings.builder().put("location", "./" + repoName).put(READONLY_SETTING_KEY, readOnly)).verify(verify), RequestOptions.DEFAULT)
                .isAcknowledged(),
            is(true));
    }

    private static void createSnapshot(RestHighLevelClient client, String repoName, String name, String index) throws IOException {
        final Request createSnapshotRequest = new Request("PUT", "/_snapshot/" + repoName + "/" + name);
        createSnapshotRequest.addParameter("wait_for_completion", "true");
        createSnapshotRequest.setJsonEntity("{ \"indices\" : \"" + index + "\"}");
        final Response response = client.getLowLevelClient().performRequest(createSnapshotRequest);
        assertThat(response.getStatusLine().getStatusCode(), is(HttpURLConnection.HTTP_OK));
    }

    private void createIndex(RestHighLevelClient client, String name, int shards) throws IOException {
        final Request putIndexRequest = new Request("PUT", "/" + name);
        putIndexRequest.setJsonEntity("{\n" +
            "    \"settings\" : {\n" +
            "        \"index\" : {\n" +
            "            \"number_of_shards\" : " + shards + ", \n" +
            "            \"number_of_replicas\" : 0 \n" +
            "        }\n" +
            "    }\n" +
            "}");
        final Response response = client.getLowLevelClient().performRequest(putIndexRequest);
        assertThat(response.getStatusLine().getStatusCode(), is(HttpURLConnection.HTTP_OK));
    }
}
