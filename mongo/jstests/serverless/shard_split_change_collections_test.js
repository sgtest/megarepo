/**
 * Tests that a shard split handles change collections.
 * @tags: [requires_fcv_63, serverless]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {
    ChangeStreamMultitenantReplicaSetTest
} from "jstests/serverless/libs/change_collection_util.js";
import {assertMigrationState, ShardSplitTest} from "jstests/serverless/libs/shard_split_test.js";

const tenantIds = [ObjectId(), ObjectId()];
const donorRst = new ChangeStreamMultitenantReplicaSetTest({
    nodes: 3,
    nodeOptions: {
        setParameter: {
            shardSplitGarbageCollectionDelayMS: 0,
            ttlMonitorSleepSecs: 1,
            shardSplitTimeoutMS: 100000
        }
    }
});

const test = new ShardSplitTest({quickGarbageCollection: true, donorRst});
test.addRecipientNodes();
test.donor.awaitSecondaryNodes();

const donorPrimary = test.getDonorPrimary();
const donorTenantConn =
    ChangeStreamMultitenantReplicaSetTest.getTenantConnection(donorPrimary.host, tenantIds[0]);
test.donor.setChangeStreamState(donorTenantConn, true);

const donorNonMovingTenantConn =
    ChangeStreamMultitenantReplicaSetTest.getTenantConnection(donorPrimary.host, ObjectId());
test.donor.setChangeStreamState(donorNonMovingTenantConn, true);
const donorNonMovingCursor = donorNonMovingTenantConn.getDB("database").collection.watch();

// Open a change stream and insert documents into database.collection before the split
// starts.
const donorCursor = donorTenantConn.getDB("database").collection.watch([]);
const insertedDocs = [{_id: "tenant1_1"}, {_id: "tenant1_2"}, {_id: "tenant1_3"}];
donorTenantConn.getDB("database").collection.insertMany(insertedDocs);

// Start up a cursor to check if we can getMore after the tenant has been migrated and change
// collection is dropped.
const donorCursor2 = donorTenantConn.getDB("database").collection.watch([]);

const donorTenantSession = donorTenantConn.startSession({retryWrites: true});
const donorTenantSessionCollection = donorTenantSession.getDatabase("database").collection;
assert.commandWorked(donorTenantSessionCollection.insert({_id: "tenant1_4", w: "RETRYABLE"}));
assert.commandWorked(donorTenantSession.getDatabase("database").runCommand({
    findAndModify: "collection",
    query: {_id: "tenant1_4"},
    update: {$set: {updated: true}}
}));

// Start a transaction and perform some writes.
const donorTxnSession = donorTenantConn.getDB("database").getMongo().startSession();
donorTxnSession.startTransaction();
donorTxnSession.getDatabase("database").collection.insertOne({_id: "tenant1_in_transaction_1"});
donorTxnSession.getDatabase("database").collection.updateOne({_id: "tenant1_in_transaction_1"}, {
    $set: {updated: true}
});
donorTxnSession.commitTransaction();
donorTxnSession.endSession();

// Get the first entry from the change stream cursor and grab the resume token.
assert.eq(donorCursor.hasNext(), true);
const {_id: resumeToken} = donorCursor.next();

// Set this break point so that we can run commands against the primary when the split operation
// enters a blocking state.
const blockingFp = configureFailPoint(donorPrimary, "pauseShardSplitAfterBlocking");
const operation = test.createSplitOperation(tenantIds);
const splitThread = operation.commitAsync();

// Wait for the split to enter the blocking state.
blockingFp.wait();

assert.commandFailedWithCode(
    donorTenantConn.getDB("database").runCommand({
        aggregate: "collection",
        cursor: {},
        pipeline: [{$changeStream: {}}],
        // Timeout set higher than 1000ms to make sure its actually blocked and not just waiting for
        // inserts, since change streams are awaitdata cursors.
        maxTimeMS: 2 * 1000
    }),
    ErrorCodes.MaxTimeMSExpired,
    "Opening new change streams should block while a split operation is in a blocking state");

blockingFp.off();
splitThread.join();
assert.commandWorked(splitThread.returnData());
assertMigrationState(donorPrimary, operation.migrationId, "committed");

// Test that we cannot open a new change stream after the tenant has been migrated.
assert.commandFailedWithCode(
    donorTenantConn.getDB("database")
        .runCommand({aggregate: "collection", cursor: {}, pipeline: [{$changeStream: {}}]}),
    ErrorCodes.TenantMigrationCommitted,
    "Opening a change stream on the donor after completion of a shard split should fail.");

// Test change stream cursor behavior on the donor for a tenant which was migrated, and for one
// which remains on the donor.
assert.commandWorked(
    donorNonMovingTenantConn.getDB("database")
        .runCommand("getMore", {getMore: donorNonMovingCursor._cursorid, collection: "collection"}),
    "Tailing a change stream for a tenant that wasn't moved by a split" +
        "should not be blocked after the split was committed");

// Test that running a getMore on a change stream cursor after the migration commits throws a
// resumable change stream exception.
const failedGetMore = donorTenantConn.getDB("database").runCommand("getMore", {
    getMore: donorCursor._cursorid,
    collection: "collection"
});
assert.commandFailedWithCode(
    failedGetMore,
    ErrorCodes.ResumeTenantChangeStream,
    "Tailing a change stream on the donor after completion of a shard split should fail.");
assert(failedGetMore.hasOwnProperty("errorLabels"));
assert.contains("ResumableChangeStreamError", failedGetMore.errorLabels);

// The cursor should have been deleted after the error so a getMore should fail.
assert.commandFailedWithCode(
    donorTenantConn.getDB("database")
        .runCommand("getMore", {getMore: donorCursor._cursorid, collection: "collection"}),
    ErrorCodes.CursorNotFound);

operation.forget();

const recipientRst = test.getRecipient();
const recipientPrimary = recipientRst.getPrimary();

const recipientPrimaryTenantConn = ChangeStreamMultitenantReplicaSetTest.getTenantConnection(
    recipientPrimary.host, tenantIds[0], tenantIds[0].str);

// Running ChangeStreamMultitenantReplicaSetTest.getTenantConnection will create a user on the
// primary. Await replication so that we can use the same user on secondaries.
recipientRst.awaitReplication();

const recipientSecondaryConns = recipientRst.getSecondaries().map(
    node => ChangeStreamMultitenantReplicaSetTest.getTenantConnection(
        node.host, tenantIds[0], tenantIds[0].str));

// Resume the change stream on all Recipient nodes.
const cursors = [recipientPrimaryTenantConn, ...recipientSecondaryConns].map(
    conn => conn.getDB("database").collection.watch([], {resumeAfter: resumeToken}));

[{_id: "tenant1_2", operationType: "insert"},
 {_id: "tenant1_3", operationType: "insert"},
 {_id: "tenant1_4", operationType: "insert"},
 {_id: "tenant1_4", operationType: "update"},
 {_id: "tenant1_in_transaction_1", operationType: "insert"},
 {_id: "tenant1_in_transaction_1", operationType: "update"},
].forEach(expectedEvent => {
    cursors.forEach(cursor => {
        assert.soon(() => cursor.hasNext());
        const changeEvent = cursor.next();
        assert.eq(changeEvent.documentKey._id, expectedEvent._id);
        assert.eq(changeEvent.operationType, expectedEvent.operationType);
    });
});

test.cleanupSuccesfulCommitted(operation.migrationId, tenantIds);

// getMore cursor to check if we can getMore after the database is dropped.
donorTenantSession.getDatabase("config")["system.change_collection"].drop();
assert.commandFailedWithCode(
    donorTenantConn.getDB("database")
        .runCommand("getMore", {getMore: donorCursor2._cursorid, collection: "collection"}),
    ErrorCodes.QueryPlanKilled);

test.stop();
