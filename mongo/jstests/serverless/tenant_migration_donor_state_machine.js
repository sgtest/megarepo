/**
 * Tests the TenantMigrationAccessBlocker and donor state document are updated correctly at each
 * stage of the migration, and are eventually removed after the donorForgetMigration has returned.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   # Some tenant migration statistics field names were changed in 6.1.
 *   requires_fcv_61,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   serverless,
 *   requires_fcv_71,
 * ]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {
    getTenantMigrationAccessBlocker,
    isShardMergeEnabled,
    makeX509OptionsForTest,
} from "jstests/replsets/libs/tenant_migration_util.js";

let expectedNumRecipientSyncDataCmdSent = 0;
let expectedNumRecipientForgetMigrationCmdSent = 0;
let expectedRecipientSyncDataMetricsFailed = 0;

/**
 * Runs the donorForgetMigration command and asserts that the TenantMigrationAccessBlocker and donor
 * state document are eventually removed from the donor.
 */
function testDonorForgetMigrationAfterMigrationCompletes(
    donorRst, recipientRst, migrationId, tenantId) {
    jsTest.log("Test donorForgetMigration after the migration completes");
    const donorPrimary = donorRst.getPrimary();
    const recipientPrimary = recipientRst.getPrimary();

    assert.commandWorked(
        donorPrimary.adminCommand({donorForgetMigration: 1, migrationId: migrationId}));

    expectedNumRecipientForgetMigrationCmdSent++;
    const recipientForgetMigrationMetrics =
        recipientPrimary.adminCommand({serverStatus: 1}).metrics.commands.recipientForgetMigration;
    assert.eq(recipientForgetMigrationMetrics.failed, 0);
    assert.eq(recipientForgetMigrationMetrics.total, expectedNumRecipientForgetMigrationCmdSent);

    // Wait for garbage collection on donor.
    donorRst.nodes.forEach((node) => {
        assert.soon(() => null == getTenantMigrationAccessBlocker({donorNode: node}));
    });

    assert.soon(() => 0 === donorPrimary.getCollection(TenantMigrationTest.kConfigDonorsNS).count({
        _id: migrationId,
    }));
    assert.soon(() => 0 ===
                    donorPrimary.adminCommand({serverStatus: 1})
                        .repl.primaryOnlyServices.TenantMigrationDonorService.numInstances);

    const donorRecipientMonitorPoolStats =
        donorPrimary.adminCommand({connPoolStats: 1}).replicaSets;
    assert.eq(Object.keys(donorRecipientMonitorPoolStats).length, 0);

    // Wait for garbage collection on recipient.
    recipientRst.nodes.forEach((node) => {
        assert.soon(() => null == getTenantMigrationAccessBlocker({recipientNode: node}));
    });

    const recipientStateDocNss = isShardMergeEnabled(recipientPrimary.getDB("admin"))
        ? TenantMigrationTest.kConfigShardMergeRecipientsNS
        : TenantMigrationTest.kConfigRecipientsNS;

    assert.soon(() => 0 === recipientPrimary.getCollection(recipientStateDocNss).count({
        _id: migrationId,
    }));
    assert.soon(() => 0 ===
                    recipientPrimary.adminCommand({serverStatus: 1})
                        .repl.primaryOnlyServices.TenantMigrationRecipientService.numInstances);

    const recipientRecipientMonitorPoolStats =
        recipientPrimary.adminCommand({connPoolStats: 1}).replicaSets;
    assert.eq(Object.keys(recipientRecipientMonitorPoolStats).length, 0);
}

const sharedOptions = {
    setParameter: {
        // Set the delay before a state doc is garbage collected to be short to speed up the test.
        tenantMigrationGarbageCollectionDelayMS: 3 * 1000,
        ttlMonitorSleepSecs: 1,
    }
};
const x509Options = makeX509OptionsForTest();

const donorRst = new ReplSetTest({
    nodes: [{}, {rsConfig: {priority: 0}}, {rsConfig: {priority: 0}}],
    name: "donor",
    serverless: true,
    nodeOptions: Object.assign(x509Options.donor, sharedOptions)
});

const recipientRst = new ReplSetTest({
    nodes: [{}, {rsConfig: {priority: 0}}, {rsConfig: {priority: 0}}],
    name: "recipient",
    serverless: true,
    nodeOptions: Object.assign(x509Options.recipient, sharedOptions)
});

donorRst.startSet();
donorRst.initiate();

recipientRst.startSet();
recipientRst.initiate();

const tenantMigrationTest = new TenantMigrationTest({name: jsTestName(), donorRst, recipientRst});

const donorPrimary = tenantMigrationTest.getDonorPrimary();
const recipientPrimary = tenantMigrationTest.getRecipientPrimary();

const kTenantId = ObjectId().str;

let configDonorsColl = donorPrimary.getCollection(TenantMigrationTest.kConfigDonorsNS);

function testStats(node, {
    currentMigrationsDonating = 0,
    currentMigrationsReceiving = 0,
    totalMigrationDonationsCommitted = 0,
    totalMigrationDonationsAborted = 0,
}) {
    const stats = tenantMigrationTest.getTenantMigrationStats(node);
    jsTestLog(stats);
    assert.eq(currentMigrationsDonating, stats.currentMigrationsDonating);
    assert.eq(currentMigrationsReceiving, stats.currentMigrationsReceiving);
    assert.eq(totalMigrationDonationsCommitted, stats.totalMigrationDonationsCommitted);
    assert.eq(totalMigrationDonationsAborted, stats.totalMigrationDonationsAborted);
}

(() => {
    jsTest.log("Test the case where the migration commits");
    const migrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: kTenantId,
    };

    let blockingFp =
        configureFailPoint(donorPrimary, "pauseTenantMigrationBeforeLeavingBlockingState");
    assert.commandWorked(tenantMigrationTest.startMigration(migrationOpts));

    // Wait for the migration to enter the blocking state.
    blockingFp.wait();

    let mtab = getTenantMigrationAccessBlocker({donorNode: donorPrimary, tenantId: kTenantId});
    assert.eq(mtab.donor.state, TenantMigrationTest.DonorAccessState.kBlockWritesAndReads);
    assert(mtab.donor.blockTimestamp);

    let donorDoc = configDonorsColl.findOne({_id: migrationId});
    let blockOplogEntry =
        donorPrimary.getDB("local")
            .oplog.rs.find({ns: TenantMigrationTest.kConfigDonorsNS, op: "u", "o._id": migrationId})
            .sort({"$natural": -1})
            .limit(1)
            .next();
    assert.eq(donorDoc.state, "blocking");
    assert.eq(donorDoc.blockTimestamp, blockOplogEntry.ts);
    if (isShardMergeEnabled(donorPrimary.getDB("admin"))) {
        assert.eq(donorDoc.protocol, "shard merge");
        assert.eq(donorDoc.tenantIds, [ObjectId(kTenantId)]);
    }

    // Verify that donorForgetMigration fails since the decision has not been made.
    assert.commandFailedWithCode(
        donorPrimary.adminCommand({donorForgetMigration: 1, migrationId: migrationId}),
        ErrorCodes.TenantMigrationInProgress);

    testStats(donorPrimary, {currentMigrationsDonating: 1});
    testStats(recipientPrimary, {currentMigrationsReceiving: 1});

    // Allow the migration to complete.
    blockingFp.off();
    TenantMigrationTest.assertCommitted(
        tenantMigrationTest.waitForMigrationToComplete(migrationOpts));

    donorDoc = configDonorsColl.findOne({_id: migrationId});
    let commitOplogEntry = donorPrimary.getDB("local").oplog.rs.findOne(
        {ns: TenantMigrationTest.kConfigDonorsNS, op: "u", o: donorDoc});
    assert.eq(donorDoc.state, TenantMigrationTest.DonorState.kCommitted);
    assert.eq(donorDoc.commitOrAbortOpTime.ts, commitOplogEntry.ts);

    assert.soon(() => {
        mtab = getTenantMigrationAccessBlocker({donorNode: donorPrimary, tenantId: kTenantId});
        return mtab.donor.state === TenantMigrationTest.DonorAccessState.kReject;
    });
    assert(mtab.donor.commitOpTime);

    expectedNumRecipientSyncDataCmdSent += 2;
    const recipientSyncDataMetrics =
        recipientPrimary.adminCommand({serverStatus: 1}).metrics.commands.recipientSyncData;
    assert.eq(recipientSyncDataMetrics.failed, 0);
    assert.eq(recipientSyncDataMetrics.total, expectedNumRecipientSyncDataCmdSent);

    testDonorForgetMigrationAfterMigrationCompletes(donorRst, recipientRst, migrationId, kTenantId);

    testStats(donorPrimary, {totalMigrationDonationsCommitted: 1});
})();

(() => {
    jsTest.log(
        "Test the case where the migration aborts after data becomes consistent on the recipient " +
        "but before setting the consistent promise.");
    const migrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: kTenantId,
    };

    let abortRecipientFp =
        configureFailPoint(recipientPrimary,
                           "fpBeforeFulfillingDataConsistentPromise",
                           {action: "stop", stopErrorCode: ErrorCodes.InternalError});
    TenantMigrationTest.assertAborted(
        tenantMigrationTest.runMigration(migrationOpts, {automaticForgetMigration: false}));
    abortRecipientFp.off();

    const donorDoc = configDonorsColl.findOne({_id: migrationId});
    const abortOplogEntry = donorPrimary.getDB("local").oplog.rs.findOne(
        {ns: TenantMigrationTest.kConfigDonorsNS, op: "u", o: donorDoc});
    assert.eq(donorDoc.state, TenantMigrationTest.DonorState.kAborted);
    assert.eq(donorDoc.commitOrAbortOpTime.ts, abortOplogEntry.ts);
    assert.eq(donorDoc.abortReason.code, ErrorCodes.InternalError);

    let mtab;
    assert.soon(() => {
        mtab = getTenantMigrationAccessBlocker({donorNode: donorPrimary, tenantId: kTenantId});
        return mtab.donor.state === TenantMigrationTest.DonorAccessState.kAborted;
    });
    assert(mtab.donor.abortOpTime);

    expectedRecipientSyncDataMetricsFailed++;
    expectedNumRecipientSyncDataCmdSent++;
    const recipientSyncDataMetrics =
        recipientPrimary.adminCommand({serverStatus: 1}).metrics.commands.recipientSyncData;
    assert.eq(recipientSyncDataMetrics.failed, expectedRecipientSyncDataMetricsFailed);
    assert.eq(recipientSyncDataMetrics.total, expectedNumRecipientSyncDataCmdSent);

    testDonorForgetMigrationAfterMigrationCompletes(donorRst, recipientRst, migrationId, kTenantId);

    testStats(donorPrimary,
              {totalMigrationDonationsCommitted: 1, totalMigrationDonationsAborted: 1});
})();

(() => {
    jsTest.log("Test the case where the migration aborts");
    const migrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: kTenantId,
    };

    let abortDonorFp =
        configureFailPoint(donorPrimary, "abortTenantMigrationBeforeLeavingBlockingState");
    TenantMigrationTest.assertAborted(
        tenantMigrationTest.runMigration(migrationOpts, {automaticForgetMigration: false}));
    abortDonorFp.off();

    const donorDoc = configDonorsColl.findOne({_id: migrationId});
    const abortOplogEntry = donorPrimary.getDB("local").oplog.rs.findOne(
        {ns: TenantMigrationTest.kConfigDonorsNS, op: "u", o: donorDoc});
    assert.eq(donorDoc.state, TenantMigrationTest.DonorState.kAborted);
    assert.eq(donorDoc.commitOrAbortOpTime.ts, abortOplogEntry.ts);
    assert.eq(donorDoc.abortReason.code, ErrorCodes.InternalError);

    let mtab;
    assert.soon(() => {
        mtab = getTenantMigrationAccessBlocker({donorNode: donorPrimary, tenantId: kTenantId});
        return mtab.donor.state === TenantMigrationTest.DonorAccessState.kAborted;
    });
    assert(mtab.donor.abortOpTime);

    expectedNumRecipientSyncDataCmdSent += 2;
    const recipientSyncDataMetrics =
        recipientPrimary.adminCommand({serverStatus: 1}).metrics.commands.recipientSyncData;
    assert.eq(recipientSyncDataMetrics.failed, expectedRecipientSyncDataMetricsFailed);
    assert.eq(recipientSyncDataMetrics.total, expectedNumRecipientSyncDataCmdSent);

    testDonorForgetMigrationAfterMigrationCompletes(donorRst, recipientRst, migrationId, kTenantId);

    testStats(donorPrimary,
              {totalMigrationDonationsCommitted: 1, totalMigrationDonationsAborted: 2});
})();

// Drop the TTL index to make sure that the migration state is still available when the
// donorForgetMigration command is retried.
configDonorsColl.dropIndex({expireAt: 1});

(() => {
    jsTest.log("Test that donorForgetMigration can be run multiple times");
    const migrationId = UUID();
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: kTenantId,
    };

    // Verify that donorForgetMigration fails since the migration hasn't started.
    assert.commandFailedWithCode(
        donorPrimary.adminCommand({donorForgetMigration: 1, migrationId: migrationId}),
        ErrorCodes.NoSuchTenantMigration);

    TenantMigrationTest.assertCommitted(
        tenantMigrationTest.runMigration(migrationOpts, {automaticForgetMigration: false}));
    assert.commandWorked(
        donorPrimary.adminCommand({donorForgetMigration: 1, migrationId: migrationId}));

    // Verify that the retry succeeds.
    assert.commandWorked(
        donorPrimary.adminCommand({donorForgetMigration: 1, migrationId: migrationId}));
})();

tenantMigrationTest.stop();
donorRst.stopSet();
recipientRst.stopSet();
