/**
 * Tests that migration certificates do not show up in the logs.
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

import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";

function assertNoCertificateOrPrivateKeyLogsForCmd(conn, cmdName) {
    assert(checkLog.checkContainsOnce(conn, new RegExp(`Slow query.*${cmdName}`)),
           "did not find slow query logs for the command");
    assert(!checkLog.checkContainsOnce(conn, /BEGIN CERTIFICATE.*END CERTIFICATE/),
           "found certificate in the logs");
    assert(!checkLog.checkContainsOnce(conn, /BEGIN PRIVATE KEY.*END PRIVATE KEY/),
           "found private key in the logs");
}

const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

const donorPrimary = tenantMigrationTest.getDonorPrimary();
const recipientPrimary = tenantMigrationTest.getRecipientPrimary();

// Verify that migration certificates are not logged as part of slow query logging.
(() => {
    const donorDefaultSlowMs =
        assert.commandWorked(donorPrimary.adminCommand({profile: 0, slowms: 0})).slowms;
    const recipientDefaultSlowMs =
        assert.commandWorked(recipientPrimary.adminCommand({profile: 0, slowms: 0})).slowms;

    const migrationOpts = {
        migrationIdString: extractUUIDFromObject(UUID()),
        tenantId: ObjectId().str,
    };

    TenantMigrationTest.assertCommitted(
        tenantMigrationTest.runMigration(migrationOpts, {automaticForgetMigration: false}));

    assertNoCertificateOrPrivateKeyLogsForCmd(donorPrimary, "donorStartMigration");
    assertNoCertificateOrPrivateKeyLogsForCmd(recipientPrimary, "recipientSyncData");

    assert.commandWorked(tenantMigrationTest.forgetMigration(migrationOpts.migrationIdString));

    assertNoCertificateOrPrivateKeyLogsForCmd(donorPrimary, "donorForgetMigration");
    assertNoCertificateOrPrivateKeyLogsForCmd(recipientPrimary, "recipientForgetMigration");

    assert.commandWorked(donorPrimary.adminCommand({profile: 0, slowms: donorDefaultSlowMs}));
    assert.commandWorked(
        recipientPrimary.adminCommand({profile: 0, slowms: recipientDefaultSlowMs}));
})();

tenantMigrationTest.stop();
