/**
 * Tests that tenant migration donor's in memory state is initialized correctly on initial sync.
 * This test randomly selects a point during the migration to add a node to the donor replica set.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_fcv_62,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   serverless,
 *   requires_fcv_71,
 * ]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {restartServerReplication, stopServerReplication} from "jstests/libs/write_concern_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {
    getServerlessOperationLock,
    ServerlessLockType
} from "jstests/replsets/libs/tenant_migration_util.js";

const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

const kMaxSleepTimeMS = 1000;
const kTenantId = ObjectId().str;

let donorPrimary = tenantMigrationTest.getDonorPrimary();

// Force the migration to pause after entering a randomly selected state.
Random.setRandomSeed();
const kMigrationFpNames = [
    "pauseTenantMigrationBeforeLeavingDataSyncState",
    "pauseTenantMigrationBeforeLeavingBlockingState",
    "abortTenantMigrationBeforeLeavingBlockingState"
];
let fp;
const index = Random.randInt(kMigrationFpNames.length + 1);
if (index < kMigrationFpNames.length) {
    fp = configureFailPoint(donorPrimary, kMigrationFpNames[index]);
}

const donorRst = tenantMigrationTest.getDonorRst();
const hangInDonorAfterReplicatingKeys =
    configureFailPoint(donorRst.getPrimary(), "pauseTenantMigrationAfterFetchingAndStoringKeys");
const migrationOpts = {
    migrationIdString: extractUUIDFromObject(UUID()),
    tenantId: kTenantId
};

assert.commandWorked(tenantMigrationTest.startMigration(migrationOpts));
// We must wait for the migration to have finished replicating the recipient keys on the donor set
// before starting initial sync, otherwise the migration will hang while waiting for initial sync to
// complete. We wait for the keys to be replicated with 'w: all' write concern.
hangInDonorAfterReplicatingKeys.wait();

// Add the initial sync node and make sure that it does not step up. We must add this node before
// sending the first 'recipientSyncData' command to avoid the scenario where a new donor node is
// added in-between 'recipientSyncData' commands to the recipient, prompting a
// 'ConflictingOperationInProgress' error. We do not support reconfigs that add/removes nodes during
// a migration.
const initialSyncNode = donorRst.add({
    rsConfig: {priority: 0, votes: 0},
    setParameter: {"failpoint.initialSyncHangBeforeChoosingSyncSource": tojson({mode: "alwaysOn"})}
});
donorRst.reInitiate();
donorRst.waitForState(initialSyncNode, ReplSetTest.State.STARTUP_2);
// Resume the migration. Wait randomly before resuming initial sync on the new secondary to test
// the various migration states.
hangInDonorAfterReplicatingKeys.off();
sleep(Math.random() * kMaxSleepTimeMS);
if (fp) {
    fp.wait();
}

jsTestLog("Waiting for initial sync to finish: " + initialSyncNode.port);
initialSyncNode.getDB('admin').adminCommand(
    {configureFailPoint: 'initialSyncHangBeforeChoosingSyncSource', mode: "off"});
donorRst.awaitSecondaryNodes();
donorRst.awaitReplication();

// Stop replication on the node so that the TenantMigrationAccessBlocker cannot transition its state
// past what is reflected in the state doc read below.
stopServerReplication(initialSyncNode);

let configDonorsColl = initialSyncNode.getCollection(TenantMigrationTest.kConfigDonorsNS);
assert.lte(configDonorsColl.count(), 1);
let donorDoc = configDonorsColl.findOne();
if (donorDoc) {
    jsTestLog("Initial sync completed while migration was in state: " + donorDoc.state);
    switch (donorDoc.state) {
        case TenantMigrationTest.DonorState.kAbortingIndexBuilds:
        case TenantMigrationTest.DonorState.kDataSync:
            assert.soon(() => tenantMigrationTest
                                  .getTenantMigrationAccessBlocker(
                                      {donorNode: initialSyncNode, tenantId: kTenantId})
                                  .donor.state == TenantMigrationTest.DonorAccessState.kAllow);
            break;
        case TenantMigrationTest.DonorState.kBlocking:
            assert.soon(() => tenantMigrationTest
                                  .getTenantMigrationAccessBlocker(
                                      {donorNode: initialSyncNode, tenantId: kTenantId})
                                  .donor.state ==
                            TenantMigrationTest.DonorAccessState.kBlockWritesAndReads);
            assert.soon(() =>
                            bsonWoCompare(tenantMigrationTest
                                              .getTenantMigrationAccessBlocker(
                                                  {donorNode: initialSyncNode, tenantId: kTenantId})
                                              .donor.blockTimestamp,
                                          donorDoc.blockTimestamp) == 0);
            break;
        case TenantMigrationTest.DonorState.kCommitted:
            assert.soon(() => tenantMigrationTest
                                  .getTenantMigrationAccessBlocker(
                                      {donorNode: initialSyncNode, tenantId: kTenantId})
                                  .donor.state == TenantMigrationTest.DonorAccessState.kReject);
            assert.soon(() =>
                            bsonWoCompare(tenantMigrationTest
                                              .getTenantMigrationAccessBlocker(
                                                  {donorNode: initialSyncNode, tenantId: kTenantId})
                                              .donor.commitOpTime,
                                          donorDoc.commitOrAbortOpTime) == 0);
            assert.soon(() =>
                            bsonWoCompare(tenantMigrationTest
                                              .getTenantMigrationAccessBlocker(
                                                  {donorNode: initialSyncNode, tenantId: kTenantId})
                                              .donor.blockTimestamp,
                                          donorDoc.blockTimestamp) == 0);
            break;
        case TenantMigrationTest.DonorState.kAborted:
            assert.soon(() => tenantMigrationTest
                                  .getTenantMigrationAccessBlocker(
                                      {donorNode: initialSyncNode, tenantId: kTenantId})
                                  .donor.state == TenantMigrationTest.DonorAccessState.kAborted);
            assert.soon(() =>
                            bsonWoCompare(tenantMigrationTest
                                              .getTenantMigrationAccessBlocker(
                                                  {donorNode: initialSyncNode, tenantId: kTenantId})
                                              .donor.abortOpTime,
                                          donorDoc.commitOrAbortOpTime) == 0);
            assert.soon(() =>
                            bsonWoCompare(tenantMigrationTest
                                              .getTenantMigrationAccessBlocker(
                                                  {donorNode: initialSyncNode, tenantId: kTenantId})
                                              .donor.blockTimestamp,
                                          donorDoc.blockTimestamp) == 0);
            break;
        default:
            throw new Error(`Invalid state "${donorDoc.state}" from donor doc.`);
    }
}

const activeServerlessLock = getServerlessOperationLock(initialSyncNode);
if (donorDoc && !donorDoc.expireAt) {
    assert.eq(activeServerlessLock, ServerlessLockType.TenantMigrationDonor);
} else {
    assert.eq(activeServerlessLock, ServerlessLockType.None);
}

if (fp) {
    fp.off();
}

restartServerReplication(initialSyncNode);

if (kMigrationFpNames[index] === "abortTenantMigrationBeforeLeavingBlockingState") {
    TenantMigrationTest.assertAborted(
        tenantMigrationTest.waitForMigrationToComplete(migrationOpts));
} else {
    TenantMigrationTest.assertCommitted(
        tenantMigrationTest.waitForMigrationToComplete(migrationOpts));
}
assert.commandWorked(tenantMigrationTest.forgetMigration(migrationOpts.migrationIdString));
tenantMigrationTest.stop();
