/**
 * Tests that writes on collections clustered by _id can be rolled back.
 * @tags: [
 *   requires_replication,
 * ]
 */
import {getUUIDFromListCollections} from "jstests/libs/uuid_util.js";
import {checkRollbackFiles} from "jstests/replsets/libs/rollback_files.js";
import {RollbackTest} from "jstests/replsets/libs/rollback_test.js";

// Operations that will be present on both nodes, before the common point.
const dbName = 'test';
const collName = 'test.system.buckets.t';
const collNameShort = 'system.buckets.t';
let commonOps = (node) => {
    const db = node.getDB(dbName);
    assert.commandWorked(db.createCollection(collNameShort, {clusteredIndex: true}));
    const coll = node.getCollection(collName);
    assert.commandWorked(coll.createIndex({a: 1, b: -1}));
    assert.commandWorked(coll.insert({a: 0, b: 0}));
};

// Operations that will be performed on the rollback node past the common point.
let rollbackDocs = [];
let rollbackOps = (node) => {
    const coll = node.getCollection(collName);
    let doc;
    doc = {_id: new ObjectId(), a: 1, b: 3};
    assert.commandWorked(coll.insert(doc));
    rollbackDocs.push(doc);

    doc = {_id: new ObjectId(), a: 2, b: 2};
    assert.commandWorked(coll.insert(doc));
    rollbackDocs.push(doc);

    doc = {_id: new ObjectId(), a: 3, b: 1};
    assert.commandWorked(coll.insert(doc));
    rollbackDocs.push(doc);
};

// Set up Rollback Test.
const rollbackTest = new RollbackTest();

commonOps(rollbackTest.getPrimary());

const rollbackNode = rollbackTest.transitionToRollbackOperations();
rollbackOps(rollbackNode);

// Wait for rollback to finish.
rollbackTest.transitionToSyncSourceOperationsBeforeRollback();
rollbackTest.transitionToSyncSourceOperationsDuringRollback();
rollbackTest.transitionToSteadyStateOperations();

// Check collection count.
const primary = rollbackTest.getPrimary();
const coll = primary.getCollection(collName);
assert.eq(1, coll.find().itcount());
assert.eq(1, coll.count());

// Confirm that the rollback wrote deleted documents to a file.
const replTest = rollbackTest.getTestFixture();

const uuid = getUUIDFromListCollections(rollbackTest.getPrimary().getDB(dbName), collNameShort);
checkRollbackFiles(replTest.getDbPath(rollbackNode), collName, uuid, rollbackDocs);

rollbackTest.stop();
