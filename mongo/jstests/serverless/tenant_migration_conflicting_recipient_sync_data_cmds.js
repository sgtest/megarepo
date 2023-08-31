/**
 * Test that tenant migration recipient rejects conflicting recipientSyncData commands.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   # Shard merge protocol will be tested by
 *   # tenant_migration_shard_merge_conflicting_recipient_sync_data_cmds.js.
 *   incompatible_with_shard_merge,
 *   requires_fcv_52,
 *   incompatible_with_windows_tls,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   serverless,
 *   requires_fcv_71,
 * ]
 */

import {configureFailPoint, kDefaultWaitForFailPointTimeout} from "jstests/libs/fail_point_util.js";
import {Thread} from "jstests/libs/parallelTester.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {isShardMergeEnabled} from "jstests/replsets/libs/tenant_migration_util.js";

var rst = new ReplSetTest({nodes: 1, serverless: true});
rst.startSet();
rst.initiate();
const primary = rst.getPrimary();

if (isShardMergeEnabled(primary.getDB("admin"))) {
    rst.stopSet();
    jsTestLog("Skipping this shard merge incompatible test.");
    quit();
}

const configDB = primary.getDB("config");
const configRecipientsColl = configDB["tenantMigrationRecipients"];

const kDonorConnectionString0 = "foo/bar:12345";
const kDonorConnectionString1 = "foo/bar:56789";
const kPrimaryReadPreference = {
    mode: "primary"
};
const kSecondaryReadPreference = {
    mode: "secondary"
};

TestData.stopFailPointErrorCode = 4880402;

/**
 * Runs recipientSyncData on the given host and returns the response.
 */
function runRecipientSyncDataCmd(primaryHost, {
    migrationIdString,
    tenantId,
    donorConnectionString,
    readPreference,
}) {
    jsTestLog("Starting a recipientSyncDataCmd for migrationId: " + migrationIdString +
              " tenantId: '" + tenantId + "'");
    const primary = new Mongo(primaryHost);
    const res = primary.adminCommand({
        recipientSyncData: 1,
        migrationId: UUID(migrationIdString),
        donorConnectionString: donorConnectionString,
        tenantId: tenantId,
        readPreference: readPreference,
        startMigrationDonorTimestamp: Timestamp(1, 1),
    });
    return res;
}

/**
 * Returns an array of currentOp entries for the TenantMigrationRecipientService instances that
 * match the given query.
 */
function getTenantMigrationRecipientCurrentOpEntries(recipientPrimary, query) {
    const cmdObj = Object.assign({currentOp: true, desc: "tenant recipient migration"}, query);
    return assert.commandWorked(recipientPrimary.adminCommand(cmdObj)).inprog;
}

// Enable the failpoint to stop the tenant migration after persisting the state doc.
assert.commandWorked(primary.adminCommand({
    configureFailPoint: "fpAfterPersistingTenantMigrationRecipientInstanceStateDoc",
    mode: "alwaysOn",
    data: {action: "stop", stopErrorCode: NumberInt(TestData.stopFailPointErrorCode)}
}));

// Test migrations with different migrationIds but identical settings.
(() => {
    const tenantId = ObjectId().str;
    // Enable failPoint to pause the migration just as it starts.
    const fpPauseBeforeRunTenantMigrationRecipientInstance =
        configureFailPoint(primary, "pauseBeforeRunTenantMigrationRecipientInstance");

    // Start the conflicting recipientSyncData cmds.
    const migrationOpts0 = {
        migrationIdString: extractUUIDFromObject(UUID()),
        tenantId: tenantId,
        donorConnectionString: kDonorConnectionString0,
        readPreference: kPrimaryReadPreference,
    };
    const migrationOpts1 = Object.extend({}, migrationOpts0, true);
    migrationOpts1.migrationIdString = extractUUIDFromObject(UUID());
    const recipientSyncDataThread0 =
        new Thread(runRecipientSyncDataCmd, primary.host, migrationOpts0);
    const recipientSyncDataThread1 =
        new Thread(runRecipientSyncDataCmd, primary.host, migrationOpts1);
    recipientSyncDataThread0.start();
    recipientSyncDataThread1.start();

    jsTestLog("Waiting until one gets started and hits the failPoint.");
    assert.commandWorked(primary.adminCommand({
        waitForFailPoint: "pauseBeforeRunTenantMigrationRecipientInstance",
        timesEntered: fpPauseBeforeRunTenantMigrationRecipientInstance.timesEntered + 1,
        maxTimeMS: kDefaultWaitForFailPointTimeout
    }));

    // One instance is expected as the tenantId conflict is still unresolved.
    jsTestLog("Fetching current operations before conflict is resolved.");
    const currentOpEntriesBeforeInsert = getTenantMigrationRecipientCurrentOpEntries(
        primary, {desc: "tenant recipient migration", tenantId});
    assert.eq(1, currentOpEntriesBeforeInsert.length, tojson(currentOpEntriesBeforeInsert));

    jsTestLog("Unblocking the tenant migration instance from persisting the state doc.");
    fpPauseBeforeRunTenantMigrationRecipientInstance.off();

    // Check responses for both commands. One will return with
    // ErrorCodes.ConflictingOperationInProgress, and the other with a
    // TestData.stopFailPointErrorCode (a failpoint indicating that we have persisted the document).
    const res0 = assert.commandFailed(recipientSyncDataThread0.returnData());
    const res1 = assert.commandFailed(recipientSyncDataThread1.returnData());

    if (res0.code == TestData.stopFailPointErrorCode) {
        assert.commandFailedWithCode(res0, TestData.stopFailPointErrorCode);
        assert.commandFailedWithCode(res1, ErrorCodes.ConflictingOperationInProgress);
    } else {
        assert.commandFailedWithCode(res0, ErrorCodes.ConflictingOperationInProgress);
        assert.commandFailedWithCode(res1, TestData.stopFailPointErrorCode);
    }

    // One of the two instances should have been cleaned up, and therefore only one will remain.
    const currentOpEntriesAfterInsert = getTenantMigrationRecipientCurrentOpEntries(
        primary, {desc: "tenant recipient migration", tenantId});
    assert.eq(1, currentOpEntriesAfterInsert.length, tojson(currentOpEntriesAfterInsert));

    // Only one instance should have succeeded in persisting the state doc, other should have failed
    // with ErrorCodes.ConflictingOperationInProgress.
    assert.eq(1, configRecipientsColl.count({}));

    // Run another recipientSyncData cmd for the tenant. Since the previous migration hasn't been
    // garbage collected, the migration is considered as active. So this command should fail with
    // ErrorCodes.ConflictingOperationInProgress.
    const migrationOpts2 = Object.extend({}, migrationOpts0, true);
    migrationOpts2.migrationIdString = extractUUIDFromObject(UUID());
    const recipientSyncDataCmd2 = new Thread(runRecipientSyncDataCmd, primary.host, migrationOpts2);
    recipientSyncDataCmd2.start();
    const res2 = recipientSyncDataCmd2.returnData();
    assert.commandFailedWithCode(res2, ErrorCodes.ConflictingOperationInProgress);

    // Collection count should remain the same.
    assert.eq(1, configRecipientsColl.count({}));
    fpPauseBeforeRunTenantMigrationRecipientInstance.off();
})();

/**
 * Tests that if the client runs multiple recipientSyncData commands that would start conflicting
 * migrations, only one of the migrations will start and succeed.
 */
function testConcurrentConflictingMigration(migrationOpts0, migrationOpts1) {
    // Start the conflicting recipientSyncData cmds.
    const recipientSyncDataThread0 =
        new Thread(runRecipientSyncDataCmd, primary.host, migrationOpts0);
    const recipientSyncDataThread1 =
        new Thread(runRecipientSyncDataCmd, primary.host, migrationOpts1);
    recipientSyncDataThread0.start();
    recipientSyncDataThread1.start();

    const res0 = assert.commandFailed(recipientSyncDataThread0.returnData());
    const res1 = assert.commandFailed(recipientSyncDataThread1.returnData());

    if (res0.code == TestData.stopFailPointErrorCode) {
        assert.commandFailedWithCode(res0, TestData.stopFailPointErrorCode);
        assert.commandFailedWithCode(res1, ErrorCodes.ConflictingOperationInProgress);
        assert.eq(1, configRecipientsColl.count({_id: UUID(migrationOpts0.migrationIdString)}));
        assert.eq(1, getTenantMigrationRecipientCurrentOpEntries(primary, {
                         "instanceID": UUID(migrationOpts0.migrationIdString)
                     }).length);
        if (migrationOpts0.migrationIdString != migrationOpts1.migrationIdString) {
            assert.eq(0, configRecipientsColl.count({_id: UUID(migrationOpts1.migrationIdString)}));
            assert.eq(0, getTenantMigrationRecipientCurrentOpEntries(primary, {
                             "instanceID": UUID(migrationOpts1.migrationIdString)
                         }).length);
        } else if (migrationOpts0.tenantId != migrationOpts1.tenantId) {
            assert.eq(0, configRecipientsColl.count({tenantId: migrationOpts1.tenantId}));
            assert.eq(0, getTenantMigrationRecipientCurrentOpEntries(primary, {
                             tenantId: migrationOpts1.tenantId
                         }).length);
        }
    } else {
        assert.commandFailedWithCode(res0, ErrorCodes.ConflictingOperationInProgress);
        assert.commandFailedWithCode(res1, TestData.stopFailPointErrorCode);
        assert.eq(1, configRecipientsColl.count({_id: UUID(migrationOpts1.migrationIdString)}));
        assert.eq(1, getTenantMigrationRecipientCurrentOpEntries(primary, {
                         "instanceID": UUID(migrationOpts1.migrationIdString)
                     }).length);
        if (migrationOpts0.migrationIdString != migrationOpts1.migrationIdString) {
            assert.eq(0, configRecipientsColl.count({_id: UUID(migrationOpts0.migrationIdString)}));
            assert.eq(0, getTenantMigrationRecipientCurrentOpEntries(primary, {
                             "instanceID": UUID(migrationOpts0.migrationIdString)
                         }).length);
        } else if (migrationOpts0.tenantId != migrationOpts1.tenantId) {
            assert.eq(0, configRecipientsColl.count({tenantId: migrationOpts0.tenantId}));
            assert.eq(0, getTenantMigrationRecipientCurrentOpEntries(primary, {
                             tenantId: migrationOpts0.tenantId
                         }).length);
        }
    }
}

// Test reusing a migrationId with different migration settings.

// Test different tenantIds.
(() => {
    const migrationOpts0 = {
        migrationIdString: extractUUIDFromObject(UUID()),
        tenantId: ObjectId().str,
        donorConnectionString: kDonorConnectionString0,
        readPreference: kPrimaryReadPreference,
    };
    const migrationOpts1 = Object.extend({}, migrationOpts0, true);
    migrationOpts1.tenantId = ObjectId().str;
    testConcurrentConflictingMigration(migrationOpts0, migrationOpts1);
})();

// Test different donor connection strings.
(() => {
    const migrationOpts0 = {
        migrationIdString: extractUUIDFromObject(UUID()),
        tenantId: ObjectId().str,
        donorConnectionString: kDonorConnectionString0,
        readPreference: kPrimaryReadPreference,
    };
    const migrationOpts1 = Object.extend({}, migrationOpts0, true);
    migrationOpts1.donorConnectionString = kDonorConnectionString1;
    testConcurrentConflictingMigration(migrationOpts0, migrationOpts1);
})();

// Test different read preference.
(() => {
    const migrationOpts0 = {
        migrationIdString: extractUUIDFromObject(UUID()),
        tenantId: ObjectId().str,
        donorConnectionString: kDonorConnectionString0,
        readPreference: kPrimaryReadPreference,
    };
    const migrationOpts1 = Object.extend({}, migrationOpts0, true);
    migrationOpts1.readPreference = kSecondaryReadPreference;
    testConcurrentConflictingMigration(migrationOpts0, migrationOpts1);
})();

rst.stopSet();
