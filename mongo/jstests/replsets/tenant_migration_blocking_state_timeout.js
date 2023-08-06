/**
 * Tests tenant migration timeout scenarios.
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
import {
    runMigrationAsync,
} from "jstests/replsets/libs/tenant_migration_util.js";
import {createRstArgs} from "jstests/replsets/rslib.js";

const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

function testTimeoutBlockingState() {
    const donorRst = tenantMigrationTest.getDonorRst();
    const donorPrimary = donorRst.getPrimary();
    let savedTimeoutParam = assert.commandWorked(donorPrimary.adminCommand({
        getParameter: 1,
        tenantMigrationBlockingStateTimeoutMS: 1
    }))['tenantMigrationBlockingStateTimeoutMS'];

    assert.commandWorked(
        donorPrimary.adminCommand({setParameter: 1, tenantMigrationBlockingStateTimeoutMS: 5000}));

    const tenantId = ObjectId().str;
    const migrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId,
        recipientConnString: tenantMigrationTest.getRecipientConnString(),
    };

    const donorRstArgs = createRstArgs(donorRst);

    // Fail point to pause right before entering the blocking mode.
    let afterDataSyncFp =
        configureFailPoint(donorPrimary, "pauseTenantMigrationBeforeLeavingDataSyncState");

    // Run the migration in its own thread, since the initial 'donorStartMigration' command will
    // hang due to the fail point.
    let migrationThread = new Thread(runMigrationAsync, migrationOpts, donorRstArgs);
    migrationThread.start();

    afterDataSyncFp.wait();
    // Fail point to pause the '_sendRecipientSyncDataCommand()' call inside the blocking state
    // until the cancellation token for the method is cancelled.
    let inCallFp =
        configureFailPoint(donorPrimary, "pauseScheduleCallWithCancelTokenUntilCanceled");
    afterDataSyncFp.off();

    tenantMigrationTest.waitForDonorNodesToReachState(
        donorRst.nodes, migrationId, tenantId, TenantMigrationTest.DonorState.kAborted);

    TenantMigrationTest.assertAborted(migrationThread.returnData(), ErrorCodes.ExceededTimeLimit);

    // This fail point is pausing all calls to recipient, so it has to be disabled to make
    // the 'forget migration' command to work.
    inCallFp.off();
    assert.commandWorked(tenantMigrationTest.forgetMigration(migrationOpts.migrationIdString));
    assert.commandWorked(donorPrimary.adminCommand(
        {setParameter: 1, tenantMigrationBlockingStateTimeoutMS: savedTimeoutParam}));
}

jsTest.log("Test timeout of the blocking state");
testTimeoutBlockingState();

tenantMigrationTest.stop();
