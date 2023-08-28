/**
 * Tests that the donorStartMigration and recipientSyncData commands throw an error if the provided
 * tenantId is unsupported (i.e. '', 'admin', 'local' or 'config') or if the recipient
 * connection string matches the donor's connection string or doesn't correspond to a replica set
 * with a least one host.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   incompatible_with_shard_merge,
 *   requires_persistence,
 *   requires_fcv_63,
 *   serverless,
 *   requires_fcv_71,
 * ]
 */

import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";

const tenantMigrationTest =
    new TenantMigrationTest({name: jsTestName(), enableRecipientTesting: false});

const donorPrimary = tenantMigrationTest.getDonorPrimary();
const recipientPrimary = tenantMigrationTest.getRecipientPrimary();

const tenantId = ObjectId().str;
const readPreference = {
    mode: 'primary'
};

jsTestLog("Testing 'donorStartMigration' command provided with invalid options.");

// Test missing tenantId field for protocol 'multitenant migrations'.
assert.commandFailedWithCode(donorPrimary.adminCommand({
    donorStartMigration: 1,
    migrationId: UUID(),
    recipientConnectionString: tenantMigrationTest.getRecipientRst().getURL(),
    readPreference,
}),
                             ErrorCodes.InvalidOptions);

// Test empty tenantId and unsupported database prefixes.
const unsupportedtenantIds = ['', 'admin', 'local', 'config'];
unsupportedtenantIds.forEach((invalidTenantId) => {
    assert.commandFailedWithCode(donorPrimary.adminCommand({
        donorStartMigration: 1,
        migrationId: UUID(),
        recipientConnectionString: tenantMigrationTest.getRecipientRst().getURL(),
        tenantId: invalidTenantId,
        readPreference,
    }),
                                 [ErrorCodes.InvalidOptions, ErrorCodes.BadValue]);
});

// Test migrating a tenant to the donor itself.
assert.commandFailedWithCode(donorPrimary.adminCommand({
    donorStartMigration: 1,
    migrationId: UUID(),
    recipientConnectionString: tenantMigrationTest.getDonorRst().getURL(),
    tenantId,
    readPreference,
}),
                             ErrorCodes.BadValue);

// Test migrating a tenant to a recipient that shares one or more hosts with the donor.
assert.commandFailedWithCode(donorPrimary.adminCommand({
    donorStartMigration: 1,
    migrationId: UUID(),
    recipientConnectionString:
        tenantMigrationTest.getRecipientRst().getURL() + "," + donorPrimary.host,
    tenantId,
    readPreference,
}),
                             ErrorCodes.BadValue);

// Test setting tenantIds field for protocol 'multitenant migrations'.
assert.commandFailedWithCode(donorPrimary.adminCommand({
    donorStartMigration: 1,
    migrationId: UUID(),
    recipientConnectionString:
        tenantMigrationTest.getRecipientRst().getURL() + "," + donorPrimary.host,
    tenantId,
    tenantIds: [ObjectId(), ObjectId()],
    readPreference,
}),
                             ErrorCodes.BadValue);

// Test migrating a tenant to a standalone recipient.
assert.commandFailedWithCode(donorPrimary.adminCommand({
    donorStartMigration: 1,
    migrationId: UUID(),
    recipientConnectionString: recipientPrimary.host,
    tenantId,
    readPreference,
}),
                             ErrorCodes.BadValue);

jsTestLog("Testing 'recipientSyncData' command provided with invalid options.");

// Test missing tenantId field for protocol 'multitenant migrations'.
assert.commandFailedWithCode(recipientPrimary.adminCommand({
    recipientSyncData: 1,
    migrationId: UUID(),
    donorConnectionString: tenantMigrationTest.getDonorRst().getURL(),
    startMigrationDonorTimestamp: Timestamp(1, 1),
    readPreference,
}),
                             ErrorCodes.InvalidOptions);

// Test setting tenantIds field for protocol 'multitenant migration'.
assert.commandFailedWithCode(recipientPrimary.adminCommand({
    recipientSyncData: 1,
    migrationId: UUID(),
    tenantIds: [ObjectId()],
    donorConnectionString: tenantMigrationTest.getDonorRst().getURL(),
    startMigrationDonorTimestamp: Timestamp(1, 1),
    readPreference,
}),
                             ErrorCodes.InvalidOptions);

// Test unsupported database prefixes.
unsupportedtenantIds.forEach((invalidTenantId) => {
    assert.commandFailedWithCode(recipientPrimary.adminCommand({
        recipientSyncData: 1,
        migrationId: UUID(),
        donorConnectionString: tenantMigrationTest.getDonorRst().getURL(),
        tenantId: invalidTenantId,
        startMigrationDonorTimestamp: Timestamp(1, 1),
        readPreference,
    }),
                                 [ErrorCodes.InvalidOptions, ErrorCodes.BadValue]);
});

// Test migrating a tenant from the recipient itself.
assert.commandFailedWithCode(recipientPrimary.adminCommand({
    recipientSyncData: 1,
    migrationId: UUID(),
    donorConnectionString: tenantMigrationTest.getRecipientRst().getURL(),
    tenantId: tenantId,
    startMigrationDonorTimestamp: Timestamp(1, 1),
    readPreference,
}),
                             ErrorCodes.BadValue);

// Test migrating a tenant from a donor that shares one or more hosts with the recipient.
assert.commandFailedWithCode(recipientPrimary.adminCommand({
    recipientSyncData: 1,
    migrationId: UUID(),
    donorConnectionString: `${tenantMigrationTest.getDonorRst().getURL()},${recipientPrimary.host}`,
    tenantId: tenantId,
    startMigrationDonorTimestamp: Timestamp(1, 1),
    readPreference,
}),
                             ErrorCodes.BadValue);

// Test migrating a tenant from a standalone donor.
assert.commandFailedWithCode(recipientPrimary.adminCommand({
    recipientSyncData: 1,
    migrationId: UUID(),
    donorConnectionString: recipientPrimary.host,
    tenantId: tenantId,
    startMigrationDonorTimestamp: Timestamp(1, 1),
    readPreference,
}),
                             ErrorCodes.BadValue);

// Test 'returnAfterReachingDonorTimestamp' can't be null.
const nullTimestamps = [Timestamp(0, 0), Timestamp(0, 1)];
nullTimestamps.forEach((nullTs) => {
    assert.commandFailedWithCode(donorPrimary.adminCommand({
        recipientSyncData: 1,
        migrationId: UUID(),
        donorConnectionString: tenantMigrationTest.getDonorRst().getURL(),
        tenantId: tenantId,
        startMigrationDonorTimestamp: Timestamp(1, 1),
        readPreference,
        returnAfterReachingDonorTimestamp: nullTs,
    }),
                                 ErrorCodes.BadValue);
});

// The decision field must not be set for recipientForgetMigration with multitenant migration
assert.commandFailedWithCode(recipientPrimary.adminCommand({
    recipientForgetMigration: 1,
    migrationId: UUID(),
    tenantId: ObjectId().str,
    donorConnectionString: tenantMigrationTest.getDonorRst().getURL(),
    readPreference,
    decision: "committed"
}),
                             ErrorCodes.InvalidOptions);

tenantMigrationTest.stop();
