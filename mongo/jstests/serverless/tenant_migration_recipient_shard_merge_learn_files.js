/**
 * Tests that recipient is able to learn files to be imported from donor for shard merge protocol.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_majority_read_concern,
 *   requires_persistence,
 *   serverless,
 *   requires_shard_merge,
 *   # The error code for a rejected recipient command invoked during the reject phase was changed.
 *   requires_fcv_71,
 * ]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {makeTenantDB} from "jstests/replsets/libs/tenant_migration_util.js";

const tenantMigrationTest =
    new TenantMigrationTest({name: jsTestName(), sharedOptions: {nodes: 3}});

const recipientPrimary = tenantMigrationTest.getRecipientPrimary();

jsTestLog(
    "Test that recipient state is correctly set to 'learned filenames' after creating the backup cursor");
const tenantId = ObjectId();
const tenantDB = makeTenantDB(tenantId.str, "DB");
const collName = "testColl";

const donorPrimary = tenantMigrationTest.getDonorPrimary();

// Do a majority write.
tenantMigrationTest.insertDonorDB(tenantDB, collName);

const failpoint = "fpBeforeMarkingCloneSuccess";
const waitInFailPoint = configureFailPoint(recipientPrimary, failpoint, {action: "hang"});

const migrationUuid = UUID();
const migrationOpts = {
    migrationIdString: extractUUIDFromObject(migrationUuid),
    readPreference: {mode: 'primary'},
    tenantIds: [tenantId],
};

jsTestLog(`Starting the tenant migration to wait in failpoint: ${failpoint}`);
assert.commandWorked(tenantMigrationTest.startMigration(migrationOpts));

waitInFailPoint.wait();

// Before transitioning to `kConsistent` state, check that all recipient nodes have
// "importDoneMarker" collection.
const importMarkerCollName = "importDoneMarker." + extractUUIDFromObject(migrationUuid);
tenantMigrationTest.getRecipientRst().nodes.forEach(node => {
    jsTestLog(`Checking if the local.${importMarkerCollName} collection exists on ${node}`);
    assert.eq(1, node.getDB("local").getCollectionInfos({name: importMarkerCollName}).length);
});

tenantMigrationTest.assertRecipientNodesInExpectedState({
    nodes: tenantMigrationTest.getRecipientRst().nodes,
    migrationId: migrationUuid,
    tenantId: tenantId.str,
    expectedState: TenantMigrationTest.ShardMergeRecipientState.kLearnedFilenames,
    expectedAccessState: TenantMigrationTest.RecipientAccessState.kRejectReadsAndWrites
});

waitInFailPoint.off();

TenantMigrationTest.assertCommitted(tenantMigrationTest.waitForMigrationToComplete(migrationOpts));

const donorPrimaryCountDocumentsResult = donorPrimary.getDB(tenantDB)[collName].countDocuments({});
const donorPrimaryCountResult = donorPrimary.getDB(tenantDB)[collName].count();

tenantMigrationTest.getRecipientRst().nodes.forEach(node => {
    jsTestLog(`Checking ${tenantDB}.${collName} on ${node}`);
    // Use "countDocuments" to check actual docs, "count" to check sizeStorer data.
    assert.eq(donorPrimaryCountDocumentsResult,
              node.getDB(tenantDB)[collName].countDocuments({}),
              "countDocuments");
    assert.eq(donorPrimaryCountResult, node.getDB(tenantDB)[collName].count(), "count");
});

tenantMigrationTest.stop();
