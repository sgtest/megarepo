/**
 * Tests that the donor
 * - does not rejects reads with atClusterTime/afterClusterTime >= blockOpTime reads and
 * linearizable reads after the split aborts.
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
import {
    assertMigrationState,
    findSplitOperation,
    ShardSplitTest
} from "jstests/serverless/libs/shard_split_test.js";
import {
    runCommandForConcurrentReadTest,
    shardSplitConcurrentReadTestCases
} from "jstests/serverless/shard_split_concurrent_reads_on_donor_util.js";

const kCollName = "testColl";

/**
 * Tests that after the split abort, the donor does not reject linearizable reads or reads with
 * atClusterTime/afterClusterTime >= blockOpTime.
 */
function testDoNotRejectReadsAfterMigrationAborted(testCase, dbName, collName) {
    const tenantId = dbName.split('_')[0];
    const donorDoc = findSplitOperation(donorPrimary, operation.migrationId);
    const nodes = testCase.isSupportedOnSecondaries ? donorRst.nodes : [donorPrimary];
    nodes.forEach(node => {
        const db = node.getDB(dbName);
        if (testCase.requiresReadTimestamp) {
            runCommandForConcurrentReadTest(db,
                                            testCase.command(collName, donorDoc.blockOpTime.ts),
                                            null,
                                            testCase.isTransaction);
            runCommandForConcurrentReadTest(
                db,
                testCase.command(collName, donorDoc.commitOrAbortOpTime.ts),
                null,
                testCase.isTransaction);
            ShardSplitTest.checkShardSplitAccessBlocker(
                node, tenantId, {numTenantMigrationAbortedErrors: 0});
        } else {
            runCommandForConcurrentReadTest(
                db, testCase.command(collName), null, testCase.isTransaction);
            ShardSplitTest.checkShardSplitAccessBlocker(
                node, tenantId, {numTenantMigrationAbortedErrors: 0});
        }
    });
}

const testCases = shardSplitConcurrentReadTestCases;

const test = new ShardSplitTest({
    recipientTagName: "recipientTag",
    recipientSetName: "recipientSet",
    quickGarbageCollection: true
});
test.addRecipientNodes();

const ktenantId = ObjectId();
const tenantIds = [ktenantId];

const donorRst = test.donor;
const donorPrimary = test.getDonorPrimary();

// Force the donor to preserve all snapshot history to ensure that transactional reads do not
// fail with TransientTransactionError "Read timestamp is older than the oldest available
// timestamp".
donorRst.nodes.forEach(node => {
    configureFailPoint(node, "WTPreserveSnapshotHistoryIndefinitely");
});

let blockFp = configureFailPoint(donorPrimary, "pauseShardSplitAfterBlocking");

const operation = test.createSplitOperation(tenantIds);
const splitThread = operation.commitAsync();

blockFp.wait();
operation.abort();

blockFp.off();

splitThread.join();
assert.commandFailed(splitThread.returnData());
assertMigrationState(donorPrimary, operation.migrationId, "aborted");

// Wait for the last oplog entry on the primary to be visible in the committed snapshot view of
// the oplog on all the secondaries. This is to ensure that snapshot reads on secondaries with
// unspecified atClusterTime have read timestamp >= abortTimestamp.
donorRst.awaitLastOpCommitted();

for (const [testCaseName, testCase] of Object.entries(testCases)) {
    jsTest.log(`Testing inAborted with testCase ${testCaseName}`);
    const dbName = `${ktenantId.str}_${testCaseName}`;
    testDoNotRejectReadsAfterMigrationAborted(testCase, dbName, kCollName);
}

test.stop();
