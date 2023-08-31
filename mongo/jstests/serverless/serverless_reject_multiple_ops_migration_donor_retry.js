/**
 * @tags: [
 *   serverless,
 *   requires_fcv_71,
 *   requires_shard_merge
 * ]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";
import {TenantMigrationTest} from "jstests/replsets/libs/tenant_migration_test.js";
import {
    addRecipientNodes,
    commitSplitAsync,
    waitForGarbageCollectionForSplit
} from "jstests/serverless/libs/shard_split_test.js";

function retryMigrationAfterSplitCompletes(protocol) {
    // Test that we cannot start a migration while a shard split is in progress.
    const recipientTagName = "recipientTag";
    const recipientSetName = "recipient";
    const tenantIds = [ObjectId(), ObjectId()];
    const splitMigrationId = UUID();
    const firstTenantMigrationId = UUID();
    const secondTenantMigrationId = UUID();

    const sharedOptions = {};
    sharedOptions["setParameter"] = {shardSplitGarbageCollectionDelayMS: 0, ttlMonitorSleepSecs: 1};

    const test = new TenantMigrationTest({quickGarbageCollection: true, sharedOptions});

    const splitRst = test.getDonorRst();

    let splitRecipientNodes = addRecipientNodes({rst: splitRst, recipientTagName});

    let fp = configureFailPoint(splitRst.getPrimary(), "pauseShardSplitBeforeBlockingState");

    const commitThread = commitSplitAsync({
        rst: splitRst,
        tenantIds,
        recipientTagName,
        recipientSetName,
        migrationId: splitMigrationId
    });
    fp.wait();

    const firstMigrationOpts = {
        migrationIdString: extractUUIDFromObject(firstTenantMigrationId),
        protocol,
    };
    if (protocol != "shard merge") {
        firstMigrationOpts["tenantId"] = tenantIds[0].str;
    } else {
        firstMigrationOpts["tenantIds"] = tenantIds;
    }
    jsTestLog("Starting tenant migration");
    assert.commandFailedWithCode(test.startMigration(firstMigrationOpts),
                                 ErrorCodes.ConflictingServerlessOperation);

    fp.off();

    assert.commandWorked(commitThread.returnData());

    splitRst.nodes = splitRst.nodes.filter(node => !splitRecipientNodes.includes(node));
    splitRst.ports =
        splitRst.ports.filter(port => !splitRecipientNodes.some(node => node.port === port));

    assert.commandWorked(
        splitRst.getPrimary().adminCommand({forgetShardSplit: 1, migrationId: splitMigrationId}));

    splitRecipientNodes.forEach(node => {
        MongoRunner.stopMongod(node);
    });

    const secondMigrationOpts = {
        migrationIdString: extractUUIDFromObject(secondTenantMigrationId),
        protocol,
    };
    if (protocol != "shard merge") {
        secondMigrationOpts["tenantId"] = tenantIds[0].str;
    } else {
        secondMigrationOpts["tenantIds"] = tenantIds;
    }
    jsTestLog("Starting tenant migration");
    assert.commandWorked(test.startMigration(secondMigrationOpts));
    TenantMigrationTest.assertCommitted(test.waitForMigrationToComplete(secondMigrationOpts));
    assert.commandWorked(test.forgetMigration(secondMigrationOpts.migrationIdString));

    waitForGarbageCollectionForSplit(splitRst.nodes, splitMigrationId, tenantIds);

    test.stop();
    jsTestLog("cannotStartMigrationWhileShardSplitIsInProgress test completed");
}

retryMigrationAfterSplitCompletes("multitenant migrations");
retryMigrationAfterSplitCompletes("shard merge");
