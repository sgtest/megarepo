/**
 * Tests that the donor will retry its steps if its OperationContext is interrupted by a killOp.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   multiversion_incompatible,
 *   serverless,
 * ]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {Thread} from "jstests/libs/parallelTester.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {
    createTenantMigrationDonorRoleIfNotExist,
    createTenantMigrationRecipientRoleIfNotExist,
    forgetMigrationAsync,
    makeX509OptionsForTest,
    runMigrationAsync
} from "jstests/replsets/libs/tenant_migration_util.js";
import {createRstArgs} from "jstests/replsets/rslib.js";

const kGarbageCollectionDelayMS = 5 * 1000;

const migrationX509Options = makeX509OptionsForTest();
const garbageCollectionOpts = {
    // Set the delay before a donor state doc is garbage collected to be short to speed
    // up the test.
    tenantMigrationGarbageCollectionDelayMS: kGarbageCollectionDelayMS,
    ttlMonitorSleepSecs: 1
};

function makeTenantId() {
    return ObjectId().str;
}

{
    // This section tests behavior in the middle of a tenant migration.
    let fpNames = [
        "pauseTenantMigrationBeforeInsertingDonorStateDoc",
        "pauseTenantMigrationDonorWhileUpdatingStateDoc",
        "pauseTenantMigrationDonorBeforeStoringExternalClusterTimeKeyDocs"
    ];
    for (let fpName of fpNames) {
        jsTestLog("Setting failpoint \"" + fpName +
                  "\" to test that the migration will retry the " +
                  "operation at the failpoint if a killOp is issued.");

        const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

        const migrationOpts = {
            migrationIdString: extractUUIDFromObject(UUID()),
            recipientConnString: tenantMigrationTest.getRecipientConnString(),
            tenantId: makeTenantId(),

        };
        const donorPrimary = tenantMigrationTest.getDonorPrimary();
        let fp = configureFailPoint(donorPrimary, fpName);

        const donorRstArgs = createRstArgs(tenantMigrationTest.getDonorRst());

        const runMigrationThread = new Thread(runMigrationAsync, migrationOpts, donorRstArgs);
        runMigrationThread.start();
        fp.wait();

        const res = assert.commandWorked(donorPrimary.adminCommand({
            currentOp: true,
            $all: true,
            desc: {$regex: 'TenantMigrationDonorService'},
            opid: {$exists: true}
        }));

        const opid = res.inprog[0].opid;
        assert.commandWorked(donorPrimary.adminCommand({killOp: 1, op: opid}));

        fp.off();
        runMigrationThread.join();

        TenantMigrationTest.assertCommitted(runMigrationThread.returnData());
        assert.commandWorked(tenantMigrationTest.forgetMigration(migrationOpts.migrationIdString));
        tenantMigrationTest.stop();
    }
}

{
    // This section tests the behavior during TenantMigrationDonorService creation.
    let fpNames = [
        "pauseTenantMigrationBeforeCreatingStateDocumentTTLIndex",
        "pauseTenantMigrationBeforeCreatingExternalKeysTTLIndex"
    ];
    for (let fpName of fpNames) {
        const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});
        tenantMigrationTest.getDonorRst().stopSet();
        tenantMigrationTest.getDonorRst().startSet(Object.assign({}, migrationX509Options.donor, {
            setParameter: {['failpoint.' + fpName]: tojson({mode: 'alwaysOn'})}
        }));
        // The failpoints in this test run hang the TenantMigrationDonorService during service
        // rebuild, so we need to skip waiting on PrimaryOnlyServices.
        // This is also a problem when setting the server featureCompatibilityVersion, as this
        // waits for a rebuild, which is why this test is tagged as 'multiversion_incompatible'.
        // We initiate with any node as primary because we cannot safely step up node 0 if we
        // don't wait for primary only services.
        tenantMigrationTest.getDonorRst().initiateWithAnyNodeAsPrimary(
            null, null, {doNotWaitForPrimaryOnlyServices: true});
        createTenantMigrationRecipientRoleIfNotExist(tenantMigrationTest.getDonorRst());

        jsTestLog(
            "Setting failpoint \"" + fpName +
            "\" during the creation of a ReplSetTest to test that the migration will retry the " +
            "operation at the failpoint if a killOp is issued.");

        const migrationOpts = {
            migrationIdString: extractUUIDFromObject(UUID()),
            recipientConnString: tenantMigrationTest.getRecipientConnString(),
            tenantId: makeTenantId(),

        };
        const donorPrimary = tenantMigrationTest.getDonorPrimary();
        const donorRstArgs = createRstArgs(tenantMigrationTest.getDonorRst());
        const runMigrationThread = new Thread(runMigrationAsync, migrationOpts, donorRstArgs);
        runMigrationThread.start();

        const res = assert.commandWorked(donorPrimary.adminCommand({
            currentOp: true,
            $all: true,
            desc: {$regex: 'TenantMigrationDonorService'},
            opid: {$exists: true}
        }));
        const opid = res.inprog[0].opid;
        assert.commandWorked(donorPrimary.adminCommand({killOp: 1, op: opid}));

        assert.commandWorked(donorPrimary.adminCommand({configureFailPoint: fpName, mode: "off"}));

        runMigrationThread.join();

        TenantMigrationTest.assertCommitted(runMigrationThread.returnData());
        assert.commandWorked(tenantMigrationTest.forgetMigration(migrationOpts.migrationIdString));
        tenantMigrationTest.stop();
    }
}

{
    let fpNames = [
        "pauseTenantMigrationDonorBeforeMarkingStateGarbageCollectable",
        "pauseTenantMigrationBeforeMarkingExternalKeysGarbageCollectable"
    ];
    for (let fpName of fpNames) {
        const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});
        // This section is testing behavior during garbage collection.
        tenantMigrationTest.getDonorRst().stopSet();
        tenantMigrationTest.getDonorRst().startSet(
            Object.assign({}, migrationX509Options.donor, {setParameter: garbageCollectionOpts}));
        tenantMigrationTest.getDonorRst().initiate();
        createTenantMigrationRecipientRoleIfNotExist(tenantMigrationTest.getDonorRst());

        tenantMigrationTest.getRecipientRst().stopSet();
        tenantMigrationTest.getRecipientRst().startSet(Object.assign(
            {}, migrationX509Options.recipient, {setParameter: garbageCollectionOpts}));
        tenantMigrationTest.getRecipientRst().initiate();
        createTenantMigrationDonorRoleIfNotExist(tenantMigrationTest.getRecipientRst());

        jsTestLog(
            "Setting failpoint \"" + fpName +
            "\" during migration garbage collection to test that the migration will retry the " +
            "operation at the failpoint if a killOp is issued.");
        const migrationId = UUID();
        const tenantId = makeTenantId();
        const migrationOpts = {
            migrationIdString: extractUUIDFromObject(migrationId),
            recipientConnString: tenantMigrationTest.getRecipientConnString(),
            tenantId: tenantId,
        };

        let fp = configureFailPoint(tenantMigrationTest.getDonorPrimary(), fpName);

        TenantMigrationTest.assertCommitted(
            tenantMigrationTest.runMigration(migrationOpts, {automaticForgetMigration: false}));

        const donorPrimary = tenantMigrationTest.getDonorPrimary();
        const donorRstArgs = createRstArgs(tenantMigrationTest.getDonorRst());
        const forgetMigrationThread =
            new Thread(forgetMigrationAsync, migrationOpts.migrationIdString, donorRstArgs);
        forgetMigrationThread.start();

        fp.wait();

        const res = assert.commandWorked(donorPrimary.adminCommand({
            currentOp: true,
            $all: true,
            desc: {$regex: 'TenantMigrationDonorService'},
            opid: {$exists: true}
        }));
        const opid = res.inprog[0].opid;
        assert.commandWorked(donorPrimary.adminCommand({killOp: 1, op: opid}));

        fp.off();
        forgetMigrationThread.join();
        tenantMigrationTest.waitForMigrationGarbageCollection(migrationId, tenantId);
        tenantMigrationTest.stop();
    }
}
