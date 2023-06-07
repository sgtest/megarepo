/**
 * Ensures that the TTLMonitor does not remove the cached index information from the
 * TTLCollectionCache object for a newly created index before the implicitly created collection is
 * registered and visible in the CollectionCatalog.
 * Removing this cached index information prevents the TTLMonitor from removing expired documents
 * for that collection.
 */
(function() {
'use strict';
load("jstests/libs/ttl_util.js");

const conn = MongoRunner.runMongod({setParameter: 'ttlMonitorSleepSecs=1'});

const dbName = "test";
const collName = "ttlMonitor";

const db = conn.getDB(dbName);
const coll = db.getCollection(collName);

TestData.dbName = dbName;
TestData.collName = collName;

coll.drop();

const failPoint = "hangTTLCollectionCacheAfterRegisteringInfo";
assert.commandWorked(db.adminCommand({configureFailPoint: failPoint, mode: "alwaysOn"}));

// Create an index on a non-existent collection. This will implicitly create the collection.
let awaitcreateIndex = startParallelShell(() => {
    const testDB = db.getSiblingDB(TestData.dbName);
    assert.commandWorked(
        testDB.getCollection(TestData.collName).createIndex({x: 1}, {expireAfterSeconds: 0}));
}, db.getMongo().port);

// Wait for the TTL monitor to run and register the index in the TTL collection cache.
checkLog.containsJson(db.getMongo(), 4664000);

// Let the TTL monitor run once. It should not remove the index from the cached TTL information
// until the collection is committed.
TTLUtil.waitForPass(coll.getDB());

// Finish the index build.
assert.commandWorked(db.adminCommand({configureFailPoint: failPoint, mode: "off"}));
awaitcreateIndex();

// Insert documents, which should expire immediately and be removed on the next TTL pass.
const now = new Date();
for (let i = 0; i < 10; i++) {
    assert.commandWorked(coll.insert({x: now}));
}

// Let the TTL monitor run once to remove the expired documents.
TTLUtil.waitForPass(coll.getDB());

assert.eq(0, coll.find({}).count());

MongoRunner.stopMongod(conn);
}());
