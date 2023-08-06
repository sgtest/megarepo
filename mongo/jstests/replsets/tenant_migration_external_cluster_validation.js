/**
 * Verify that after a tenant migration the donor and recipient can validate each other's
 * cluster times.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   serverless,
 * ]
 */

import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {makeX509OptionsForTest} from "jstests/replsets/libs/tenant_migration_util.js";

// Multiple users cannot be authenticated on one connection within a session.
TestData.disableImplicitSessions = true;

// User that runs the tenant migration.
const kAdminUser = {
    name: "admin",
    pwd: "pwd",
};

// User that runs commands against the tenant database.
const kTestUser = {
    name: "testUser",
    pwd: "pwd",
};

function createUsers(rst) {
    const primary = rst.getPrimary();
    rst.asCluster(primary, () => {
        const adminDB = primary.getDB("admin");
        assert.commandWorked(adminDB.runCommand(
            {createUser: kAdminUser.name, pwd: kAdminUser.pwd, roles: ["root"]}));

        const testDB = primary.getDB(kDbName);
        assert.commandWorked(testDB.runCommand(
            {createUser: kTestUser.name, pwd: kTestUser.pwd, roles: ["readWrite"]}));
    });
    rst.awaitReplication();
}

const kTenantId = ObjectId().str;
const kDbName = kTenantId + "_" +
    "testDb";
const kCollName = "testColl";

const x509Options = makeX509OptionsForTest();
const donorRst = new ReplSetTest({
    nodes: 2,
    name: "donor",
    serverless: true,
    keyFile: "jstests/libs/key1",
    nodeOptions: Object.assign(x509Options.donor, {
        setParameter: {
            "failpoint.alwaysValidateClientsClusterTime": tojson({mode: "alwaysOn"}),
            // Allow non-timestamped reads on donor after migration completes for testing.
            'failpoint.tenantMigrationDonorAllowsNonTimestampedReads': tojson({mode: 'alwaysOn'}),
        }
    }),
});

const recipientRst = new ReplSetTest({
    nodes: 2,
    name: "recipient",
    serverless: true,
    keyFile: "jstests/libs/key1",
    nodeOptions: Object.assign(
        x509Options.recipient,
        {setParameter: {"failpoint.alwaysValidateClientsClusterTime": tojson({mode: "alwaysOn"})}}),
});

donorRst.startSet();
donorRst.initiate();

recipientRst.startSet();
recipientRst.initiate();

const donorPrimary = donorRst.getPrimary();
const recipientPrimary = recipientRst.getPrimary();

const donorAdminDB = donorPrimary.getDB("admin");
const recipientAdminDB = recipientPrimary.getDB("admin");

const donorPrimaryTestDB = donorPrimary.getDB(kDbName);
const recipientPrimaryTestDB = recipientPrimary.getDB(kDbName);
const donorSecondaryTestDB = donorRst.getSecondary().getDB(kDbName);
const recipientSecondaryTestDB = recipientRst.getSecondary().getDB(kDbName);

createUsers(donorRst);
createUsers(recipientRst);

assert.eq(1, donorPrimaryTestDB.auth(kTestUser.name, kTestUser.pwd));
assert.eq(1, recipientPrimaryTestDB.auth(kTestUser.name, kTestUser.pwd));

assert.eq(1, donorSecondaryTestDB.auth(kTestUser.name, kTestUser.pwd));
assert.eq(1, recipientSecondaryTestDB.auth(kTestUser.name, kTestUser.pwd));

const donorClusterTime =
    assert.commandWorked(donorPrimaryTestDB.runCommand({find: kCollName})).$clusterTime;
const recipientClusterTime =
    assert.commandWorked(recipientPrimaryTestDB.runCommand({find: kCollName})).$clusterTime;
jsTest.log("donor's clusterTime " + tojson(donorClusterTime));
jsTest.log("recipient's clusterTime " + tojson(recipientClusterTime));

jsTest.log("Verify that prior to the migration, the donor and recipient fail to validate each " +
           "other's clusterTime");

assert.commandFailedWithCode(
    donorPrimaryTestDB.runCommand({find: kCollName, $clusterTime: recipientClusterTime}),
    [ErrorCodes.TimeProofMismatch, ErrorCodes.KeyNotFound]);
assert.commandFailedWithCode(
    recipientPrimaryTestDB.runCommand({find: kCollName, $clusterTime: donorClusterTime}),
    [ErrorCodes.TimeProofMismatch, ErrorCodes.KeyNotFound]);

assert.commandFailedWithCode(
    donorSecondaryTestDB.runCommand({find: kCollName, $clusterTime: recipientClusterTime}),
    [ErrorCodes.TimeProofMismatch, ErrorCodes.KeyNotFound]);
assert.commandFailedWithCode(
    recipientSecondaryTestDB.runCommand({find: kCollName, $clusterTime: donorClusterTime}),
    [ErrorCodes.TimeProofMismatch, ErrorCodes.KeyNotFound]);

donorPrimaryTestDB.logout();
recipientPrimaryTestDB.logout();

donorSecondaryTestDB.logout();
recipientSecondaryTestDB.logout();

assert.eq(1, donorAdminDB.auth(kAdminUser.name, kAdminUser.pwd));
assert.eq(1, recipientAdminDB.auth(kAdminUser.name, kAdminUser.pwd));

const tenantMigrationTest = new TenantMigrationTest({name: jsTestName(), donorRst, recipientRst});
const migrationId = UUID();
const migrationOpts = {
    migrationIdString: extractUUIDFromObject(migrationId),
    tenantId: kTenantId,
};
TenantMigrationTest.assertCommitted(tenantMigrationTest.runMigration(migrationOpts));

donorAdminDB.logout();
recipientAdminDB.logout();

jsTest.log("Verify that after the migration, the donor and recipient can validate each other's" +
           " clusterTime");

assert.eq(1, donorPrimaryTestDB.auth(kTestUser.name, kTestUser.pwd));
assert.eq(1, recipientPrimaryTestDB.auth(kTestUser.name, kTestUser.pwd));

assert.eq(1, donorSecondaryTestDB.auth(kTestUser.name, kTestUser.pwd));
assert.eq(1, recipientSecondaryTestDB.auth(kTestUser.name, kTestUser.pwd));

assert.commandWorked(
    donorPrimaryTestDB.runCommand({find: kCollName, $clusterTime: recipientClusterTime}));
assert.commandWorked(
    recipientPrimaryTestDB.runCommand({find: kCollName, $clusterTime: donorClusterTime}));

assert.commandWorked(
    donorSecondaryTestDB.runCommand({find: kCollName, $clusterTime: recipientClusterTime}));
assert.commandWorked(
    recipientSecondaryTestDB.runCommand({find: kCollName, $clusterTime: donorClusterTime}));

donorPrimaryTestDB.logout();
recipientPrimaryTestDB.logout();

donorSecondaryTestDB.logout();
recipientSecondaryTestDB.logout();

tenantMigrationTest.stop();
donorRst.stopSet();
recipientRst.stopSet();
