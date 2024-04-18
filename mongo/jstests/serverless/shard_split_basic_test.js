/**
 * Tests that runs a shard split to completion.
 * @tags: [requires_fcv_63, serverless]
 */

import {
    assertMigrationState,
    findSplitOperation,
    ShardSplitTest
} from "jstests/serverless/libs/shard_split_test.js";

function runBasicShardSplitTest() {
    jsTestLog("Starting to run shard split ");
    const tenantIds = [ObjectId(), ObjectId()];
    const testOptions = {quickGarbageCollection: true};
    const test = new ShardSplitTest(testOptions);
    test.addRecipientNodes();
    test.donor.awaitSecondaryNodes();

    const donorPrimary = test.getDonorPrimary();
    const operation = test.createSplitOperation(tenantIds);
    const result = assert.commandWorked(operation.commit());
    assertMigrationState(donorPrimary, operation.migrationId, "committed");

    // Confirm blockOpTime in result matches the state document before forgetting the operation
    const stateDoc = findSplitOperation(donorPrimary, operation.migrationId);
    assert.eq(stateDoc.blockOpTime.ts, result.blockOpTime.ts);

    operation.forget();

    const status = donorPrimary.adminCommand({serverStatus: 1});
    assert.eq(status.shardSplits.totalCommitted, 1);
    assert.eq(status.shardSplits.totalAborted, 0);
    assert.gt(status.shardSplits.totalCommittedDurationMillis, 0);
    assert.gt(status.shardSplits.totalCommittedDurationWithoutCatchupMillis, 0);

    test.cleanupSuccesfulCommitted(operation.migrationId, tenantIds);
    test.stop();
}

runBasicShardSplitTest();
