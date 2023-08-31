/**
 * Tests that tenant migration donor can peacefully shut down when there are reads being blocked due
 * to an in-progress migration.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   serverless,
 *   requires_fcv_71,
 * ]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {Thread} from "jstests/libs/parallelTester.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {getNumBlockedReads} from "jstests/replsets/libs/tenant_migration_util.js";

const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

const kTenantId = ObjectId().str;
const kDbName = kTenantId + "_testDb";
const kCollName = "testColl";

const donorRst = tenantMigrationTest.getDonorRst();
const donorPrimary = tenantMigrationTest.getDonorPrimary();
const testDb = donorPrimary.getDB(kDbName);

assert.commandWorked(testDb.runCommand({insert: kCollName, documents: [{_id: 0}]}));

const migrationId = UUID();
const migrationOpts = {
    migrationIdString: extractUUIDFromObject(migrationId),
    tenantId: kTenantId,
};

let fp = configureFailPoint(donorPrimary, "pauseTenantMigrationBeforeLeavingBlockingState");
assert.commandWorked(tenantMigrationTest.startMigration(migrationOpts));

fp.wait();
const donorDoc =
    donorPrimary.getCollection(TenantMigrationTest.kConfigDonorsNS).findOne({_id: migrationId});
assert.neq(null, donorDoc);

let readThread = new Thread((host, dbName, collName, afterClusterTime) => {
    const node = new Mongo(host);
    const db = node.getDB(dbName);
    // The primary will shut down and might throw an unhandled exception that needs to be caught by
    // our test to verify that we indeed failed with the expected network errors.
    const res = executeNoThrowNetworkError(() => db.runCommand({
        find: collName,
        readConcern: {afterClusterTime: Timestamp(afterClusterTime.t, afterClusterTime.i)}
    }));
    // In some cases (ASAN builds) we could end up closing the connection before stopping the
    // worker thread. This race condition would result in HostUnreachable instead of
    // InterruptedAtShutdown. Since the error is uncaught we need to catch it here.
    assert.commandFailedWithCode(
        res, [ErrorCodes.InterruptedAtShutdown, ErrorCodes.HostUnreachable], tojson(res.code));
}, donorPrimary.host, kDbName, kCollName, donorDoc.blockTimestamp);
readThread.start();

// Shut down the donor after the read starts blocking.
assert.soon(() => getNumBlockedReads(donorPrimary, kTenantId) == 1);
donorRst.stop(donorPrimary);
readThread.join();

donorRst.stopSet();
tenantMigrationTest.stop();
