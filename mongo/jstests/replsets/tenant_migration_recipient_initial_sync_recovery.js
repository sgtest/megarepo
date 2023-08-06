/**
 * Tests that tenant migration recipient's in memory state is initialized correctly on initial sync.
 * This test randomly selects a point during the migration to add a node to the recipient.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_shard_merge,
 *   incompatible_with_windows_tls,
 *   # The error code for a rejected recipient command invoked during the reject phase was changed.
 *   requires_fcv_71,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   serverless,
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

const kMaxSleepTimeMS = 7500;
const kTenantId = ObjectId().str;

let recipientPrimary = tenantMigrationTest.getRecipientPrimary();

// Force the migration to pause after entering a randomly selected state.
Random.setRandomSeed();
const kMigrationFpNames = [
    "fpBeforeFetchingCommittedTransactions",
    "fpAfterWaitForRejectReadsBeforeTimestamp",
];
const index = Random.randInt(kMigrationFpNames.length + 1);
if (index < kMigrationFpNames.length) {
    configureFailPoint(recipientPrimary, kMigrationFpNames[index], {action: "hang"});
}

const migrationOpts = {
    migrationIdString: extractUUIDFromObject(UUID()),
    tenantId: kTenantId
};
assert.commandWorked(tenantMigrationTest.startMigration(migrationOpts));
sleep(Math.random() * kMaxSleepTimeMS);

// Add the initial sync node and make sure that it does not step up.
const recipientRst = tenantMigrationTest.getRecipientRst();
const initialSyncNode = recipientRst.add({rsConfig: {priority: 0, votes: 0}});

recipientRst.reInitiate();
jsTestLog("Waiting for initial sync to finish.");
recipientRst.awaitSecondaryNodes();
recipientRst.awaitReplication();

// Stop replication on the node so that the TenantMigrationAccessBlocker cannot transition its state
// past what is reflected in the state doc read below.
stopServerReplication(initialSyncNode);

const configRecipientsColl = initialSyncNode.getCollection(TenantMigrationTest.kConfigRecipientsNS);
assert.lte(configRecipientsColl.count(), 1);
const recipientDoc = configRecipientsColl.findOne();
if (recipientDoc) {
    switch (recipientDoc.state) {
        case TenantMigrationTest.RecipientState.kStarted:
            if (recipientDoc.dataConsistentStopDonorOpTime) {
                assert.soon(() => tenantMigrationTest
                                      .getTenantMigrationAccessBlocker(
                                          {recipientNode: initialSyncNode, tenantId: kTenantId})
                                      .recipient.state ==
                                TenantMigrationTest.RecipientAccessState.kRejectReadsAndWrites);
            }
            break;
        case TenantMigrationTest.RecipientState.kConsistent:
            if (recipientDoc.rejectReadsBeforeTimestamp) {
                assert.soon(() => tenantMigrationTest
                                      .getTenantMigrationAccessBlocker(
                                          {recipientNode: initialSyncNode, tenantId: kTenantId})
                                      .recipient.state ==
                                TenantMigrationTest.RecipientAccessState.kRejectReadsBefore);
                assert.soon(() => bsonWoCompare(
                                      tenantMigrationTest
                                          .getTenantMigrationAccessBlocker(
                                              {recipientNode: initialSyncNode, tenantId: kTenantId})
                                          .recipient.rejectBeforeTimestamp,
                                      recipientDoc.rejectReadsBeforeTimestamp) == 0);
            } else {
                assert.soon(() => tenantMigrationTest
                                      .getTenantMigrationAccessBlocker(
                                          {recipientNode: initialSyncNode, tenantId: kTenantId})
                                      .recipient.state ==
                                TenantMigrationTest.RecipientAccessState.kRejectReadsAndWrites);
            }
            break;
        default:
            throw new Error(`Invalid state "${recipientDoc.state}" from recipient doc.`);
    }
}

const activeServerlessLock = getServerlessOperationLock(initialSyncNode);
if (recipientDoc && !recipientDoc.expireAt) {
    assert.eq(activeServerlessLock, ServerlessLockType.TenantMigrationRecipient);
} else {
    assert.eq(activeServerlessLock, ServerlessLockType.None);
}

restartServerReplication(initialSyncNode);

tenantMigrationTest.stop();
