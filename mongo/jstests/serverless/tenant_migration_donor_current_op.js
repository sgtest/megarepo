/**
 * Tests currentOp command during a tenant migration.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   # The currentOp output field 'lastDurableState' was changed from an enum to a string.
 *   requires_fcv_70,
 *   serverless,
 *   requires_fcv_71,
 * ]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {isShardMergeEnabled} from "jstests/replsets/libs/tenant_migration_util.js";

const kTenantId = ObjectId().str;
const kReadPreference = {
    mode: "primary"
};

function checkStandardFieldsOK(ops, {
    migrationId,
    lastDurableState,
    tenantMigrationTest,
    garbageCollectable = false,
}) {
    assert.eq(ops.length, 1);
    const [op] = ops;
    assert.eq(bsonWoCompare(op.instanceID, migrationId), 0);
    assert.eq(bsonWoCompare(op.readPreference, kReadPreference), 0);
    assert.eq(op.lastDurableState, lastDurableState);
    assert.eq(op.garbageCollectable, garbageCollectable);
    assert(op.migrationStart instanceof Date);
    assert.eq(op.recipientConnectionString, tenantMigrationTest.getRecipientRst().getURL());

    if (isShardMergeEnabled(tenantMigrationTest.getDonorPrimary().getDB("admin"))) {
        assert.eq(op.tenantId, undefined);
        assert(bsonBinaryEqual(op.tenantIds, [ObjectId(kTenantId)]), op);
    } else {
        assert.eq(bsonWoCompare(op.tenantId, kTenantId), 0);
        assert.eq(op.tenantIds, undefined);
    }
}

(() => {
    jsTestLog("Testing currentOp output for migration in data sync state");
    const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

    const donorPrimary = tenantMigrationTest.getDonorPrimary();

    const migrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: kTenantId,
        readPreference: kReadPreference
    };
    let fp = configureFailPoint(donorPrimary,
                                "pauseTenantMigrationBeforeLeavingAbortingIndexBuildsState");
    assert.commandWorked(tenantMigrationTest.startMigration(migrationOpts));
    fp.wait();

    const res = assert.commandWorked(
        donorPrimary.adminCommand({currentOp: true, desc: "tenant donor migration"}));

    checkStandardFieldsOK(res.inprog, {
        migrationId,
        lastDurableState: TenantMigrationTest.DonorState.kAbortingIndexBuilds,
        tenantMigrationTest,
    });

    fp.off();
    TenantMigrationTest.assertCommitted(
        tenantMigrationTest.waitForMigrationToComplete(migrationOpts));
    tenantMigrationTest.stop();
})();

(() => {
    jsTestLog("Testing currentOp output for migration in data sync state");
    const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

    const donorPrimary = tenantMigrationTest.getDonorPrimary();

    const migrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: kTenantId,
        readPreference: kReadPreference
    };
    let fp = configureFailPoint(donorPrimary, "pauseTenantMigrationBeforeLeavingDataSyncState");
    assert.commandWorked(tenantMigrationTest.startMigration(migrationOpts));
    fp.wait();

    const res = assert.commandWorked(
        donorPrimary.adminCommand({currentOp: true, desc: "tenant donor migration"}));

    checkStandardFieldsOK(res.inprog, {
        migrationId,
        lastDurableState: TenantMigrationTest.DonorState.kDataSync,
        tenantMigrationTest,
    });
    assert(res.inprog[0].startMigrationDonorTimestamp instanceof Timestamp);

    fp.off();
    TenantMigrationTest.assertCommitted(
        tenantMigrationTest.waitForMigrationToComplete(migrationOpts));
    tenantMigrationTest.stop();
})();

(() => {
    jsTestLog("Testing currentOp output for migration in blocking state");
    const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

    const donorPrimary = tenantMigrationTest.getDonorPrimary();

    const migrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: kTenantId,
        readPreference: kReadPreference
    };
    let fp = configureFailPoint(donorPrimary, "pauseTenantMigrationBeforeLeavingBlockingState");
    assert.commandWorked(tenantMigrationTest.startMigration(migrationOpts));
    fp.wait();

    const res = assert.commandWorked(
        donorPrimary.adminCommand({currentOp: true, desc: "tenant donor migration"}));

    checkStandardFieldsOK(res.inprog, {
        migrationId,
        lastDurableState: TenantMigrationTest.DonorState.kBlocking,
        tenantMigrationTest,
    });
    assert(res.inprog[0].blockTimestamp instanceof Timestamp);

    fp.off();
    TenantMigrationTest.assertCommitted(
        tenantMigrationTest.waitForMigrationToComplete(migrationOpts));
    tenantMigrationTest.stop();
})();

(() => {
    jsTestLog("Testing currentOp output for aborted migration");
    const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

    const donorPrimary = tenantMigrationTest.getDonorPrimary();

    const migrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: kTenantId,
        readPreference: kReadPreference
    };
    configureFailPoint(donorPrimary, "abortTenantMigrationBeforeLeavingBlockingState");
    TenantMigrationTest.assertAborted(
        tenantMigrationTest.runMigration(migrationOpts, {automaticForgetMigration: false}));

    const res = assert.commandWorked(
        donorPrimary.adminCommand({currentOp: true, desc: "tenant donor migration"}));

    checkStandardFieldsOK(res.inprog, {
        migrationId,
        lastDurableState: TenantMigrationTest.DonorState.kAborted,
        tenantMigrationTest,
    });
    assert(res.inprog[0].startMigrationDonorTimestamp instanceof Timestamp);
    assert(res.inprog[0].blockTimestamp instanceof Timestamp);
    assert(res.inprog[0].commitOrAbortOpTime.ts instanceof Timestamp);
    assert(res.inprog[0].commitOrAbortOpTime.t instanceof NumberLong);
    assert.eq(typeof res.inprog[0].abortReason.code, "number");
    assert.eq(typeof res.inprog[0].abortReason.codeName, "string");
    assert.eq(typeof res.inprog[0].abortReason.errmsg, "string");

    tenantMigrationTest.stop();
})();

// Check currentOp while in committed state before and after a migration has completed.
(() => {
    jsTestLog("Testing currentOp output for committed migration");
    const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

    const donorPrimary = tenantMigrationTest.getDonorPrimary();

    const migrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: kTenantId,
        readPreference: kReadPreference
    };
    assert.commandWorked(
        tenantMigrationTest.runMigration(migrationOpts, {automaticForgetMigration: false}));

    let res = donorPrimary.adminCommand({currentOp: true, desc: "tenant donor migration"});

    checkStandardFieldsOK(res.inprog, {
        migrationId,
        lastDurableState: TenantMigrationTest.DonorState.kCommitted,
        tenantMigrationTest,
    });
    assert(res.inprog[0].startMigrationDonorTimestamp instanceof Timestamp);
    assert(res.inprog[0].blockTimestamp instanceof Timestamp);
    assert(res.inprog[0].commitOrAbortOpTime.ts instanceof Timestamp);
    assert(res.inprog[0].commitOrAbortOpTime.t instanceof NumberLong);

    jsTestLog("Testing currentOp output for a committed migration after donorForgetMigration");

    assert.commandWorked(tenantMigrationTest.forgetMigration(migrationOpts.migrationIdString));

    res = donorPrimary.adminCommand({currentOp: true, desc: "tenant donor migration"});

    checkStandardFieldsOK(res.inprog, {
        migrationId,
        lastDurableState: TenantMigrationTest.DonorState.kCommitted,
        tenantMigrationTest,
        garbageCollectable: true,
    });
    assert(res.inprog[0].startMigrationDonorTimestamp instanceof Timestamp);
    assert(res.inprog[0].blockTimestamp instanceof Timestamp);
    assert(res.inprog[0].commitOrAbortOpTime.ts instanceof Timestamp);
    assert(res.inprog[0].commitOrAbortOpTime.t instanceof NumberLong);
    assert(res.inprog[0].expireAt instanceof Date);

    tenantMigrationTest.stop();
})();
