/**
 * Starts a tenant migration that aborts, either due to the
 * abortTenantMigrationBeforeLeavingBlockingState failpoint or due to receiving donorAbortMigration,
 * and then issues a donorForgetMigration command. Finally, starts a second tenant migration with
 * the same tenantId as the aborted migration, and expects this second migration to go through.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   serverless,
 * ]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {Thread} from "jstests/libs/parallelTester.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {tryAbortMigrationAsync} from "jstests/replsets/libs/tenant_migration_util.js";
import {createRstArgs} from "jstests/replsets/rslib.js";

function makeTenantId() {
    return ObjectId().str;
}

const tenantMigrationTest =
    new TenantMigrationTest({name: jsTestName(), quickGarbageCollection: true});

(() => {
    const migrationId1 = extractUUIDFromObject(UUID());
    const migrationId2 = extractUUIDFromObject(UUID());
    const tenantId = makeTenantId();

    // Start a migration with the "abortTenantMigrationBeforeLeavingBlockingState" failPoint
    // enabled. The migration will abort as a result, and a status of "kAborted" should be returned.
    jsTestLog(
        "Starting a migration that is expected to abort due to setting abortTenantMigrationBeforeLeavingBlockingState failpoint. migrationId: " +
        migrationId1 + ", tenantId: " + tenantId);
    const donorPrimary = tenantMigrationTest.getDonorPrimary();
    const abortFp =
        configureFailPoint(donorPrimary, "abortTenantMigrationBeforeLeavingBlockingState");
    TenantMigrationTest.assertAborted(tenantMigrationTest.runMigration(
        {migrationIdString: migrationId1, tenantId: tenantId}, {automaticForgetMigration: false}));
    abortFp.off();

    // Forget the aborted migration.
    jsTestLog("Forgetting aborted migration with migrationId: " + migrationId1);
    assert.commandWorked(tenantMigrationTest.forgetMigration(migrationId1));

    // Try running a new migration with the same tenantId. It should succeed, since the previous
    // migration with the same tenantId was aborted.
    jsTestLog("Attempting to run a new migration with the same tenantId. New migrationId: " +
              migrationId2 + ", tenantId: " + tenantId);
    TenantMigrationTest.assertCommitted(
        tenantMigrationTest.runMigration({migrationIdString: migrationId2, tenantId: tenantId}));
    tenantMigrationTest.waitForMigrationGarbageCollection(migrationId2, tenantId);
})();

(() => {
    const migrationId1 = extractUUIDFromObject(UUID());
    const migrationId2 = extractUUIDFromObject(UUID());
    const tenantId = makeTenantId();

    jsTestLog(
        "Starting a migration that is expected to abort in blocking state due to receiving donorAbortMigration. migrationId: " +
        migrationId1 + ", tenantId: " + tenantId);

    const donorPrimary = tenantMigrationTest.getDonorPrimary();
    let fp = configureFailPoint(donorPrimary, "pauseTenantMigrationBeforeLeavingBlockingState");
    assert.commandWorked(
        tenantMigrationTest.startMigration({migrationIdString: migrationId1, tenantId: tenantId}));

    fp.wait();

    const donorRstArgs = createRstArgs(tenantMigrationTest.getDonorRst());
    const tryAbortThread = new Thread(tryAbortMigrationAsync,
                                      {migrationIdString: migrationId1, tenantId: tenantId},
                                      donorRstArgs,
                                      true /* retryOnRetryableErrors */);
    tryAbortThread.start();

    // Wait for donorAbortMigration command to start.
    assert.soon(() => {
        const res = assert.commandWorked(
            donorPrimary.adminCommand({currentOp: true, desc: "tenant donor migration"}));
        const op = res.inprog.find(op => extractUUIDFromObject(op.instanceID) === migrationId1);
        return op.receivedCancellation;
    });

    fp.off();

    tryAbortThread.join();
    assert.commandWorked(tryAbortThread.returnData());

    TenantMigrationTest.assertAborted(tenantMigrationTest.waitForMigrationToComplete(
        {migrationIdString: migrationId1, tenantId: tenantId}));

    // Forget the aborted migration.
    jsTestLog("Forgetting aborted migration with migrationId: " + migrationId1);
    assert.commandWorked(tenantMigrationTest.forgetMigration(migrationId1));

    // Try running a new migration with the same tenantId. It should succeed, since the previous
    // migration with the same tenantId was aborted.
    jsTestLog("Attempting to run a new migration with the same tenantId. New migrationId: " +
              migrationId2 + ", tenantId: " + tenantId);
    TenantMigrationTest.assertCommitted(
        tenantMigrationTest.runMigration({migrationIdString: migrationId2, tenantId: tenantId}));
    tenantMigrationTest.waitForMigrationGarbageCollection(migrationId2, tenantId);
})();

tenantMigrationTest.stop();
