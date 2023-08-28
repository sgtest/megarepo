/**
 * Tests that tenant migration does not fail if the recipientSyncData takes a long time to return.
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
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";

const tenantMigrationTest = new TenantMigrationTest({name: jsTestName()});

const recipientPrimary = tenantMigrationTest.getRecipientPrimary();
configureFailPoint(recipientPrimary, "failCommand", {
    failInternalCommands: true,
    blockConnection: true,
    blockTimeMS: 35 * 1000,
    failCommands: ["recipientSyncData"],
});

const migrationId = UUID();
const migrationOpts = {
    migrationIdString: extractUUIDFromObject(migrationId),
    tenantId: ObjectId().str,
};

TenantMigrationTest.assertCommitted(tenantMigrationTest.runMigration(migrationOpts));
assert.commandWorked(tenantMigrationTest.forgetMigration(migrationOpts.migrationIdString));

tenantMigrationTest.stop();
