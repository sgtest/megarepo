/**
 * Tests forgetMigration cleanup behavior.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_persistence,
 *   requires_replication,
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
    isShardMergeEnabled
} from "jstests/replsets/libs/tenant_migration_util.js";
import {createRstArgs} from "jstests/replsets/rslib.js";

const tenantMigrationTest = new TenantMigrationTest({
    name: jsTestName(),
    sharedOptions: {nodes: 2},
    quickGarbageCollection: true,
});

const kTenantId = ObjectId().str;
const kReadPreference = {
    mode: "primary"
};

const isShardMergeEnabledOnDonorPrimary =
    isShardMergeEnabled(tenantMigrationTest.getDonorPrimary().getDB("admin"));

const oplogBufferCollectionName = (migrationIdString) =>
    `repl.migration.oplog_${migrationIdString}`;
const donatedFilesCollectionName = (migrationIdString) => `donatedFiles.${migrationIdString}`;
const importMarkerCollName = (migrationIdString) => `importDoneMarker.${migrationIdString}`;

const assertTempCollectionsExist = (conn, migrationIdString) => {
    const collections = conn.getDB("config").getCollectionNames();
    assert(collections.includes(oplogBufferCollectionName(migrationIdString)), collections);
    if (isShardMergeEnabledOnDonorPrimary) {
        assert(collections.includes(donatedFilesCollectionName(migrationIdString)), collections);
        assert.eq(1,
                  conn.getDB("local")
                      .getCollectionInfos({name: importMarkerCollName(migrationIdString)})
                      .length);
    }
};

const assertTempCollectionsDoNotExist = (conn, migrationIdString) => {
    const collections = conn.getDB("config").getCollectionNames();
    assert(!collections.includes(oplogBufferCollectionName(migrationIdString)), collections);
    if (isShardMergeEnabledOnDonorPrimary) {
        assert(!collections.includes(donatedFilesCollectionName(migrationIdString)), collections);
        assert.eq(0,
                  conn.getDB("local")
                      .getCollectionInfos({name: importMarkerCollName(migrationIdString)})
                      .length);
    }
};

(() => {
    jsTestLog("Test that expected collections are cleaned up when forgetting a migration.");
    const kMigrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(kMigrationId),
        tenantId: kTenantId,
        readPreference: kReadPreference
    };

    TenantMigrationTest.assertCommitted(tenantMigrationTest.runMigration(
        migrationOpts, {retryOnRetryableErrors: true, automaticForgetMigration: false}));

    const fpBeforeDroppingTempCollections =
        configureFailPoint(tenantMigrationTest.getRecipientPrimary(),
                           "fpBeforeDroppingTempCollections",
                           {action: "hang"});

    jsTestLog("Issuing a forget migration command.");
    const forgetMigrationThread = new Thread(forgetMigrationAsync,
                                             migrationOpts.migrationIdString,
                                             createRstArgs(tenantMigrationTest.getDonorRst()),
                                             true /* retryOnRetryableErrors */);
    forgetMigrationThread.start();

    fpBeforeDroppingTempCollections.wait();

    const recipientPrimary = tenantMigrationTest.getRecipientPrimary();

    assertTempCollectionsExist(recipientPrimary, migrationOpts.migrationIdString);

    fpBeforeDroppingTempCollections.off();

    jsTestLog("Waiting for forget migration to complete.");
    assert.commandWorked(forgetMigrationThread.returnData());

    assertTempCollectionsDoNotExist(recipientPrimary, migrationOpts.migrationIdString);

    tenantMigrationTest.waitForMigrationGarbageCollection(migrationOpts.migrationIdString);
})();

(() => {
    jsTestLog(
        "Tests whether the new recipient primary properly processes a forgetMigration when " +
        "the original primary steps down after the migration is marked as garbage collectable.");
    const kMigrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(kMigrationId),
        tenantId: kTenantId,
        readPreference: kReadPreference
    };

    TenantMigrationTest.assertCommitted(tenantMigrationTest.runMigration(
        migrationOpts, {retryOnRetryableErrors: true, automaticForgetMigration: false}));

    const fpBeforeDroppingTempCollections =
        configureFailPoint(tenantMigrationTest.getRecipientPrimary(),
                           "fpBeforeDroppingTempCollections",
                           {action: "hang"});

    jsTestLog("Issuing a forget migration command.");
    const forgetMigrationThread = new Thread(forgetMigrationAsync,
                                             migrationOpts.migrationIdString,
                                             createRstArgs(tenantMigrationTest.getDonorRst()),
                                             true /* retryOnRetryableErrors */);
    forgetMigrationThread.start();

    fpBeforeDroppingTempCollections.wait();

    assertTempCollectionsExist(tenantMigrationTest.getRecipientPrimary(),
                               migrationOpts.migrationIdString);

    jsTestLog("Stepping up a new recipient primary.");
    tenantMigrationTest.getRecipientRst().stepUp(
        tenantMigrationTest.getRecipientRst().getSecondaries()[0]);

    fpBeforeDroppingTempCollections.off();

    jsTestLog("Waiting for forget migration to complete.");
    assert.commandWorked(forgetMigrationThread.returnData());

    assertTempCollectionsDoNotExist(tenantMigrationTest.getRecipientPrimary(),
                                    migrationOpts.migrationIdString);

    tenantMigrationTest.waitForMigrationGarbageCollection(migrationOpts.migrationIdString);
})();

tenantMigrationTest.stop();
