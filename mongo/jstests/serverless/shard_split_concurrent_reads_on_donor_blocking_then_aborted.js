/**
 * Tests that the donor
 * - blocks reads with atClusterTime/afterClusterTime >= blockOpTime that are executed while the
 *   split is in the blocking state but does not block linearizable reads.
 * - does not reject reads with atClusterTime/afterClusterTime >= blockOpTime and linearizable
 *   reads after the split aborts.
 *
 * @tags: [
 *   incompatible_with_eft,
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   serverless,
 *   requires_fcv_63
 * ]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {Thread} from "jstests/libs/parallelTester.js";
import {findSplitOperation, ShardSplitTest} from "jstests/serverless/libs/shard_split_test.js";
import {
    runCommandForConcurrentReadTest,
    shardSplitConcurrentReadTestCases
} from "jstests/serverless/shard_split_concurrent_reads_on_donor_util.js";

const ktenantId = ObjectId();
const tenantIds = [ktenantId];
const kCollName = "testColl";

/**
 * To be used to resume a split that is paused after entering the blocking state. Waits for the
 * number of blocked reads to reach 'targetNumBlockedReads' and unpauses the split.
 */
async function resumeMigrationAfterBlockingRead(host, tenantId, targetNumBlockedReads) {
    const {ShardSplitTest} = await import("jstests/serverless/libs/shard_split_test.js");

    const primary = new Mongo(host);
    assert.soon(() => ShardSplitTest.getNumBlockedReads(primary, eval(tenantId)) ==
                    targetNumBlockedReads);

    assert.commandWorked(
        primary.adminCommand({configureFailPoint: "pauseShardSplitAfterBlocking", mode: "off"}));
}

/**
 * Tests that the donor unblocks blocked reads (reads with atClusterTime/afterClusterTime >=
 * blockingTimestamp) once the split aborts.
 */
function testUnblockBlockedReadsAfterMigrationAborted(testCase, dbName, collName) {
    if (testCase.isLinearizableRead) {
        // Linearizable reads are not blocked.
        return;
    }

    const test = new ShardSplitTest({
        recipientTagName: "recipientTag",
        recipientSetName: "recipientSet",
        quickGarbageCollection: true
    });
    test.addRecipientNodes();

    const donorRst = test.donor;
    const donorPrimary = test.getDonorPrimary();

    let blockingFp = configureFailPoint(donorPrimary, "pauseShardSplitAfterBlocking");
    let abortFp = configureFailPoint(donorPrimary, "abortShardSplitBeforeLeavingBlockingState");
    const operation = test.createSplitOperation(tenantIds);

    // Run the commands after the split enters the blocking state.
    const splitThread = operation.commitAsync();

    let resumeMigrationThread =
        new Thread(resumeMigrationAfterBlockingRead, donorPrimary.host, tojson(ktenantId), 1);

    // Run the commands after the split enters the blocking state.
    resumeMigrationThread.start();
    blockingFp.wait();

    // Wait for the last oplog entry on the primary to be visible in the committed snapshot view of
    // the oplog on all secondaries to ensure that snapshot reads on the secondaries with
    // unspecified atClusterTime have read timestamp >= blockOpTime.
    donorRst.awaitLastOpCommitted();

    const donorDoc = findSplitOperation(donorPrimary, operation.migrationId);
    const command = testCase.requiresReadTimestamp
        ? testCase.command(collName, donorDoc.blockOpTime.ts)
        : testCase.command(collName);

    // The split should unpause and abort after the read is blocked. Verify that the read
    // unblocks.
    const db = donorPrimary.getDB(dbName);
    runCommandForConcurrentReadTest(db, command, null, testCase.isTransaction);
    if (testCase.isSupportedOnSecondaries) {
        const secondaries =
            test.getDonorNodes().filter(node => node.adminCommand({hello: 1}).secondary);
        secondaries.forEach(node => {
            const db = node.getDB(dbName);
            runCommandForConcurrentReadTest(db, command, null, testCase.isTransaction);
        });
    }

    const shouldBlock = !testCase.isLinearizableRead;
    ShardSplitTest.checkShardSplitAccessBlocker(donorPrimary, ktenantId, {
        numBlockedReads: shouldBlock ? 1 : 0,
        // Reads just get unblocked if the split aborts.
        numTenantMigrationAbortedErrors: 0
    });

    jsTestLog("Joining");
    splitThread.join();
    assert.commandFailed(splitThread.returnData());

    resumeMigrationThread.join();
    abortFp.off();
    test.stop();
}

const testCases = shardSplitConcurrentReadTestCases;

for (const [testCaseName, testCase] of Object.entries(testCases)) {
    jsTest.log(`Testing inBlockingThenAborted with testCase ${testCaseName}`);
    const dbName = `${ktenantId.str}_${testCaseName}`;
    testUnblockBlockedReadsAfterMigrationAborted(testCase, dbName, kCollName);
}
