/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.snapshots;

import org.elasticsearch.action.ActionFuture;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.admin.cluster.snapshots.create.CreateSnapshotResponse;
import org.elasticsearch.action.admin.cluster.snapshots.get.GetSnapshotsRequest;
import org.elasticsearch.action.admin.cluster.snapshots.get.GetSnapshotsRequestBuilder;
import org.elasticsearch.action.admin.cluster.snapshots.get.GetSnapshotsResponse;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.search.sort.SortOrder;
import org.elasticsearch.threadpool.ThreadPool;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.HashSet;
import java.util.List;
import java.util.Map;

import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.in;
import static org.hamcrest.Matchers.is;

public class GetSnapshotsIT extends AbstractSnapshotIntegTestCase {

    @Override
    protected Settings nodeSettings(int nodeOrdinal, Settings otherSettings) {
        return Settings.builder()
            .put(super.nodeSettings(nodeOrdinal, otherSettings))
            .put(ThreadPool.ESTIMATED_TIME_INTERVAL_SETTING.getKey(), 0) // We have tests that check by-timestamp order
            .build();
    }

    public void testSortBy() throws Exception {
        final String repoNameA = "test-repo-a";
        final Path repoPath = randomRepoPath();
        createRepository(repoNameA, "fs", repoPath);
        maybeInitWithOldSnapshotVersion(repoNameA, repoPath);
        final String repoNameB = "test-repo-b";
        createRepository(repoNameB, "fs");

        final List<String> snapshotNamesWithoutIndexA = createNSnapshots(repoNameA, randomIntBetween(3, 20));
        final List<String> snapshotNamesWithoutIndexB = createNSnapshots(repoNameB, randomIntBetween(3, 20));

        createIndexWithContent("test-index");

        final List<String> snapshotNamesWithIndexA = createNSnapshots(repoNameA, randomIntBetween(3, 20));
        final List<String> snapshotNamesWithIndexB = createNSnapshots(repoNameB, randomIntBetween(3, 20));

        final Collection<String> allSnapshotNamesA = new HashSet<>(snapshotNamesWithIndexA);
        final Collection<String> allSnapshotNamesB = new HashSet<>(snapshotNamesWithIndexB);
        allSnapshotNamesA.addAll(snapshotNamesWithoutIndexA);
        allSnapshotNamesB.addAll(snapshotNamesWithoutIndexB);

        doTestSortOrder(repoNameA, allSnapshotNamesA, SortOrder.ASC);
        doTestSortOrder(repoNameA, allSnapshotNamesA, SortOrder.DESC);

        doTestSortOrder(repoNameB, allSnapshotNamesB, SortOrder.ASC);
        doTestSortOrder(repoNameB, allSnapshotNamesB, SortOrder.DESC);

        final Collection<String> allSnapshots = new HashSet<>(allSnapshotNamesA);
        allSnapshots.addAll(allSnapshotNamesB);
        doTestSortOrder("*", allSnapshots, SortOrder.ASC);
        doTestSortOrder("*", allSnapshots, SortOrder.DESC);
    }

    private void doTestSortOrder(String repoName, Collection<String> allSnapshotNames, SortOrder order) {
        final List<SnapshotInfo> defaultSorting = clusterAdmin().prepareGetSnapshots(repoName).setOrder(order).get().getSnapshots();
        assertSnapshotListSorted(defaultSorting, null, order);
        assertSnapshotListSorted(
            allSnapshotsSorted(allSnapshotNames, repoName, GetSnapshotsRequest.SortBy.NAME, order),
            GetSnapshotsRequest.SortBy.NAME,
            order
        );
        assertSnapshotListSorted(
            allSnapshotsSorted(allSnapshotNames, repoName, GetSnapshotsRequest.SortBy.DURATION, order),
            GetSnapshotsRequest.SortBy.DURATION,
            order
        );
        assertSnapshotListSorted(
            allSnapshotsSorted(allSnapshotNames, repoName, GetSnapshotsRequest.SortBy.INDICES, order),
            GetSnapshotsRequest.SortBy.INDICES,
            order
        );
        assertSnapshotListSorted(
            allSnapshotsSorted(allSnapshotNames, repoName, GetSnapshotsRequest.SortBy.START_TIME, order),
            GetSnapshotsRequest.SortBy.START_TIME,
            order
        );
        assertSnapshotListSorted(
            allSnapshotsSorted(allSnapshotNames, repoName, GetSnapshotsRequest.SortBy.SHARDS, order),
            GetSnapshotsRequest.SortBy.SHARDS,
            order
        );
        assertSnapshotListSorted(
            allSnapshotsSorted(allSnapshotNames, repoName, GetSnapshotsRequest.SortBy.FAILED_SHARDS, order),
            GetSnapshotsRequest.SortBy.FAILED_SHARDS,
            order
        );
        assertSnapshotListSorted(
            allSnapshotsSorted(allSnapshotNames, repoName, GetSnapshotsRequest.SortBy.REPOSITORY, order),
            GetSnapshotsRequest.SortBy.REPOSITORY,
            order
        );
    }

    public void testResponseSizeLimit() throws Exception {
        final String repoName = "test-repo";
        final Path repoPath = randomRepoPath();
        createRepository(repoName, "fs", repoPath);
        maybeInitWithOldSnapshotVersion(repoName, repoPath);
        final List<String> names = createNSnapshots(repoName, randomIntBetween(6, 20));
        for (GetSnapshotsRequest.SortBy sort : GetSnapshotsRequest.SortBy.values()) {
            for (SortOrder order : SortOrder.values()) {
                logger.info("--> testing pagination for [{}] [{}]", sort, order);
                doTestPagination(repoName, names, sort, order);
            }
        }
    }

    private void doTestPagination(String repoName, List<String> names, GetSnapshotsRequest.SortBy sort, SortOrder order) {
        final List<SnapshotInfo> allSnapshotsSorted = allSnapshotsSorted(names, repoName, sort, order);
        final GetSnapshotsResponse batch1 = sortedWithLimit(repoName, sort, null, 2, order);
        assertEquals(allSnapshotsSorted.subList(0, 2), batch1.getSnapshots());
        final GetSnapshotsResponse batch2 = sortedWithLimit(repoName, sort, batch1.next(), 2, order);
        assertEquals(allSnapshotsSorted.subList(2, 4), batch2.getSnapshots());
        final int lastBatch = names.size() - batch1.getSnapshots().size() - batch2.getSnapshots().size();
        final GetSnapshotsResponse batch3 = sortedWithLimit(repoName, sort, batch2.next(), lastBatch, order);
        assertEquals(
            batch3.getSnapshots(),
            allSnapshotsSorted.subList(batch1.getSnapshots().size() + batch2.getSnapshots().size(), names.size())
        );
        final GetSnapshotsResponse batch3NoLimit = sortedWithLimit(repoName, sort, batch2.next(), GetSnapshotsRequest.NO_LIMIT, order);
        assertNull(batch3NoLimit.next());
        assertEquals(batch3.getSnapshots(), batch3NoLimit.getSnapshots());
        final GetSnapshotsResponse batch3LargeLimit = sortedWithLimit(
            repoName,
            sort,
            batch2.next(),
            lastBatch + randomIntBetween(1, 100),
            order
        );
        assertEquals(batch3.getSnapshots(), batch3LargeLimit.getSnapshots());
        assertNull(batch3LargeLimit.next());
    }

    public void testSortAndPaginateWithInProgress() throws Exception {
        final String repoName = "test-repo";
        final Path repoPath = randomRepoPath();
        createRepository(repoName, "mock", repoPath);
        maybeInitWithOldSnapshotVersion(repoName, repoPath);
        final Collection<String> allSnapshotNames = new HashSet<>(createNSnapshots(repoName, randomIntBetween(3, 20)));
        createIndexWithContent("test-index-1");
        allSnapshotNames.addAll(createNSnapshots(repoName, randomIntBetween(3, 20)));
        createIndexWithContent("test-index-2");

        final int inProgressCount = randomIntBetween(6, 20);
        final List<ActionFuture<CreateSnapshotResponse>> inProgressSnapshots = new ArrayList<>(inProgressCount);
        blockAllDataNodes(repoName);
        for (int i = 0; i < inProgressCount; i++) {
            final String snapshotName = "snap-" + i;
            allSnapshotNames.add(snapshotName);
            inProgressSnapshots.add(startFullSnapshot(repoName, snapshotName));
        }
        awaitNumberOfSnapshotsInProgress(inProgressCount);

        assertStablePagination(repoName, allSnapshotNames, GetSnapshotsRequest.SortBy.START_TIME);
        assertStablePagination(repoName, allSnapshotNames, GetSnapshotsRequest.SortBy.NAME);
        assertStablePagination(repoName, allSnapshotNames, GetSnapshotsRequest.SortBy.INDICES);

        unblockAllDataNodes(repoName);
        for (ActionFuture<CreateSnapshotResponse> inProgressSnapshot : inProgressSnapshots) {
            assertSuccessful(inProgressSnapshot);
        }

        assertStablePagination(repoName, allSnapshotNames, GetSnapshotsRequest.SortBy.START_TIME);
        assertStablePagination(repoName, allSnapshotNames, GetSnapshotsRequest.SortBy.NAME);
        assertStablePagination(repoName, allSnapshotNames, GetSnapshotsRequest.SortBy.INDICES);
    }

    public void testPaginationRequiresVerboseListing() throws Exception {
        final String repoName = "tst-repo";
        createRepository(repoName, "fs");
        createNSnapshots(repoName, randomIntBetween(1, 5));
        expectThrows(
            ActionRequestValidationException.class,
            () -> clusterAdmin().prepareGetSnapshots(repoName)
                .setVerbose(false)
                .setSort(GetSnapshotsRequest.SortBy.DURATION)
                .setSize(GetSnapshotsRequest.NO_LIMIT)
                .execute()
                .actionGet()
        );
        expectThrows(
            ActionRequestValidationException.class,
            () -> clusterAdmin().prepareGetSnapshots(repoName)
                .setVerbose(false)
                .setSort(GetSnapshotsRequest.SortBy.START_TIME)
                .setSize(randomIntBetween(1, 100))
                .execute()
                .actionGet()
        );
    }

    public void testFilterBySLMPolicy() throws Exception {
        final String repoName = "test-repo";
        createRepository(repoName, "fs");
        createNSnapshots(repoName, randomIntBetween(1, 5));
        final List<SnapshotInfo> snapshotsWithoutPolicy = clusterAdmin().prepareGetSnapshots("*")
            .setSnapshots("*")
            .setSort(GetSnapshotsRequest.SortBy.NAME)
            .get()
            .getSnapshots();
        final String snapshotWithPolicy = "snapshot-with-policy";
        final String policyName = "some-policy";
        final SnapshotInfo withPolicy = assertSuccessful(
            clusterAdmin().prepareCreateSnapshot(repoName, snapshotWithPolicy)
                .setUserMetadata(Map.of(SnapshotsService.POLICY_ID_METADATA_FIELD, policyName))
                .setWaitForCompletion(true)
                .execute()
        );

        assertThat(getAllSnapshotsForPolicies(policyName), is(List.of(withPolicy)));
        assertThat(getAllSnapshotsForPolicies("some-*"), is(List.of(withPolicy)));
        assertThat(getAllSnapshotsForPolicies("*", "-" + policyName), empty());
        assertThat(getAllSnapshotsForPolicies(GetSnapshotsRequest.NO_POLICY_PATTERN), is(snapshotsWithoutPolicy));
        assertThat(getAllSnapshotsForPolicies(GetSnapshotsRequest.NO_POLICY_PATTERN, "-" + policyName), is(snapshotsWithoutPolicy));
        assertThat(getAllSnapshotsForPolicies(GetSnapshotsRequest.NO_POLICY_PATTERN), is(snapshotsWithoutPolicy));
        assertThat(getAllSnapshotsForPolicies(GetSnapshotsRequest.NO_POLICY_PATTERN, "-*"), is(snapshotsWithoutPolicy));
        assertThat(getAllSnapshotsForPolicies("no-such-policy"), empty());
        assertThat(getAllSnapshotsForPolicies("no-such-policy*"), empty());

        final String snapshotWithOtherPolicy = "snapshot-with-other-policy";
        final String otherPolicyName = "other-policy";
        final SnapshotInfo withOtherPolicy = assertSuccessful(
            clusterAdmin().prepareCreateSnapshot(repoName, snapshotWithOtherPolicy)
                .setUserMetadata(Map.of(SnapshotsService.POLICY_ID_METADATA_FIELD, otherPolicyName))
                .setWaitForCompletion(true)
                .execute()
        );
        assertThat(getAllSnapshotsForPolicies("*"), is(List.of(withOtherPolicy, withPolicy)));
        assertThat(getAllSnapshotsForPolicies(policyName, otherPolicyName), is(List.of(withOtherPolicy, withPolicy)));
        assertThat(getAllSnapshotsForPolicies(policyName, otherPolicyName, "no-such-policy*"), is(List.of(withOtherPolicy, withPolicy)));

        final List<SnapshotInfo> allSnapshots = clusterAdmin().prepareGetSnapshots("*")
            .setSnapshots("*")
            .setSort(GetSnapshotsRequest.SortBy.NAME)
            .get()
            .getSnapshots();
        assertThat(getAllSnapshotsForPolicies(GetSnapshotsRequest.NO_POLICY_PATTERN, policyName, otherPolicyName), is(allSnapshots));
        assertThat(getAllSnapshotsForPolicies(GetSnapshotsRequest.NO_POLICY_PATTERN, "*"), is(allSnapshots));
    }

    private static List<SnapshotInfo> getAllSnapshotsForPolicies(String... policies) {
        return clusterAdmin().prepareGetSnapshots("*")
            .setSnapshots("*")
            .setPolicies(policies)
            .setSort(GetSnapshotsRequest.SortBy.NAME)
            .get()
            .getSnapshots();
    }

    private static void assertStablePagination(String repoName, Collection<String> allSnapshotNames, GetSnapshotsRequest.SortBy sort) {
        final SortOrder order = randomFrom(SortOrder.values());
        final List<SnapshotInfo> allSorted = allSnapshotsSorted(allSnapshotNames, repoName, sort, order);

        for (int i = 1; i <= allSnapshotNames.size(); i++) {
            final GetSnapshotsResponse subsetSorted = sortedWithLimit(repoName, sort, null, i, order);
            assertEquals(allSorted.subList(0, i), subsetSorted.getSnapshots());
        }

        for (int j = 0; j < allSnapshotNames.size(); j++) {
            final SnapshotInfo after = allSorted.get(j);
            for (int i = 1; i < allSnapshotNames.size() - j; i++) {
                final GetSnapshotsResponse getSnapshotsResponse = sortedWithLimit(
                    repoName,
                    sort,
                    GetSnapshotsRequest.After.from(after, sort).asQueryParam(),
                    i,
                    order
                );
                final GetSnapshotsResponse getSnapshotsResponseNumeric = sortedWithLimit(repoName, sort, j + 1, i, order);
                final List<SnapshotInfo> subsetSorted = getSnapshotsResponse.getSnapshots();
                assertEquals(subsetSorted, getSnapshotsResponseNumeric.getSnapshots());
                assertEquals(subsetSorted, allSorted.subList(j + 1, j + i + 1));
                assertEquals(allSnapshotNames.size(), getSnapshotsResponse.totalCount());
                assertEquals(allSnapshotNames.size() - (j + i + 1), getSnapshotsResponse.remaining());
                assertEquals(subsetSorted, allSorted.subList(j + 1, j + i + 1));
                assertEquals(getSnapshotsResponseNumeric.totalCount(), getSnapshotsResponse.totalCount());
                assertEquals(getSnapshotsResponseNumeric.remaining(), getSnapshotsResponse.remaining());
            }
        }
    }

    private static List<SnapshotInfo> allSnapshotsSorted(
        Collection<String> allSnapshotNames,
        String repoName,
        GetSnapshotsRequest.SortBy sortBy,
        SortOrder order
    ) {
        final GetSnapshotsResponse getSnapshotsResponse = sortedWithLimit(repoName, sortBy, null, GetSnapshotsRequest.NO_LIMIT, order);
        final List<SnapshotInfo> snapshotInfos = getSnapshotsResponse.getSnapshots();
        assertEquals(snapshotInfos.size(), allSnapshotNames.size());
        assertEquals(getSnapshotsResponse.totalCount(), allSnapshotNames.size());
        assertEquals(0, getSnapshotsResponse.remaining());
        for (SnapshotInfo snapshotInfo : snapshotInfos) {
            assertThat(snapshotInfo.snapshotId().getName(), is(in(allSnapshotNames)));
        }
        return snapshotInfos;
    }

    private static GetSnapshotsResponse sortedWithLimit(
        String repoName,
        GetSnapshotsRequest.SortBy sortBy,
        String after,
        int size,
        SortOrder order
    ) {
        return baseGetSnapshotsRequest(repoName).setAfter(after).setSort(sortBy).setSize(size).setOrder(order).get();
    }

    private static GetSnapshotsResponse sortedWithLimit(
        String repoName,
        GetSnapshotsRequest.SortBy sortBy,
        int offset,
        int size,
        SortOrder order
    ) {
        return baseGetSnapshotsRequest(repoName).setOffset(offset).setSort(sortBy).setSize(size).setOrder(order).get();
    }

    private static GetSnapshotsRequestBuilder baseGetSnapshotsRequest(String repoName) {
        final GetSnapshotsRequestBuilder builder = clusterAdmin().prepareGetSnapshots(repoName);
        // exclude old version snapshot from test assertions every time and do a prefixed query in either case half the time
        if (randomBoolean()
            || clusterAdmin().prepareGetSnapshots(repoName)
                .setSnapshots(AbstractSnapshotIntegTestCase.OLD_VERSION_SNAPSHOT_PREFIX + "*")
                .setIgnoreUnavailable(true)
                .get()
                .getSnapshots()
                .isEmpty() == false) {
            builder.setSnapshots(RANDOM_SNAPSHOT_NAME_PREFIX + "*");
        }
        return builder;
    }
}
