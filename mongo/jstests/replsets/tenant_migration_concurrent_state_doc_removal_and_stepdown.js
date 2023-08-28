/**
 * Tests that donorForgetMigration command doesn't hang if failover occurs immediately after the
 * state doc for the migration has been removed.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   serverless,
 *   requires_fcv_71,
 * ]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {Thread} from "jstests/libs/parallelTester.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {forgetMigrationAsync} from "jstests/replsets/libs/tenant_migration_util.js";
import {createRstArgs} from "jstests/replsets/rslib.js";

const tenantMigrationTest = new TenantMigrationTest(
    {name: jsTestName(), quickGarbageCollection: true, initiateRstWithHighElectionTimeout: false});

const kTenantId = ObjectId().str;

const donorRst = tenantMigrationTest.getDonorRst();
const donorRstArgs = createRstArgs(donorRst);
let donorPrimary = tenantMigrationTest.getDonorPrimary();

const migrationId = UUID();
const migrationOpts = {
    migrationIdString: extractUUIDFromObject(migrationId),
    tenantId: kTenantId,
};

TenantMigrationTest.assertCommitted(
    tenantMigrationTest.runMigration(migrationOpts, {automaticForgetMigration: false}));

let fp = configureFailPoint(donorPrimary,
                            "pauseTenantMigrationDonorAfterMarkingStateGarbageCollectable");
const forgetMigrationThread = new Thread(forgetMigrationAsync,
                                         migrationOpts.migrationIdString,
                                         donorRstArgs,
                                         false /* retryOnRetryableErrors */);
forgetMigrationThread.start();
fp.wait();
tenantMigrationTest.waitForMigrationGarbageCollection(migrationId, migrationOpts.tenantId);

assert.commandWorked(
    donorPrimary.adminCommand({replSetStepDown: ReplSetTest.kForeverSecs, force: true}));
assert.commandWorked(donorPrimary.adminCommand({replSetFreeze: 0}));
fp.off();
donorPrimary = donorRst.getPrimary();

assert.commandFailedWithCode(forgetMigrationThread.returnData(),
                             ErrorCodes.InterruptedDueToReplStateChange);

donorRst.stopSet();
tenantMigrationTest.stop();
