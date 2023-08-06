/**
 * Tests the the donor correctly recovers the abort reason and the migration after stepup.
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
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {makeX509OptionsForTest} from "jstests/replsets/libs/tenant_migration_util.js";

// Set the delay before a state doc is garbage collected to be short to speed up the test.
const kGarbageCollectionParams = {
    tenantMigrationGarbageCollectionDelayMS: 3 * 1000,
    ttlMonitorSleepSecs: 1,
};

const donorRst = new ReplSetTest({
    nodes: 3,
    name: "donor",
    serverless: true,
    nodeOptions:
        Object.assign(makeX509OptionsForTest().donor, {setParameter: kGarbageCollectionParams})
});

donorRst.startSet();
donorRst.initiate();

const tenantMigrationTest = new TenantMigrationTest(
    {name: jsTestName(), donorRst, sharedOptions: {setParameter: kGarbageCollectionParams}});

const tenantId = ObjectId().str;
const migrationId = UUID();
const migrationOpts = {
    migrationIdString: extractUUIDFromObject(migrationId),
    tenantId: tenantId,
};

const donorPrimary = tenantMigrationTest.getDonorPrimary();

assert.commandWorked(donorPrimary.getCollection(tenantId + "_testDb.testColl").insert({_id: 0}));

const donorFp = configureFailPoint(donorPrimary, "abortTenantMigrationBeforeLeavingBlockingState");

TenantMigrationTest.assertAborted(
    tenantMigrationTest.runMigration(migrationOpts, {automaticForgetMigration: false}),
    ErrorCodes.InternalError);
donorFp.off();

assert.commandWorked(
    donorPrimary.adminCommand({replSetStepDown: ReplSetTest.kForeverSecs, force: true}));
assert.commandWorked(donorPrimary.adminCommand({replSetFreeze: 0}));

TenantMigrationTest.assertAborted(
    tenantMigrationTest.runMigration(migrationOpts, {automaticForgetMigration: false}),
    ErrorCodes.InternalError);

assert.commandWorked(tenantMigrationTest.forgetMigration(migrationOpts.migrationIdString));
tenantMigrationTest.waitForMigrationGarbageCollection(migrationId, tenantId);

donorRst.stopSet();
tenantMigrationTest.stop();
