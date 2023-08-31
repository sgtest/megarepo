/**
 * Tests that the migration recipient will retrieve committed transactions on the donor
 * with lastWriteOpTime <= the stored startApplyingOpTime. The recipient should store
 * these committed transaction entries in its own 'config.transactions' collection.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   serverless,
 *   requires_fcv_71,
 *   requires_shard_merge,
 * ]
 */

import {PrepareHelpers} from "jstests/core/txns/libs/prepare_helpers.js";
import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {makeTenantDB} from "jstests/replsets/libs/tenant_migration_util.js";

const tenantId = ObjectId().str;
const otherTenantId = ObjectId().str;
const transactionsNS = "config.transactions";
const collName = "testColl";

const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});
const tenantDB = makeTenantDB(tenantId, "testDB");
const otherTenantDB = makeTenantDB(otherTenantId, "testDB");
const tenantNS = `${tenantDB}.${collName}`;

const donorPrimary = tenantMigrationTest.getDonorPrimary();
const recipientPrimary = tenantMigrationTest.getRecipientPrimary();

function validateTransactionEntryonRecipient(sessionId) {
    const donorTxnEntry =
        donorPrimary.getCollection(transactionsNS).findOne({"_id.id": sessionId.id});
    const recipientTxnEntry =
        recipientPrimary.getCollection(transactionsNS).findOne({"_id.id": sessionId.id});

    assert.eq(donorTxnEntry.txnNum, recipientTxnEntry.txnNum);
    assert.eq(donorTxnEntry.state, recipientTxnEntry.state);

    // The recipient should have replaced the 'lastWriteOpTime' and 'lastWriteDate' fields.
    assert.neq(donorTxnEntry.lastWriteOpTime, recipientTxnEntry.lastWriteOpTime);
    assert.neq(donorTxnEntry.lastWriteDate, recipientTxnEntry.lastWriteDate);

    // Test that the client can retry the first 'commitTransaction' on the recipient.
    assert.commandWorked(recipientPrimary.adminCommand({
        commitTransaction: 1,
        lsid: recipientTxnEntry._id,
        txnNumber: recipientTxnEntry.txnNum,
        autocommit: false,
    }));
}

assert.commandWorked(donorPrimary.getCollection(tenantNS).insert([{_id: 0, x: 0}, {_id: 1, x: 1}],
                                                                 {writeConcern: {w: "majority"}}));

let sessionIdBeforeMigration;
{
    jsTestLog("Run and commit a transaction prior to the migration");
    const session = donorPrimary.startSession({causalConsistency: false});
    sessionIdBeforeMigration = session.getSessionId();
    const sessionDb = session.getDatabase(tenantDB);
    const sessionColl = sessionDb.getCollection(collName);

    session.startTransaction({writeConcern: {w: "majority"}});
    const findAndModifyRes0 = sessionColl.findAndModify({query: {x: 0}, remove: true});
    assert.eq({_id: 0, x: 0}, findAndModifyRes0);
    assert.commandWorked(session.commitTransaction_forTesting());
    assert.sameMembers(sessionColl.find({}).toArray(), [{_id: 1, x: 1}]);
    session.endSession();
}

assert.eq(1, donorPrimary.getCollection(transactionsNS).find().itcount());

{
    jsTestLog("Run and abort a transaction prior to the migration");
    const session = donorPrimary.startSession({causalConsistency: false});
    const sessionDb = session.getDatabase(tenantDB);
    const sessionColl = sessionDb.getCollection(collName);

    session.startTransaction({writeConcern: {w: "majority"}});
    const findAndModifyRes0 = sessionColl.findAndModify({query: {x: 1}, remove: true});
    assert.eq({_id: 1, x: 1}, findAndModifyRes0);

    // We prepare the transaction so that 'abortTransaction' will update the transactions table.
    // We should later see that the recipient will not update its transactions table with this
    // entry, since we only fetch committed transactions.
    PrepareHelpers.prepareTransaction(session);

    assert.commandWorked(session.abortTransaction_forTesting());
    assert.sameMembers(sessionColl.find({}).toArray(), [{_id: 1, x: 1}]);
    session.endSession();
}

assert.eq(2, donorPrimary.getCollection(transactionsNS).find().itcount());

let sessionIdForOtherTenant;
{
    jsTestLog("Run and commit a transaction for a different tenant");
    const session = donorPrimary.startSession({causalConsistency: false});
    sessionIdForOtherTenant = session.getSessionId();
    const sessionDb = session.getDatabase(otherTenantDB);
    const sessionColl = sessionDb.getCollection(collName);

    session.startTransaction({writeConcern: {w: "majority"}});
    assert.commandWorked(sessionColl.insert([{_id: 0, x: 0}, {_id: 1, x: 1}]));
    assert.commandWorked(session.commitTransaction_forTesting());
    session.endSession();
}

assert.eq(3, donorPrimary.getCollection(transactionsNS).find().itcount());

jsTestLog("Running a migration");

const migrationId = UUID();
const migrationOpts = {
    migrationIdString: extractUUIDFromObject(migrationId),
    tenantIds: [ObjectId(tenantId), ObjectId(otherTenantId)],
};

const fpAfterFetchingCommittedTransactions =
    configureFailPoint(recipientPrimary, "fpAfterFetchingCommittedTransactions", {action: "hang"});

assert.commandWorked(tenantMigrationTest.startMigration(migrationOpts));

fpAfterFetchingCommittedTransactions.wait();

// Verify that the recipient has fetched and written all committed transaction entries
// from the donor.
assert.eq(2, recipientPrimary.getCollection(transactionsNS).find().itcount());

fpAfterFetchingCommittedTransactions.off();

TenantMigrationTest.assertCommitted(tenantMigrationTest.waitForMigrationToComplete(migrationOpts));

validateTransactionEntryonRecipient(sessionIdBeforeMigration);
validateTransactionEntryonRecipient(sessionIdForOtherTenant);

tenantMigrationTest.stop();
