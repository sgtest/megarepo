/**
 * Tests that tenant migrations are interrupted successfully on stepdown and shutdown.
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
import {
    forgetMigrationAsync,
    runMigrationAsync,
    tryAbortMigrationAsync
} from "jstests/replsets/libs/tenant_migration_util.js";
import {createRstArgs} from "jstests/replsets/rslib.js";

const kMaxSleepTimeMS = 100;
const kTenantId = ObjectId().str;
const kMigrationFpNames = [
    "pauseTenantMigrationBeforeLeavingDataSyncState",
    "pauseTenantMigrationBeforeLeavingBlockingState",
    "abortTenantMigrationBeforeLeavingBlockingState",
    ""
];

/**
 * Runs the donorStartMigration command to start a migration, and interrupts the migration on the
 * donor using the 'interruptFunc', and verifies the command response using the
 * 'verifyCmdResponseFunc'.
 */
function testDonorStartMigrationInterrupt(interruptFunc, verifyCmdResponseFunc) {
    const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

    const donorRst = tenantMigrationTest.getDonorRst();
    const donorPrimary = tenantMigrationTest.getDonorPrimary();

    const migrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: kTenantId,
        recipientConnString: tenantMigrationTest.getRecipientConnString(),
    };

    const donorRstArgs = createRstArgs(donorRst);

    const runMigrationThread = new Thread(runMigrationAsync, migrationOpts, donorRstArgs);
    runMigrationThread.start();

    // Wait for donorStartMigration command to start.
    assert.soon(() => donorPrimary.adminCommand({currentOp: true, desc: "tenant donor migration"})
                          .inprog.length > 0);

    sleep(Math.random() * kMaxSleepTimeMS);
    interruptFunc(donorRst, migrationId, kTenantId);
    verifyCmdResponseFunc(runMigrationThread);

    tenantMigrationTest.stop();
}

/**
 * Starts a migration and waits for it to commit, then runs the donorForgetMigration, and interrupts
 * the donor using the 'interruptFunc', and verifies the command response using the
 * 'verifyCmdResponseFunc'.
 */
function testDonorForgetMigrationInterrupt(interruptFunc, verifyCmdResponseFunc) {
    const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

    const donorRst = tenantMigrationTest.getDonorRst();
    const donorPrimary = tenantMigrationTest.getDonorPrimary();

    const migrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: kTenantId,
        recipientConnString: tenantMigrationTest.getRecipientConnString(),
    };

    const donorRstArgs = createRstArgs(donorRst);

    TenantMigrationTest.assertCommitted(
        tenantMigrationTest.runMigration(migrationOpts, {automaticForgetMigration: false}));
    const forgetMigrationThread =
        new Thread(forgetMigrationAsync, migrationOpts.migrationIdString, donorRstArgs);
    forgetMigrationThread.start();

    // Wait for the donorForgetMigration command to start.
    assert.soon(() => {
        const res = assert.commandWorked(
            donorPrimary.adminCommand({currentOp: true, desc: "tenant donor migration"}));
        return res.inprog[0].expireAt != null;
    });

    sleep(Math.random() * kMaxSleepTimeMS);
    interruptFunc(donorRst, migrationId, migrationOpts.tenantId);
    verifyCmdResponseFunc(forgetMigrationThread);

    tenantMigrationTest.stop();
}

/**
 * Starts a migration and sets the passed in failpoint during the migration, then runs the
 * donorAbortMigration, and interrupts the donor using the 'interruptFunc', and verifies the command
 * response using the 'verifyCmdResponseFunc'.
 */
function testDonorAbortMigrationInterrupt(interruptFunc, verifyCmdResponseFunc, fpName) {
    const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

    const donorRst = tenantMigrationTest.getDonorRst();
    const donorPrimary = tenantMigrationTest.getDonorPrimary();

    const migrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: kTenantId,
        recipientConnString: tenantMigrationTest.getRecipientConnString(),
    };

    const donorRstArgs = createRstArgs(donorRst);

    // If we passed in a valid failpoint we set it, otherwise we let the migration run normally.
    if (fpName) {
        configureFailPoint(donorPrimary, fpName);
    }

    assert.commandWorked(tenantMigrationTest.startMigration(migrationOpts));

    const tryAbortThread = new Thread(
        tryAbortMigrationAsync, {migrationIdString: migrationOpts.migrationIdString}, donorRstArgs);
    tryAbortThread.start();

    // Wait for donorAbortMigration command to start.
    assert.soon(() => {
        const res = assert.commandWorked(
            donorPrimary.adminCommand({currentOp: true, desc: "tenant donor migration"}));
        return res.inprog[0].receivedCancellation;
    });

    interruptFunc(donorRst, migrationId, migrationOpts.tenantId);
    verifyCmdResponseFunc(tryAbortThread);

    tenantMigrationTest.stop();
}

/**
 * Asserts the command either succeeded or failed with a NotPrimary error.
 */
function assertCmdSucceededOrInterruptedDueToStepDown(cmdThread) {
    const res = cmdThread.returnData();
    assert(res.ok || res.code === ErrorCodes.TenantMigrationCommitted ||
               ErrorCodes.isNotPrimaryError(res.code),
           res);
}

/**
 * Asserts the command either succeeded or failed with a NotPrimary or shutdown or network error.
 */
function assertCmdSucceededOrInterruptedDueToShutDown(cmdThread) {
    const res = cmdThread.returnData();
    try {
        assert(res.ok || res.code === ErrorCodes.TenantMigrationCommitted ||
                   ErrorCodes.isNotPrimaryError(res.code) || ErrorCodes.isShutdownError(res.code),
               res);
    } catch (e) {
        if (isNetworkError(e)) {
            jsTestLog(`Ignoring network error due to node shutting down ${tojson(e)}`);
        } else {
            throw e;
        }
    }
}

(() => {
    jsTest.log("Test that the donorStartMigration command is interrupted successfully on stepdown");
    testDonorStartMigrationInterrupt((donorRst) => {
        assert.commandWorked(
            donorRst.getPrimary().adminCommand({replSetStepDown: 1000, force: true}));
    }, assertCmdSucceededOrInterruptedDueToStepDown);
})();

(() => {
    jsTest.log("Test that the donorStartMigration command is interrupted successfully on shutdown");
    testDonorStartMigrationInterrupt((donorRst) => {
        donorRst.stopSet();
    }, assertCmdSucceededOrInterruptedDueToShutDown);
})();

(() => {
    jsTest.log("Test that the donorForgetMigration is interrupted successfully on stepdown");
    testDonorForgetMigrationInterrupt((donorRst) => {
        assert.commandWorked(
            donorRst.getPrimary().adminCommand({replSetStepDown: 1000, force: true}));
    }, assertCmdSucceededOrInterruptedDueToStepDown);
})();

(() => {
    jsTest.log("Test that the donorForgetMigration is interrupted successfully on shutdown");
    testDonorForgetMigrationInterrupt((donorRst) => {
        donorRst.stopSet();
    }, assertCmdSucceededOrInterruptedDueToShutDown);
})();

(() => {
    jsTest.log("Test that the donorAbortMigration is interrupted successfully on stepdown");
    kMigrationFpNames.forEach(fpName => {
        if (!fpName) {
            jsTest.log("Testing without setting a failpoint.");
        } else {
            jsTest.log("Testing with failpoint: " + fpName);
        }

        testDonorAbortMigrationInterrupt((donorRst) => {
            assert.commandWorked(
                donorRst.getPrimary().adminCommand({replSetStepDown: 1000, force: true}));
        }, assertCmdSucceededOrInterruptedDueToStepDown, fpName);
    });
})();

(() => {
    jsTest.log("Test that the donorAbortMigration is interrupted successfully on shutdown");
    kMigrationFpNames.forEach(fpName => {
        if (!fpName) {
            jsTest.log("Testing without setting a failpoint.");
        } else {
            jsTest.log("Testing with failpoint: " + fpName);
        }

        testDonorAbortMigrationInterrupt((donorRst) => {
            donorRst.stopSet();
        }, assertCmdSucceededOrInterruptedDueToShutDown, fpName);
    });
})();
