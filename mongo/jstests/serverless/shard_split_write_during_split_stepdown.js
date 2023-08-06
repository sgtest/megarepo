/**
 *
 * Tests that runs a shard split, a stepdown and writes operation simultaneously to verify the
 * commands return the expected errors and success.
 * result of write operations.
 * @tags: [requires_fcv_63, serverless]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {Thread} from "jstests/libs/parallelTester.js";
import {createRstArgs} from "jstests/replsets/rslib.js";
import {doWriteOperations, ShardSplitTest} from "jstests/serverless/libs/shard_split_test.js";

const test = new ShardSplitTest({
    nodeOptions: {
        // Set a short timeout to test that the operation times out waiting for replication
        setParameter: "shardSplitTimeoutMS=100000"
    }
});

test.addAndAwaitRecipientNodes();

const donorPrimary = test.donor.getPrimary();
const tenantIds = [ObjectId(), ObjectId()];

jsTestLog("Writing data before split");
tenantIds.forEach(id => {
    const kDbName = test.tenantDB(id.str, "testDb");
    const kCollName = "testColl";
    const kNs = `${kDbName}.${kCollName}`;

    assert.commandWorked(donorPrimary.getCollection(kNs).insert(
        [{_id: 0, x: 0}, {_id: 1, x: 1}, {_id: 2, x: 2}], {writeConcern: {w: "majority"}}));
});

const blockingFP = configureFailPoint(donorPrimary.getDB("admin"), "pauseShardSplitAfterBlocking");
const operation = test.createSplitOperation(tenantIds);
const splitThread = operation.commitAsync();
blockingFP.wait();

const donorRst = createRstArgs(test.donor);
test.removeRecipientsFromRstArgs(donorRst);
const writeThread = new Thread(doWriteOperations, donorRst, tojson(tenantIds));
writeThread.start();

assert.commandWorked(donorPrimary.adminCommand({replSetStepDown: 360, force: true}));

blockingFP.off();

assert.commandFailedWithCode(splitThread.returnData(), ErrorCodes.InterruptedDueToReplStateChange);

const writeResults = writeThread.returnData();
writeResults.forEach(res => {
    jsTestLog(`result: ${res}`);
    assert.eq(res, ErrorCodes.TenantMigrationCommitted);
});

TestData.skipCheckDBHashes = true;

test.stop();
