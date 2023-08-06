/**
 * Test that the shard merge is resilient to WriteConflict exception thrown while importing
 * collections. We will get WriteConflict exception if we try to import the files with timestamp
 * older than the stable timestamp.
 *
 * @tags: [
 *   does_not_support_encrypted_storage_engine,
 *   featureFlagShardMerge,
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_replication,
 *   requires_persistence,
 *   requires_wiredtiger,
 *   serverless,
 * ]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {isShardMergeEnabled} from "jstests/replsets/libs/tenant_migration_util.js";

const migrationId = UUID();
const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});
const donorPrimary = tenantMigrationTest.getDonorPrimary();
const recipientPrimary = tenantMigrationTest.getRecipientPrimary();

if (!isShardMergeEnabled(recipientPrimary.getDB("admin"))) {
    tenantMigrationTest.stop();
    jsTestLog("Skipping Shard Merge-specific test");
    quit();
}

const tenantId = ObjectId();
const dbName = `${tenantId.str}_myDatabase`;

jsTestLog("Generate test data");

const db = donorPrimary.getDB(dbName);
const collection = db["myCollection"];
const capped = db["myCappedCollection"];
assert.commandWorked(db.createCollection("myCappedCollection", {capped: true, size: 100}));
for (let c of [collection, capped]) {
    c.insertMany([{_id: 0}, {_id: 1}, {_id: 2}], {writeConcern: {w: "majority"}});
}

assert.commandWorked(db.runCommand({
    createIndexes: "myCollection",
    indexes: [{key: {a: 1}, name: "a_1"}],
    writeConcern: {w: "majority"}
}));

// Enable Failpoints to simulate WriteConflict exception while importing donor files.
configureFailPoint(
    recipientPrimary, "WTWriteConflictExceptionForImportCollection", {} /* data */, {times: 1});
configureFailPoint(
    recipientPrimary, "WTWriteConflictExceptionForImportIndex", {} /* data */, {times: 1});

jsTestLog("Run migration");
// The old multitenant migrations won't copy myDatabase since it doesn't start with testTenantId,
// but shard merge copies everything so we still expect myDatabase on the recipient, below.
const migrationOpts = {
    migrationIdString: extractUUIDFromObject(migrationId),
    tenantIds: [tenantId]
};
TenantMigrationTest.assertCommitted(tenantMigrationTest.runMigration(migrationOpts));

tenantMigrationTest.getRecipientRst().nodes.forEach(node => {
    for (let collectionName of ["myCollection", "myCappedCollection"]) {
        jsTestLog(`Checking ${dbName}.${collectionName} on ${node}`);
        // Use "countDocuments" to check actual docs, "count" to check sizeStorer data.
        assert.eq(donorPrimary.getDB(dbName)[collectionName].countDocuments({}),
                  node.getDB(dbName)[collectionName].countDocuments({}),
                  "countDocuments");
        assert.eq(donorPrimary.getDB(dbName)[collectionName].count(),
                  node.getDB(dbName)[collectionName].count(),
                  "count");
    }
});

tenantMigrationTest.stop();
