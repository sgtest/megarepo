/**
 * Tests that for large transactions that involve multiple applyOps oplog entries, as long as the
 * donor manages to reserve oplog slots for the operations inside transaction before the migration
 * starts blocking writes, the donor will successfully write all the applyOps oplog entries to
 * commit the transaction even if the migration enters the blocking state while the applyOps oplog
 * entries are being written.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   serverless,
 * ]
 */

import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {makeTenantDB, runMigrationAsync} from "jstests/replsets/libs/tenant_migration_util.js";

load("jstests/libs/fail_point_util.js");
load("jstests/libs/parallelTester.js");
load("jstests/libs/uuid_util.js");
load('jstests/replsets/rslib.js');  // 'createRstArgs'

const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

const kTenantId = ObjectId().str;
const kDbName = makeTenantDB(kTenantId, "testDB");
const kCollName = "testColl";

const donorPrimary = tenantMigrationTest.getDonorPrimary();

/**
 * Runs a large transaction (>16MB) on the given collection name that requires two applyOps oplog
 * entries and asserts that it commits successfully.
 */
function runTransaction(primaryHost, dbName, collName) {
    /**
     * Returns a doc of size 'numMB'.
     */
    function makeLargeDoc(numMB) {
        return {x: new Array(numMB * 1024 * 1024).join('A')};
    }

    const donorPrimary = new Mongo(primaryHost);
    const session = donorPrimary.startSession();

    session.startTransaction();
    session.getDatabase(dbName)[collName].insert({doc: makeLargeDoc(10)});
    session.getDatabase(dbName)[collName].insert({doc: makeLargeDoc(5)});
    session.getDatabase(dbName)[collName].insert({doc: makeLargeDoc(5)});
    let commitRes = session.commitTransaction_forTesting();
    assert.eq(1, commitRes.ok);
    session.endSession();
}

const migrationId = UUID();
const migrationOpts = {
    migrationIdString: extractUUIDFromObject(migrationId),
    recipientConnString: tenantMigrationTest.getRecipientConnString(),
    tenantId: kTenantId,
};
const donorRstArgs = createRstArgs(tenantMigrationTest.getDonorRst());

// Start a migration, and pause it after the donor has majority-committed the initial state doc.
const dataSyncFp =
    configureFailPoint(donorPrimary, "pauseTenantMigrationBeforeLeavingDataSyncState");
const migrationThread = new Thread(runMigrationAsync, migrationOpts, donorRstArgs);
migrationThread.start();
dataSyncFp.wait();

// Run a large transaction (>16MB) that will write two applyOps oplog entries. Pause
// commitTransaction after it has reserved oplog slots for the applyOps oplog entries and has
// written the first one.
const logApplyOpsForTxnFp =
    configureFailPoint(donorPrimary, "hangAfterLoggingApplyOpsForTransaction", {}, {skip: 1});
const txnThread = new Thread(runTransaction, donorPrimary.host, kDbName, kCollName);
txnThread.start();
logApplyOpsForTxnFp.wait();

// Allow the migration to move to the blocking state and commit.
dataSyncFp.off();
assert.soon(() =>
                tenantMigrationTest
                    .getTenantMigrationAccessBlocker({donorNode: donorPrimary, tenantId: kTenantId})
                    .donor.state === TenantMigrationTest.DonorAccessState.kBlockWritesAndReads);
logApplyOpsForTxnFp.off();
TenantMigrationTest.assertCommitted(migrationThread.returnData());

// Verify that the transaction commits successfully since both applyOps have oplog timestamp <
// blockingTimestamp .
txnThread.join();

assert.commandWorked(tenantMigrationTest.forgetMigration(migrationOpts.migrationIdString));

tenantMigrationTest.stop();
