/**
 * Test that multiple concurrent tenant migrations are supported.
 *
 * Incompatible with shard merge, which can't handle concurrent migrations.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_shard_merge,
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
import {makeX509Options} from "jstests/replsets/libs/tenant_migration_util.js";

const x509Options0 = makeX509Options("jstests/libs/rs0.pem");
const x509Options1 = makeX509Options("jstests/libs/rs1.pem");
const x509Options2 = makeX509Options("jstests/libs/rs2.pem");

const rst0 = new ReplSetTest({nodes: 1, name: 'rst0', serverless: true, nodeOptions: x509Options0});
const rst1 = new ReplSetTest({nodes: 1, name: 'rst1', serverless: true, nodeOptions: x509Options1});
const rst2 = new ReplSetTest({nodes: 1, name: 'rst2', serverless: true, nodeOptions: x509Options2});

rst0.startSet();
rst0.initiate();

rst1.startSet();
rst1.initiate();

rst2.startSet();
rst2.initiate();

// Test concurrent outgoing migrations to different recipients.
(() => {
    const tenantMigrationTest0 = new TenantMigrationTest({donorRst: rst0, recipientRst: rst1});
    const tenantMigrationTest1 = new TenantMigrationTest({donorRst: rst0, recipientRst: rst2});
    const tenantId0 = ObjectId().str;
    const tenantId1 = ObjectId().str;
    const donorPrimary = rst0.getPrimary();
    const connPoolStatsBefore = assert.commandWorked(donorPrimary.adminCommand({connPoolStats: 1}));

    const migrationOpts0 = {
        migrationIdString: extractUUIDFromObject(UUID()),
        tenantId: tenantId0,
    };
    const migrationOpts1 = {
        migrationIdString: extractUUIDFromObject(UUID()),
        tenantId: tenantId1,
    };

    assert.commandWorked(tenantMigrationTest0.startMigration(migrationOpts0));
    assert.commandWorked(tenantMigrationTest1.startMigration(migrationOpts1));

    // Wait for both migration to finish and verify they succeeded.
    TenantMigrationTest.assertCommitted(
        tenantMigrationTest0.waitForMigrationToComplete(migrationOpts0));
    TenantMigrationTest.assertCommitted(
        tenantMigrationTest1.waitForMigrationToComplete(migrationOpts1));

    const connPoolStatsAfter0 = assert.commandWorked(donorPrimary.adminCommand({connPoolStats: 1}));
    // Donor targeted two different replica sets.
    assert.eq(connPoolStatsAfter0.numReplicaSetMonitorsCreated,
              connPoolStatsBefore.numReplicaSetMonitorsCreated + 2);
    assert.eq(Object.keys(connPoolStatsAfter0.replicaSets).length, 2);

    assert.commandWorked(tenantMigrationTest0.forgetMigration(migrationOpts0.migrationIdString));
    assert.commandWorked(tenantMigrationTest1.forgetMigration(migrationOpts1.migrationIdString));

    // After migrations are complete, RSMs are garbage collected.
    const connPoolStatsAfter1 = assert.commandWorked(donorPrimary.adminCommand({connPoolStats: 1}));
    assert.eq(Object.keys(connPoolStatsAfter1.replicaSets).length, 0);

    assert.eq(Object
                  .keys(assert.commandWorked(rst1.getPrimary().adminCommand({connPoolStats: 1}))
                            .replicaSets)
                  .length,
              0);
})();

// Test concurrent incoming migrations from different donors.
(() => {
    const tenantMigrationTest0 = new TenantMigrationTest({donorRst: rst0, recipientRst: rst2});
    const tenantMigrationTest1 = new TenantMigrationTest({donorRst: rst1, recipientRst: rst2});
    const tenantId0 = ObjectId().str;
    const tenantId1 = ObjectId().str;

    const migrationOpts0 = {
        migrationIdString: extractUUIDFromObject(UUID()),
        tenantId: tenantId0,
    };
    const migrationOpts1 = {
        migrationIdString: extractUUIDFromObject(UUID()),
        tenantId: tenantId1,
    };

    assert.commandWorked(tenantMigrationTest0.startMigration(migrationOpts0));
    assert.commandWorked(tenantMigrationTest1.startMigration(migrationOpts1));

    // Wait for both migration to finish and verify they succeeded.
    TenantMigrationTest.assertCommitted(
        tenantMigrationTest0.waitForMigrationToComplete(migrationOpts0));
    TenantMigrationTest.assertCommitted(
        tenantMigrationTest1.waitForMigrationToComplete(migrationOpts1));

    // Cleanup.
    assert.commandWorked(tenantMigrationTest0.forgetMigration(migrationOpts0.migrationIdString));
    assert.commandWorked(tenantMigrationTest1.forgetMigration(migrationOpts1.migrationIdString));

    const connPoolStatsAfter0 =
        assert.commandWorked(rst0.getPrimary().adminCommand({connPoolStats: 1}));
    assert.eq(Object.keys(connPoolStatsAfter0.replicaSets).length, 0);

    const connPoolStatsAfter1 =
        assert.commandWorked(rst1.getPrimary().adminCommand({connPoolStats: 1}));
    assert.eq(Object.keys(connPoolStatsAfter1.replicaSets).length, 0);
})();

// Test concurrent outgoing migrations to same recipient. Verify that tenant
// migration donor only removes a ReplicaSetMonitor for a recipient when the last
// migration to that recipient completes.
(() => {
    const tenantMigrationTest0 = new TenantMigrationTest({donorRst: rst0, recipientRst: rst1});
    const tenantMigrationTest1 = new TenantMigrationTest({donorRst: rst0, recipientRst: rst1});

    const tenantId0 = ObjectId().str;
    const tenantId1 = ObjectId().str;

    const donorsColl = tenantMigrationTest0.getDonorRst().getPrimary().getCollection(
        TenantMigrationTest.kConfigDonorsNS);

    const migrationOpts0 = {
        migrationIdString: extractUUIDFromObject(UUID()),
        tenantId: tenantId0,
    };
    const migrationOpts1 = {
        migrationIdString: extractUUIDFromObject(UUID()),
        tenantId: tenantId1,
    };

    const donorPrimary = rst0.getPrimary();

    const connPoolStatsBefore = assert.commandWorked(donorPrimary.adminCommand({connPoolStats: 1}));

    const blockFp = configureFailPoint(
        donorPrimary, "pauseTenantMigrationBeforeLeavingBlockingState", {tenantId: tenantId1});
    assert.commandWorked(tenantMigrationTest0.startMigration(migrationOpts0));
    assert.commandWorked(tenantMigrationTest1.startMigration(migrationOpts1));

    // Wait migration1 to pause in the blocking state and for migration0 to commit.
    blockFp.wait();
    TenantMigrationTest.assertCommitted(
        tenantMigrationTest0.waitForMigrationToComplete(migrationOpts0));

    // Verify that exactly one RSM was created.
    const connPoolStatsDuringMigration =
        assert.commandWorked(donorPrimary.adminCommand({connPoolStats: 1}));
    assert.eq(connPoolStatsDuringMigration.numReplicaSetMonitorsCreated,
              connPoolStatsBefore.numReplicaSetMonitorsCreated + 1);
    assert.eq(Object.keys(connPoolStatsDuringMigration.replicaSets).length, 1);

    // Garbage collect migration0 and verify that the RSM was not removed.
    assert.commandWorked(tenantMigrationTest0.forgetMigration(migrationOpts0.migrationIdString));
    assert.eq(
        Object.keys(assert.commandWorked(donorPrimary.adminCommand({connPoolStats: 1})).replicaSets)
            .length,
        1);

    // Let the migration1 to finish.
    blockFp.off();
    TenantMigrationTest.assertCommitted(
        tenantMigrationTest1.waitForMigrationToComplete(migrationOpts1));

    // Verify that now the RSM is garbage collected after the migration1 is cleaned.
    assert.commandWorked(tenantMigrationTest1.forgetMigration(migrationOpts1.migrationIdString));

    assert.eq(
        Object.keys(assert.commandWorked(donorPrimary.adminCommand({connPoolStats: 1})).replicaSets)
            .length,
        0);
})();

rst0.stopSet();
rst1.stopSet();
rst2.stopSet();
