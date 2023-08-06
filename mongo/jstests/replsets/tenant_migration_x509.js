/**
 * Tests that the tenant migration donor authenticates as client to recipient using the
 * migration-specific x.509 certificate, and vice versa.
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
import {getCertificateAndPrivateKey} from "jstests/replsets/libs/tenant_migration_util.js";

function makeTestNs(tenantId) {
    return {dbName: tenantId + "_testDb", collName: "testColl"};
}

function setup() {
    const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});
    return {
        tenantMigrationTest,
        teardown: function() {
            tenantMigrationTest.stop();
        }
    };
}

const kDonorCertificateAndPrivateKey =
    getCertificateAndPrivateKey("jstests/libs/tenant_migration_donor.pem");
const kRecipientCertificateAndPrivateKey =
    getCertificateAndPrivateKey("jstests/libs/tenant_migration_recipient.pem");

(() => {
    jsTest.log("Test valid donor and recipient certificates");
    const {tenantMigrationTest, teardown} = setup();
    const migrationId = UUID();
    const tenantId = ObjectId().str;
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: tenantId,
        donorCertificateForRecipient: kDonorCertificateAndPrivateKey,
        recipientCertificateForDonor: kRecipientCertificateAndPrivateKey,
    };
    const {dbName, collName} = makeTestNs(tenantId);

    tenantMigrationTest.insertDonorDB(dbName, collName);
    TenantMigrationTest.assertCommitted(tenantMigrationTest.runMigration(migrationOpts));
    tenantMigrationTest.verifyRecipientDB(
        tenantId, dbName, collName, true /* migrationCommitted */);
    teardown();
})();

(() => {
    jsTest.log("Test invalid donor certificate, no header and trailer");
    const {tenantMigrationTest, teardown} = setup();
    const migrationId = UUID();
    const tenantId = ObjectId().str;
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: tenantId,
        donorCertificateForRecipient: {
            certificate: "invalidCertificate",
            privateKey: kDonorCertificateAndPrivateKey.privateKey
        },
        recipientCertificateForDonor: kRecipientCertificateAndPrivateKey,
    };
    const {dbName, collName} = makeTestNs(tenantId);

    tenantMigrationTest.insertDonorDB(dbName, collName);
    assert.commandFailedWithCode(tenantMigrationTest.runMigration(migrationOpts),
                                 ErrorCodes.InvalidSSLConfiguration);
    tenantMigrationTest.verifyRecipientDB(
        tenantId, dbName, collName, false /* migrationCommitted */);
    teardown();
})();

(() => {
    jsTest.log("Test invalid donor certificate, use private key as certificate");
    const {tenantMigrationTest, teardown} = setup();
    const migrationId = UUID();
    const tenantId = ObjectId().str;
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: tenantId,
        donorCertificateForRecipient: {
            certificate: kDonorCertificateAndPrivateKey.privateKey,
            privateKey: kDonorCertificateAndPrivateKey.privateKey
        },
        recipientCertificateForDonor: kRecipientCertificateAndPrivateKey,
    };
    const {dbName, collName} = makeTestNs(tenantId);

    tenantMigrationTest.insertDonorDB(dbName, collName);
    assert.commandFailedWithCode(tenantMigrationTest.runMigration(migrationOpts),
                                 ErrorCodes.InvalidSSLConfiguration);
    tenantMigrationTest.verifyRecipientDB(
        tenantId, dbName, collName, false /* migrationCommitted */);
    teardown();
})();

(() => {
    jsTest.log("Test invalid donor private key, no header and trailer");
    const {tenantMigrationTest, teardown} = setup();
    const migrationId = UUID();
    const tenantId = ObjectId().str;
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: tenantId,
        donorCertificateForRecipient: {
            certificate: kDonorCertificateAndPrivateKey.certificate,
            privateKey: "invalidCertificate"
        },
        recipientCertificateForDonor: kRecipientCertificateAndPrivateKey,
    };
    const {dbName, collName} = makeTestNs(tenantId);

    tenantMigrationTest.insertDonorDB(dbName, collName);
    assert.commandFailedWithCode(tenantMigrationTest.runMigration(migrationOpts),
                                 ErrorCodes.InvalidSSLConfiguration);
    tenantMigrationTest.verifyRecipientDB(
        tenantId, dbName, collName, false /* migrationCommitted */);
    teardown();
})();

(() => {
    jsTest.log("Test invalid donor private key, use certificate as private key");
    const {tenantMigrationTest, teardown} = setup();
    const migrationId = UUID();
    const tenantId = ObjectId().str;
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: tenantId,
        donorCertificateForRecipient: {
            certificate: kDonorCertificateAndPrivateKey.certificate,
            privateKey: kDonorCertificateAndPrivateKey.certificate
        },
        recipientCertificateForDonor: kRecipientCertificateAndPrivateKey,
    };
    const {dbName, collName} = makeTestNs(tenantId);

    tenantMigrationTest.insertDonorDB(dbName, collName);
    assert.commandFailedWithCode(tenantMigrationTest.runMigration(migrationOpts),
                                 ErrorCodes.InvalidSSLConfiguration);
    tenantMigrationTest.verifyRecipientDB(
        tenantId, dbName, collName, false /* migrationCommitted */);
    teardown();
})();

(() => {
    jsTest.log("Test invalid donor certificate and private key pair");
    const {tenantMigrationTest, teardown} = setup();
    const migrationId = UUID();
    const tenantId = ObjectId().str;
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: tenantId,
        donorCertificateForRecipient: {
            certificate: kDonorCertificateAndPrivateKey.certificate,
            privateKey: kRecipientCertificateAndPrivateKey.privateKey
        },
        recipientCertificateForDonor: kRecipientCertificateAndPrivateKey,
    };
    const {dbName, collName} = makeTestNs(tenantId);

    tenantMigrationTest.insertDonorDB(dbName, collName);
    assert.commandFailedWithCode(tenantMigrationTest.runMigration(migrationOpts),
                                 ErrorCodes.InvalidSSLConfiguration);
    tenantMigrationTest.verifyRecipientDB(
        tenantId, dbName, collName, false /* migrationCommitted */);
    teardown();
})();

(() => {
    jsTest.log("Test expired donor certificate and key");
    const {tenantMigrationTest, teardown} = setup();
    const migrationId = UUID();
    const tenantId = ObjectId().str;
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: tenantId,
        donorCertificateForRecipient:
            getCertificateAndPrivateKey("jstests/libs/tenant_migration_donor_expired.pem"),
        recipientCertificateForDonor: kRecipientCertificateAndPrivateKey,
    };
    const {dbName, collName} = makeTestNs(tenantId);

    tenantMigrationTest.insertDonorDB(dbName, collName);
    assert.commandFailedWithCode(tenantMigrationTest.runMigration(migrationOpts),
                                 ErrorCodes.InvalidSSLConfiguration);
    tenantMigrationTest.verifyRecipientDB(
        tenantId, dbName, collName, false /* migrationCommitted */);
    teardown();
})();

(() => {
    jsTest.log("Test invalid recipient certificate, no header and trailer");
    const {tenantMigrationTest, teardown} = setup();
    const migrationId = UUID();
    const tenantId = ObjectId().str;
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: tenantId,
        donorCertificateForRecipient: kDonorCertificateAndPrivateKey,
        recipientCertificateForDonor: {
            certificate: "invalidCertificate",
            privateKey: kRecipientCertificateAndPrivateKey.privateKey
        },
    };
    const {dbName, collName} = makeTestNs(tenantId);

    tenantMigrationTest.insertDonorDB(dbName, collName);
    assert.commandFailedWithCode(tenantMigrationTest.runMigration(migrationOpts),
                                 ErrorCodes.InvalidSSLConfiguration);
    tenantMigrationTest.verifyRecipientDB(
        tenantId, dbName, collName, false /* migrationCommitted */);
    teardown();
})();

(() => {
    jsTest.log("Test invalid recipient certificate, use private key as certificate");
    const {tenantMigrationTest, teardown} = setup();
    const migrationId = UUID();
    const tenantId = ObjectId().str;
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: tenantId,
        donorCertificateForRecipient: kDonorCertificateAndPrivateKey,
        recipientCertificateForDonor: {
            certificate: kRecipientCertificateAndPrivateKey.privateKey,
            privateKey: kRecipientCertificateAndPrivateKey.privateKey
        },
    };
    const {dbName, collName} = makeTestNs(tenantId);

    tenantMigrationTest.insertDonorDB(dbName, collName);
    assert.commandFailedWithCode(tenantMigrationTest.runMigration(migrationOpts),
                                 ErrorCodes.InvalidSSLConfiguration);
    tenantMigrationTest.verifyRecipientDB(
        tenantId, dbName, collName, false /* migrationCommitted */);
    teardown();
})();

(() => {
    jsTest.log("Test invalid recipient private key, no header and trailer");
    const {tenantMigrationTest, teardown} = setup();
    const migrationId = UUID();
    const tenantId = ObjectId().str;
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: tenantId,
        donorCertificateForRecipient: kDonorCertificateAndPrivateKey,
        recipientCertificateForDonor: {
            certificate: kRecipientCertificateAndPrivateKey.certificate,
            privateKey: "invalidCertificate"
        },
    };
    const {dbName, collName} = makeTestNs(tenantId);

    tenantMigrationTest.insertDonorDB(dbName, collName);
    assert.commandFailedWithCode(tenantMigrationTest.runMigration(migrationOpts),
                                 ErrorCodes.InvalidSSLConfiguration);
    tenantMigrationTest.verifyRecipientDB(
        tenantId, dbName, collName, false /* migrationCommitted */);
    teardown();
})();

(() => {
    jsTest.log("Test invalid recipient private key, use certificate as private key");
    const {tenantMigrationTest, teardown} = setup();
    const migrationId = UUID();
    const tenantId = ObjectId().str;
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: tenantId,
        donorCertificateForRecipient: kDonorCertificateAndPrivateKey,
        recipientCertificateForDonor: {
            certificate: kRecipientCertificateAndPrivateKey.certificate,
            privateKey: kRecipientCertificateAndPrivateKey.certificate
        },
    };
    const {dbName, collName} = makeTestNs(tenantId);

    tenantMigrationTest.insertDonorDB(dbName, collName);
    assert.commandFailedWithCode(tenantMigrationTest.runMigration(migrationOpts),
                                 ErrorCodes.InvalidSSLConfiguration);
    tenantMigrationTest.verifyRecipientDB(
        tenantId, dbName, collName, false /* migrationCommitted */);
    teardown();
})();

(() => {
    jsTest.log("Test expired recipient certificate and key");
    const {tenantMigrationTest, teardown} = setup();
    const migrationId = UUID();
    const tenantId = ObjectId().str;
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: tenantId,
        donorCertificateForRecipient: kDonorCertificateAndPrivateKey,
        recipientCertificateForDonor:
            getCertificateAndPrivateKey("jstests/libs/tenant_migration_recipient_expired.pem"),
    };
    const {dbName, collName} = makeTestNs(tenantId);

    tenantMigrationTest.insertDonorDB(dbName, collName);
    TenantMigrationTest.assertAborted(tenantMigrationTest.runMigration(migrationOpts),
                                      ErrorCodes.InvalidSSLConfiguration);
    tenantMigrationTest.verifyRecipientDB(
        tenantId, dbName, collName, false /* migrationCommitted */);
    teardown();
})();

(() => {
    jsTest.log("Test invalid recipient certificate and private key pair");
    const {tenantMigrationTest, teardown} = setup();
    const migrationId = UUID();
    const tenantId = ObjectId().str;
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: tenantId,
        donorCertificateForRecipient: kDonorCertificateAndPrivateKey,
        recipientCertificateForDonor: {
            certificate: kRecipientCertificateAndPrivateKey.certificate,
            privateKey: kDonorCertificateAndPrivateKey.privateKey
        }
    };
    const {dbName, collName} = makeTestNs(tenantId);

    tenantMigrationTest.insertDonorDB(dbName, collName);
    TenantMigrationTest.assertAborted(tenantMigrationTest.runMigration(migrationOpts),
                                      ErrorCodes.InvalidSSLConfiguration);
    tenantMigrationTest.verifyRecipientDB(
        tenantId, dbName, collName, false /* migrationCommitted */);
    teardown();
})();

if (!TestData.auth) {
    jsTestLog("Skipping testing authorization since auth is not enabled");
    quit();
}

(() => {
    jsTest.log("Test donor certificate without the required privileges");
    const {tenantMigrationTest, teardown} = setup();
    const migrationId = UUID();
    const tenantId = ObjectId().str;
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: tenantId,
        donorCertificateForRecipient: getCertificateAndPrivateKey(
            "jstests/libs/tenant_migration_donor_insufficient_privileges.pem"),
        recipientCertificateForDonor: kRecipientCertificateAndPrivateKey,
    };
    const {dbName, collName} = makeTestNs(tenantId);

    tenantMigrationTest.insertDonorDB(dbName, collName);
    TenantMigrationTest.assertAborted(tenantMigrationTest.runMigration(migrationOpts),
                                      ErrorCodes.Unauthorized);
    tenantMigrationTest.verifyRecipientDB(
        tenantId, dbName, collName, false /* migrationCommitted */);
    teardown();
})();

(() => {
    const {tenantMigrationTest, teardown} = setup();
    jsTest.log("Test recipient certificate without the required privileges");
    const migrationId = UUID();
    const tenantId = ObjectId().str;
    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(migrationId),
        tenantId: tenantId,
        donorCertificateForRecipient: kDonorCertificateAndPrivateKey,
        recipientCertificateForDonor: getCertificateAndPrivateKey(
            "jstests/libs/tenant_migration_recipient_insufficient_privileges.pem"),
    };
    const {dbName, collName} = makeTestNs(tenantId);

    tenantMigrationTest.insertDonorDB(dbName, collName);
    TenantMigrationTest.assertAborted(tenantMigrationTest.runMigration(migrationOpts),
                                      ErrorCodes.Unauthorized);
    tenantMigrationTest.verifyRecipientDB(
        tenantId, dbName, collName, false /* migrationCommitted */);
    teardown();
})();
