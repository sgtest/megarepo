/**
 * Tests that starting a migration fails if the donor and recipient do not share the same FCV.
 * @tags: [
 *   requires_majority_read_concern,
 *   incompatible_with_windows_tls,
 *   serverless,
 * ]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {isShardMergeEnabled, makeTenantDB} from "jstests/replsets/libs/tenant_migration_util.js";

if (!buildInfo()["modules"].includes("enterprise")) {
    jsTestLog("Skipping test as it requires the enterprise module");
    quit();
}

function runTest(downgradeFCV) {
    const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

    const tenantId = ObjectId().str;
    const dbName = makeTenantDB(tenantId, "testDB");
    const collName = "testColl";

    const donorPrimary = tenantMigrationTest.getDonorPrimary();
    const recipientPrimary = tenantMigrationTest.getRecipientPrimary();

    tenantMigrationTest.insertDonorDB(dbName, collName);

    const migrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        recipientConnString: tenantMigrationTest.getRecipientConnString(),
        tenantId: tenantId,
    };

    // Configure a failpoint to have the recipient primary hang after taking note of its FCV
    // and before comparing it with that of the donor.
    const recipientDB = recipientPrimary.getDB(dbName);
    const hangAfterSavingFCV = configureFailPoint(
        recipientDB, "fpAfterRecordingRecipientPrimaryStartingFCV", {action: "hang"});

    // Start a migration and wait for recipient to hang at the failpoint.
    assert.commandWorked(tenantMigrationTest.startMigration(migrationOpts));
    hangAfterSavingFCV.wait();

    // Downgrade the FCV for the donor set and resume migration.
    assert.commandWorked(
        donorPrimary.adminCommand({setFeatureCompatibilityVersion: downgradeFCV, confirm: true}));
    hangAfterSavingFCV.off();

    // Make sure we see the FCV mismatch detection message on the recipient.
    if (isShardMergeEnabled(recipientDB)) {
        checkLog.containsJson(recipientPrimary, 7339749);
    } else {
        checkLog.containsJson(recipientPrimary, 5382300);
    }

    // Upgrade again to check on the status of the migration from the donor's point of view.
    assert.commandWorked(
        donorPrimary.adminCommand({setFeatureCompatibilityVersion: latestFCV, confirm: true}));
    TenantMigrationTest.assertAborted(
        tenantMigrationTest.waitForMigrationToComplete(migrationOpts));

    tenantMigrationTest.stop();
}

runTest(lastContinuousFCV);
if (lastContinuousFCV != lastLTSFCV) {
    runTest(lastLTSFCV);
}
