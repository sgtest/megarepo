/**
 * Test that a node crashes if it tries to roll back a 'commit' oplog entry using refetch-based
 * rollback. The tests mimics the standard PSA rollback setup by using a PSS replica set where the
 * last node effectively acts as an arbiter without formally being one (this is necessary because
 * we disallow the 'prepareTransaction' command in sets with arbiters).
 *
 * @tags: [
 *   uses_transactions,
 *   uses_prepare_transaction,
 * ]
 */

TestData.skipCheckDBHashes = true;

import {PrepareHelpers} from "jstests/core/txns/libs/prepare_helpers.js";
import {RollbackTest} from "jstests/replsets/libs/rollback_test.js";

const dbName = "test";
const collName = "rollback_via_refetch_commit_transaction";

// Provide RollbackTest with custom ReplSetTest so we can set forceRollbackViaRefetch.
const rst = new ReplSetTest({
    name: collName,
    nodes: 3,
    useBridge: true,
    nodeOptions: {setParameter: "forceRollbackViaRefetch=true"}
});

rst.startSet();
const config = rst.getReplSetConfig();
config.members[2].priority = 0;
config.settings = {
    chainingAllowed: false
};
rst.initiateWithHighElectionTimeout(config);

const primaryNode = rst.getPrimary();

// Create collection that exists on the sync source and rollback node.
assert.commandWorked(
    primaryNode.getDB(dbName).runCommand({create: collName, writeConcern: {w: 2}}));

// Issue a 'prepareTransaction' command just to the current primary.
const session = primaryNode.getDB(dbName).getMongo().startSession({causalConsistency: false});
const sessionDB = session.getDatabase(dbName);
const sessionColl = sessionDB.getCollection(collName);
session.startTransaction();
assert.commandWorked(sessionColl.insert({"prepare": "entry"}));
const prepareTimestamp = PrepareHelpers.prepareTransaction(session);

const rollbackTest = new RollbackTest(collName, rst);
// Stop replication from the current primary ("rollbackNode").
const rollbackNode = rollbackTest.transitionToRollbackOperations();

PrepareHelpers.commitTransaction(session, prepareTimestamp);

// Step down current primary and elect a node that lacks the commit.
rollbackTest.transitionToSyncSourceOperationsBeforeRollback();

// Verify the old primary crashes trying to roll back.
clearRawMongoProgramOutput();
rollbackTest.transitionToSyncSourceOperationsDuringRollback();
jsTestLog("Waiting for crash");
assert.soon(function() {
    try {
        rollbackNode.getDB("local").runCommand({ping: 1});
    } catch (e) {
        return true;
    }
    return false;
}, "Node did not fassert", ReplSetTest.kDefaultTimeoutMS);

// Let the ReplSetTest know the old primary is down.
rst.stop(rst.getNodeId(rollbackNode), undefined, {allowedExitCode: MongoRunner.EXIT_ABRUPT});

const msg = RegExp("Can't roll back this command yet: ");
assert.soon(function() {
    return rawMongoProgramOutput().match(msg);
}, "Node did not fail to roll back entry.");

// Transaction is still in prepared state and validation will be blocked, so skip it.
rst.stopSet(undefined, undefined, {skipValidation: true});
