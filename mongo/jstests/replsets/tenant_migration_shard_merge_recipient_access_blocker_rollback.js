/**
 * Tests if the recipient is rolled back well after a migration has been committed, the shard
 * merge recipient access blocker is initialized in the correct state.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   featureFlagShardMerge,
 *   serverless,
 * ]
 */

import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {
    getCertificateAndPrivateKey,
    isShardMergeEnabled,
    kProtocolShardMerge,
    makeTenantDB,
    makeX509OptionsForTest
} from "jstests/replsets/libs/tenant_migration_util.js";

load("jstests/libs/uuid_util.js");           // For extractUUIDFromObject().
load("jstests/libs/fail_point_util.js");     // For configureFailPoint().
load("jstests/libs/write_concern_util.js");  // for 'stopReplicationOnSecondaries'
load("jstests/libs/parallelTester.js");      // For Thread()

const migrationX509Options = makeX509OptionsForTest();

const recipientRst = new ReplSetTest({
    name: "recipRst",
    nodes: 3,
    serverless: true,
    nodeOptions: Object.assign(migrationX509Options.recipient, {}),
    settings: {catchUpTimeoutMillis: 0, chainingAllowed: false}
});

recipientRst.startSet();
recipientRst.initiate();

// Note: including this explicit early return here due to the fact that multiversion
// suites will execute this test without featureFlagShardMerge enabled (despite the
// presence of the featureFlagShardMerge tag above), which means the test will attempt
// to run a multi-tenant migration and fail.
if (!isShardMergeEnabled(recipientRst.getPrimary().getDB("admin"))) {
    recipientRst.stopSet();
    jsTestLog("Skipping Shard Merge-specific test");
    quit();
}

// This test case
// 1) Completes and commits a tenant migration. Then forgets the migration (state doc marked with
//    'expireAt', but not yet deleted.)
// 2) Waits until the replica set is stable.
// 3) Rolls back the primary. This makes the primary recover its tenant migration access blockers.
// 4) Ensures that a read is possible from the primary.
function runRollbackAfterMigrationCommitted() {
    jsTestLog("Testing a rollback after the migration has been committed and marked forgotten.");
    const tenantMigrationTest = new TenantMigrationTest(
        {name: jsTestName(), recipientRst: recipientRst, sharedOptions: {nodes: 1}});

    const kMigrationId = UUID();
    const kTenantId = ObjectId();
    const kReadPreference = {mode: "primary"};
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(kMigrationId),
        tenantIds: [kTenantId],
        protocol: kProtocolShardMerge,
        readPreference: kReadPreference
    };

    // Populate the donor side with data.
    const dbName = makeTenantDB(kTenantId.str, "testDB");
    const collName = "testColl";
    const numDocs = 20;
    tenantMigrationTest.insertDonorDB(
        dbName,
        collName,
        [...Array(numDocs).keys()].map((i) => ({a: i, band: "Air", song: "La Femme d'Argent"})));

    jsTestLog(`Starting tenant migration with migrationId ${kMigrationId}`);
    assert.commandWorked(tenantMigrationTest.startMigration(migrationOpts));

    // Complete and commit the migration, and then forget it as well.
    jsTestLog("Waiting for migration to complete and commit.");
    TenantMigrationTest.assertCommitted(
        tenantMigrationTest.waitForMigrationToComplete(migrationOpts));
    assert.commandWorked(tenantMigrationTest.forgetMigration(migrationOpts.migrationIdString));
    recipientRst.awaitReplication();

    // It should be possible to read from the recipient now.
    jsTestLog("Reading from the recipient primary on the tenant collection.");
    const originalPrimary = recipientRst.getPrimary();
    assert.eq(numDocs, originalPrimary.getDB(dbName)[collName].find().itcount());

    jsTestLog("Halting replication on the secondaries.");
    const secondaries = recipientRst.getSecondaries();
    stopServerReplication(secondaries);

    // Prepare the recipient primary for rollback, by inserting non-tenant related data on it while
    // replication has been halted.
    jsTestLog("Inserting random data on recipient primary.");
    const randomColl = originalPrimary.getDB("randomDB")["random_coll"];
    assert.commandWorked(randomColl.insert({x: "The Real Folk Blues"}, {writeConcern: {w: 1}}));

    // Stepping up one of the secondaries should cause the original primary to rollback.
    jsTestLog("Stepping up one of the secondaries.");
    const newRecipientPrimary = secondaries[0];
    recipientRst.stepUp(newRecipientPrimary, {awaitReplicationBeforeStepUp: false});

    jsTestLog("Restarting server replication.");
    restartServerReplication(secondaries);
    recipientRst.awaitReplication();

    jsTestLog("Stepping up the original primary back to primary.");
    recipientRst.stepUp(originalPrimary, {awaitReplicationBeforeStepUp: false});

    jsTestLog("Perform a read against the original primary on the tenant collection.");
    assert.eq(numDocs, originalPrimary.getDB(dbName)[collName].find().itcount());

    tenantMigrationTest.stop();
}

// This test case:
// 1) Sets the replica set up such that the migration has already been committed and forgotten, and
//    the state doc has been deleted as well.
// 2) Sends a 'recipientForgetMigration' command to the recipient primary, and waits for the state
//    doc to persist.
// 3) Performs a rollback on the recipient primary, so that the access blockers are reconstructed.
// 4) Performs a read on the recipient primary.
function runRollbackAfterLoneRecipientForgetMigrationCommand() {
    jsTestLog("Testing a rollback after migration has been committed and completely forgotten.");
    const tenantMigrationTest = new TenantMigrationTest(
        {name: jsTestName(), recipientRst: recipientRst, sharedOptions: {nodes: 1}});

    const kMigrationId = UUID();
    const kTenantId = ObjectId();
    const kReadPreference = {mode: "primary"};
    const recipientCertificateForDonor =
        getCertificateAndPrivateKey("jstests/libs/tenant_migration_recipient.pem");

    const dbName = makeTenantDB(kTenantId.str, "testDB");
    const collName = "testColl";

    const originalPrimary = recipientRst.getPrimary();
    const newPrimary = recipientRst.getSecondaries()[0];

    // We will have the tenant database already on the recipient, as though the tenant migration has
    // already run to completion, and the state document has been cleaned up already.
    assert.commandWorked(originalPrimary.getDB(dbName)[collName].insert(
        {x: "Composer", y: "Mysore Vasudevacharya"}));
    recipientRst.awaitReplication();

    // Prevent the "expireAt" field from being populated.
    const fpOriginalPrimary = configureFailPoint(originalPrimary, "hangBeforeTaskCompletion");
    // Prevent the new primary from marking the state document as garbage collectable.
    const fpNewPrimary =
        configureFailPoint(newPrimary, "pauseBeforeRunTenantMigrationRecipientInstance");

    function runRecipientForgetMigration(host,
                                         {
                                             migrationIdString,
                                             donorConnectionString,
                                             tenantIds,
                                             readPreference,
                                             recipientCertificateForDonor
                                         },
                                         protocol) {
        const db = new Mongo(host);
        return db.adminCommand({
            recipientForgetMigration: 1,
            migrationId: UUID(migrationIdString),
            donorConnectionString,
            tenantIds: eval(tenantIds),
            protocol,
            decision: "committed",
            readPreference,
            recipientCertificateForDonor
        });
    }

    const recipientForgetMigrationThread =
        new Thread(runRecipientForgetMigration,
                   originalPrimary.host,
                   {
                       migrationIdString: extractUUIDFromObject(kMigrationId),
                       donorConnectionString: tenantMigrationTest.getDonorRst().getURL(),
                       tenantIds: tojson([kTenantId]),
                       readPreference: kReadPreference,
                       recipientCertificateForDonor
                   },
                   kProtocolShardMerge);

    // Run a delayed/retried recipientForgetMigration command after the state doc has been deleted.
    recipientForgetMigrationThread.start();

    jsTestLog("Wait until the right before the state document's 'expireAt' is set.");
    fpOriginalPrimary.wait();
    recipientRst.awaitReplication();

    // It should be possible to read from the recipient now.
    assert.eq(1, originalPrimary.getDB(dbName)[collName].find().itcount());

    // Now perform a rollback on the recipient primary.
    jsTestLog("Halting replication on the secondaries.");
    const secondaries = recipientRst.getSecondaries();
    stopServerReplication(secondaries);

    jsTestLog("Inserting random data on recipient primary.");
    const randomColl = originalPrimary.getDB("randomDB")["random_coll"];
    assert.commandWorked(randomColl.insert({x: "Que Sera Sera"}, {writeConcern: {w: 1}}));

    // Stepping up one of the secondaries should cause the original primary to rollback.
    jsTestLog("Stepping up one of the secondaries.");
    recipientRst.stepUp(newPrimary, {awaitReplicationBeforeStepUp: false});

    assert.commandFailedWithCode(recipientForgetMigrationThread.returnData(),
                                 ErrorCodes.InterruptedDueToReplStateChange);

    // It should be possible to read from new recipient primary.
    assert.eq(1, newPrimary.getDB(dbName)[collName].find().itcount());

    jsTestLog("Restarting server replication.");
    restartServerReplication(secondaries);
    recipientRst.awaitReplication();

    jsTestLog("Stepping up the original primary back to primary.");
    const fpOriginalPrimaryBeforeStarting =
        configureFailPoint(originalPrimary, "pauseBeforeRunTenantMigrationRecipientInstance");
    fpOriginalPrimary.off();
    recipientRst.stepUp(originalPrimary, {awaitReplicationBeforeStepUp: false});

    jsTestLog("Perform another read against the original primary on the tenant collection.");
    assert.eq(1, originalPrimary.getDB(dbName)[collName].find().itcount());

    fpOriginalPrimaryBeforeStarting.off();
    fpNewPrimary.off();

    tenantMigrationTest.stop();
}

runRollbackAfterMigrationCommitted();
runRollbackAfterLoneRecipientForgetMigrationCommand();

recipientRst.stopSet();
