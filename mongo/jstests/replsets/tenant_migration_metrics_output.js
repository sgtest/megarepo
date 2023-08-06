/**
 * Verifies the serverStatus output and FTDC output for tenant migrations.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   serverless,
 * ]
 */

import {verifyGetDiagnosticData} from "jstests/libs/ftdc.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {
    makeX509OptionsForTest,
} from "jstests/replsets/libs/tenant_migration_util.js";

// Verify that the server status response has the fields that we expect.
function verifyServerStatus(conn) {
    const res = assert.commandWorked(conn.adminCommand({serverStatus: 1}));
    assert.hasFields(res, ["tenantMigrationAccessBlocker"]);
}
// Verify the periodic samples used for FTDC do not have the 'tenantMigrationAccessBlocker' section.
function verifyFTDCOutput(conn) {
    const latestPeriodicFTDC = verifyGetDiagnosticData(conn.getDB("admin"));
    assert.hasFields(latestPeriodicFTDC, ["serverStatus"]);
    assert.eq(undefined, latestPeriodicFTDC.serverStatus.tenantMigrationAccessBlocker);
}

jsTestLog("Verify serverStatus and FTDC output after a migration has committed.");

const testPath = MongoRunner.toRealPath("ftdc_dir_repl_node");
const donorRst = new ReplSetTest({
    nodes: 1,
    name: "donorRst",
    serverless: true,
    nodeOptions: Object.assign(makeX509OptionsForTest().donor,
                               {setParameter: {diagnosticDataCollectionDirectoryPath: testPath}})
});

donorRst.startSet();
donorRst.initiate();

const tenantMigrationTest =
    new TenantMigrationTest({name: jsTestName(), donorRst, enableRecipientTesting: false});

const tenantId = ObjectId().str;
const migrationId = extractUUIDFromObject(UUID());
const migrationOpts = {
    migrationIdString: migrationId,
    tenantId: tenantId,
    recipientConnString: tenantMigrationTest.getRecipientConnString()
};

TenantMigrationTest.assertCommitted(tenantMigrationTest.runMigration(migrationOpts));

verifyServerStatus(tenantMigrationTest.getDonorPrimary());
verifyFTDCOutput(tenantMigrationTest.getDonorPrimary());

tenantMigrationTest.stop();
donorRst.stopSet();
