/**
 * Tests the recipientVoteImportedFiles command.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   serverless,
 *   featureFlagShardMerge,
 * ]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {isShardMergeEnabled} from "jstests/replsets/libs/tenant_migration_util.js";
import {createRstArgs} from "jstests/replsets/rslib.js";

const tenantMigrationTest = new TenantMigrationTest(
    {name: jsTestName(), sharedOptions: {nodes: 1}, quickGarbageCollection: true});

const recipientPrimary = tenantMigrationTest.getRecipientPrimary();
const kValidFromHostName = recipientPrimary.host;
const kInvalidFromHostName = "dummy:27017";
const kTenantId = ObjectId();
const migrationId = UUID();
const migrationOpts = {
    migrationIdString: extractUUIDFromObject(migrationId),
    recipientConnString: tenantMigrationTest.getRecipientConnString(),
    tenantIds: [kTenantId]
};

// Note: including this explicit early return here due to the fact that multiversion
// suites will execute this test without featureFlagShardMerge enabled (despite the
// presence of the featureFlagShardMerge tag above), which means the test will attempt
// to run a multi-tenant migration and fail.
if (!isShardMergeEnabled(recipientPrimary.getDB("admin"))) {
    tenantMigrationTest.stop();
    jsTestLog("Skipping Shard Merge-specific test");
    quit();
}

function runVoteCmd(migrationId, fromHostName) {
    // Pretend the primary tells itself it has imported files. This may preempt the primary's real
    // life message, but that's ok. We use a failpoint to prevent migration from progressing too
    // far.
    return recipientPrimary.adminCommand({
        recipientVoteImportedFiles: 1,
        migrationId: migrationId,
        from: fromHostName,
    });
}

function voteShouldFail(migrationId, fromHostName) {
    const reply = runVoteCmd(migrationId, fromHostName);
    jsTestLog(`Vote with migrationId ${migrationId} from ${fromHostName}, reply` +
              ` (should fail): ${tojson(reply)}`);
    assert.commandFailedWithCode(reply, ErrorCodes.NoSuchTenantMigration);
}

function voteShouldSucceed(migrationId, fromHostName) {
    assert.commandWorked(runVoteCmd(migrationId, fromHostName));
}

jsTestLog("Test recipientVoteImportedFiles with no migration started");
voteShouldFail(migrationId, kValidFromHostName);

const fpHangBeforeVoteImportedFiles =
    configureFailPoint(recipientPrimary, "hangBeforeVoteImportedFiles");

assert.commandWorked(tenantMigrationTest.startMigration(migrationOpts));
fpHangBeforeVoteImportedFiles.wait();

jsTestLog("Test recipientVoteImportedFiles with wrong migrationId during migration");
voteShouldFail(UUID(), kValidFromHostName);

// Import quorum will be satisfied only after receiving votes from all voting data-bearing
// nodes that are part of  current replica set config.
jsTestLog("Test recipientVoteImportedFiles with voter not part of current config during migration");
voteShouldSucceed(migrationId, kInvalidFromHostName);
let currOpRes = recipientPrimary.adminCommand({currentOp: true, desc: "shard merge recipient"});
assert.eq(currOpRes.inprog.length, 1, currOpRes);
assert.eq(currOpRes.inprog[0].importQuorumSatisfied, false, currOpRes);

jsTestLog("Test recipientVoteImportedFiles with voter part of current config during migration");
voteShouldSucceed(migrationId, kValidFromHostName);
currOpRes = recipientPrimary.adminCommand({currentOp: true, desc: "shard merge recipient"});
assert.eq(currOpRes.inprog.length, 1, currOpRes);
assert.eq(currOpRes.inprog[0].importQuorumSatisfied, true, currOpRes);

fpHangBeforeVoteImportedFiles.off();

// Just a delayed message, the primary replies "ok".
jsTestLog("Test recipientVoteImportedFiles after import quorum satisfied");
voteShouldSucceed(migrationId, kValidFromHostName);

TenantMigrationTest.assertCommitted(tenantMigrationTest.waitForMigrationToComplete(
    migrationOpts, false /* retryOnRetryableErrors */, true /* forgetMigration */));
tenantMigrationTest.waitForMigrationGarbageCollection(migrationId, kTenantId.str);

jsTestLog("Test recipientVoteImportedFiles after migration forgotten");
voteShouldFail(migrationId, kValidFromHostName);

tenantMigrationTest.stop();