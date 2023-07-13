/**
 * Test the dbCheck command.
 *
 * @tags: [
 *   # We need persistence as we temporarily restart nodes as standalones.
 *   requires_persistence,
 *   assumes_against_mongod_not_mongos,
 *   # snapshotRead:false behavior has been removed in 6.2
 *   requires_fcv_62,
 * ]
 */

(function() {
"use strict";

// This test injects inconsistencies between replica set members; do not fail because of expected
// dbHash differences.
TestData.skipCheckDBHashes = true;

let replSet = new ReplSetTest({name: "dbCheckSet", nodes: 2});

replSet.startSet();
replSet.initiate();
replSet.awaitSecondaryNodes();

function forEachSecondary(f) {
    for (let secondary of replSet.getSecondaries()) {
        f(secondary);
    }
}

function forEachNode(f) {
    f(replSet.getPrimary());
    forEachSecondary(f);
}

let dbName = "dbCheck-test";
let collName = "dbcheck-collection";

// Clear local.system.healthlog.
function clearLog() {
    forEachNode(conn => conn.getDB("local").system.healthlog.drop());
}

// Name for a collection which takes multiple batches to check and which shouldn't be modified
// by any of the tests.
const multiBatchSimpleCollName = "dbcheck-simple-collection";
const multiBatchSimpleCollSize = 10000;
replSet.getPrimary().getDB(dbName)[multiBatchSimpleCollName].insertMany(
    [...Array(10000).keys()].map(x => ({_id: x})), {ordered: false});

function dbCheckCompleted(db) {
    return db.currentOp().inprog.filter(x => x["desc"] == "dbCheck")[0] === undefined;
}

// Wait for dbCheck to complete (on both primaries and secondaries).  Fails an assertion if
// dbCheck takes longer than maxMs.
function awaitDbCheckCompletion(db, collName, maxKey, maxSize, maxCount) {
    let start = Date.now();

    assert.soon(() => dbCheckCompleted(db), "dbCheck timed out");
    replSet.awaitSecondaryNodes();
    replSet.awaitReplication();

    forEachNode(function(node) {
        const healthlog = node.getDB('local').system.healthlog;
        assert.soon(function() {
            return (healthlog.find({"operation": "dbCheckStop"}).itcount() == 1);
        }, "dbCheck command didn't complete");
    });
}

// Check that everything in the health log shows a successful and complete check with no found
// inconsistencies.
function checkLogAllConsistent(conn) {
    let healthlog = conn.getDB("local").system.healthlog;

    const debugBuild = conn.getDB('admin').adminCommand('buildInfo').debug;

    if (debugBuild) {
        // These tests only run on debug builds because they rely on dbCheck health-logging
        // all info-level batch results.
        assert(healthlog.find().count(), "dbCheck put no batches in health log");

        let maxResult = healthlog.aggregate([
            {$match: {operation: "dbCheckBatch"}},
            {$group: {_id: 1, key: {$max: "$data.maxKey"}}}
        ]);

        assert(maxResult.hasNext(), "dbCheck put no batches in health log");
        assert.eq(maxResult.next().key, {"$maxKey": 1}, "dbCheck batches should end at MaxKey");

        let minResult = healthlog.aggregate([
            {$match: {operation: "dbCheckBatch"}},
            {$group: {_id: 1, key: {$min: "$data.minKey"}}}
        ]);

        assert(minResult.hasNext(), "dbCheck put no batches in health log");
        assert.eq(minResult.next().key, {"$minKey": 1}, "dbCheck batches should start at MinKey");
    }
    // Assert no errors (i.e., found inconsistencies).
    let errs = healthlog.find({"severity": {"$ne": "info"}});
    if (errs.hasNext()) {
        assert(false, "dbCheck found inconsistency: " + tojson(errs.next()));
    }

    // Assert no failures (i.e., checks that failed to complete).
    let failedChecks = healthlog.find({"operation": "dbCheckBatch", "data.success": false});
    if (failedChecks.hasNext()) {
        assert(false, "dbCheck batch failed: " + tojson(failedChecks.next()));
    }

    if (debugBuild) {
        // These tests only run on debug builds because they rely on dbCheck health-logging
        // all info-level batch results.

        // Finds an entry with data.minKey === MinKey, and then matches its maxKey against
        // another document's minKey, and so on, and then checks that the result of that search
        // has data.maxKey === MaxKey.
        let completeCoverage = healthlog.aggregate([
                {$match: {"operation": "dbCheckBatch", "data.minKey": MinKey}},
                {
                $graphLookup: {
                    from: "system.healthlog",
                    startWith: "$data.minKey",
                    connectToField: "data.minKey",
                    connectFromField: "data.maxKey",
                    as: "batchLimits",
                    restrictSearchWithMatch: {"operation": "dbCheckBatch"}
                }
                },
                {$match: {"batchLimits.data.maxKey": MaxKey}}
            ]);
        assert(completeCoverage.hasNext(), "dbCheck batches do not cover full key range");
    }
}

// Check that the total of all batches in the health log on `conn` is equal to the total number
// of documents and bytes in `coll`.

// Returns a document with fields "totalDocs" and "totalBytes", representing the total size of
// the batches in the health log.
function healthLogCounts(healthlog) {
    // These tests only run on debug builds because they rely on dbCheck health-logging
    // all info-level batch results.
    const debugBuild = healthlog.getDB().getSiblingDB('admin').adminCommand('buildInfo').debug;
    if (!debugBuild) {
        return;
    }
    let result = healthlog.aggregate([
        {$match: {"operation": "dbCheckBatch"}},
        {
            $group: {
                "_id": null,
                "totalDocs": {$sum: "$data.count"},
                "totalBytes": {$sum: "$data.bytes"}
            }
        }
    ]);

    assert(result.hasNext(), "dbCheck put no batches in health log");

    return result.next();
}

function checkTotalCounts(conn, coll) {
    // These tests only run on debug builds because they rely on dbCheck health-logging
    // all info-level batch results.
    const debugBuild = conn.getDB('admin').adminCommand('buildInfo').debug;
    if (!debugBuild) {
        return;
    }
    let result = healthLogCounts(conn.getDB("local").system.healthlog);

    assert.eq(result.totalDocs, coll.count(), "dbCheck batches do not count all documents");

    // Calculate the size on the client side, because collection.dataSize is not necessarily the
    // sum of the document sizes.
    let size = coll.find().toArray().reduce((x, y) => x + bsonsize(y), 0);

    assert.eq(result.totalBytes, size, "dbCheck batches do not count all bytes");
}

// First check behavior when everything is consistent.
function simpleTestConsistent() {
    let primary = replSet.getPrimary();
    clearLog();

    assert.neq(primary, undefined);
    let db = primary.getDB(dbName);
    assert.commandWorked(db.runCommand({"dbCheck": multiBatchSimpleCollName}));

    awaitDbCheckCompletion(db, multiBatchSimpleCollName);

    checkLogAllConsistent(primary);
    checkTotalCounts(primary, db[multiBatchSimpleCollName]);

    forEachSecondary(function(secondary) {
        checkLogAllConsistent(secondary);
        checkTotalCounts(secondary, secondary.getDB(dbName)[multiBatchSimpleCollName]);
    });
}

function simpleTestNonSnapshot() {
    let primary = replSet.getPrimary();
    clearLog();

    assert.neq(primary, undefined);
    let db = primary.getDB(dbName);
    // "dbCheck no longer supports snapshotRead:false"
    assert.commandFailedWithCode(
        db.runCommand({"dbCheck": multiBatchSimpleCollName, snapshotRead: false}), 6769500);
    // "dbCheck no longer supports snapshotRead:false"
    assert.commandFailedWithCode(db.runCommand({"dbCheck": 1, snapshotRead: false}), 6769501);
}

// Same thing, but now with concurrent updates.
function concurrentTestConsistent() {
    let primary = replSet.getPrimary();
    clearLog();

    let db = primary.getDB(dbName);

    // Add enough documents that dbCheck will take a few seconds.
    db[collName].insertMany([...Array(10000).keys()].map(x => ({i: x})), {ordered: false});

    assert.commandWorked(db.runCommand({"dbCheck": collName}));

    let coll = db[collName];

    while (db.currentOp().inprog.filter(x => x["desc"] === "dbCheck").length) {
        coll.updateOne({}, {"$inc": {"i": 10}});
        coll.insertOne({"i": 42});
        coll.deleteOne({});
    }

    awaitDbCheckCompletion(db, collName);

    checkLogAllConsistent(primary);
    // Omit check for total counts, which might have changed with concurrent updates.

    forEachSecondary(secondary => checkLogAllConsistent(secondary, true));
}

simpleTestConsistent();
simpleTestNonSnapshot();
concurrentTestConsistent();

// Test the various other parameters.
function testDbCheckParameters() {
    let primary = replSet.getPrimary();
    let db = primary.getDB(dbName);

    // Clean up for the test.
    clearLog();

    let docSize = bsonsize({_id: 10});

    function checkEntryBounds(start, end) {
        forEachNode(function(node) {
            // These tests only run on debug builds because they rely on dbCheck health-logging
            // all info-level batch results.
            const debugBuild = node.getDB('admin').adminCommand('buildInfo').debug;
            if (!debugBuild) {
                return;
            }
            let healthlog = node.getDB("local").system.healthlog;
            let keyBoundsResult = healthlog.aggregate([
                {$match: {operation: "dbCheckBatch"}},
                {
                    $group:
                        {_id: null, minKey: {$min: "$data.minKey"}, maxKey: {$max: "$data.maxKey"}}
                }
            ]);

            assert(keyBoundsResult.hasNext(), "dbCheck put no batches in health log");

            const bounds = keyBoundsResult.next();
            const counts = healthLogCounts(healthlog);
            assert.eq(bounds.minKey, start, "dbCheck minKey field incorrect");

            // dbCheck evaluates some exit conditions like maxCount and maxBytes at batch boundary.
            // The batch boundary isn't generally deterministic (e.g. can be time-dependent per
            // maxBatchTimeMillis) hence the greater-than-or-equal comparisons.
            assert.gte(bounds.maxKey, end, "dbCheck maxKey field incorrect");
            assert.gte(counts.totalDocs, end - start);
            assert.gte(counts.totalBytes, (end - start) * docSize);
        });
    }

    // Run a dbCheck on just a subset of the documents
    let start = 1000;
    let end = 9000;

    assert.commandWorked(
        db.runCommand({dbCheck: multiBatchSimpleCollName, minKey: start, maxKey: end}));

    awaitDbCheckCompletion(db, multiBatchSimpleCollName, end);

    checkEntryBounds(start, end);

    // Now, clear the health logs again,
    clearLog();

    let maxCount = 5000;

    // and do the same with a count constraint.
    assert.commandWorked(db.runCommand(
        {dbCheck: multiBatchSimpleCollName, minKey: start, maxKey: end, maxCount: maxCount}));

    // We expect it to reach the count limit before reaching maxKey.
    awaitDbCheckCompletion(db, multiBatchSimpleCollName, undefined, undefined, maxCount);
    checkEntryBounds(start, start + maxCount);

    // Finally, do the same with a size constraint.
    clearLog();
    let maxSize = maxCount * docSize;
    assert.commandWorked(db.runCommand(
        {dbCheck: multiBatchSimpleCollName, minKey: start, maxKey: end, maxSize: maxSize}));
    awaitDbCheckCompletion(db, multiBatchSimpleCollName, end, maxSize);
    checkEntryBounds(start, start + maxCount);

    // The remaining tests only run on debug builds because they rely on dbCheck health-logging
    // all info-level batch results.

    const debugBuild = primary.getDB('admin').adminCommand('buildInfo').debug;
    if (!debugBuild) {
        return;
    }

    const healthlog = db.getSiblingDB('local').system.healthlog;
    {
        // Validate custom maxDocsPerBatch
        clearLog();
        const maxDocsPerBatch = 100;
        assert.commandWorked(
            db.runCommand({dbCheck: multiBatchSimpleCollName, maxDocsPerBatch: maxDocsPerBatch}));

        const healthlog = db.getSiblingDB('local').system.healthlog;
        assert.soon(function() {
            const expectedBatches = multiBatchSimpleCollSize / maxDocsPerBatch +
                (multiBatchSimpleCollSize % maxDocsPerBatch ? 1 : 0);
            return (healthlog.find({"operation": "dbCheckBatch"}).itcount() == expectedBatches);
        }, "dbCheck doesn't seem to complete", 60 * 1000);

        assert.eq(
            db.getSiblingDB('local')
                .system.healthlog.find({"operation": "dbCheckBatch", "data.count": maxDocsPerBatch})
                .itcount(),
            multiBatchSimpleCollSize / maxDocsPerBatch);
    }
    {
        // Validate custom maxBytesPerBatch
        clearLog();
        const coll = db.getSiblingDB("maxBytesPerBatch").maxBytesPerBatch;

        // Insert nDocs, each of which being slightly larger than 1MB, and then run dbCheck with
        // maxBytesPerBatch := 1MB
        const nDocs = 5;
        coll.insertMany([...Array(nDocs).keys()].map(x => ({a: 'a'.repeat(1024 * 1024)})),
                        {ordered: false});
        const maxBytesPerBatch = 1024 * 1024;
        assert.commandWorked(db.getSiblingDB("maxBytesPerBatch").runCommand({
            dbCheck: coll.getName(),
            maxBytesPerBatch: maxBytesPerBatch
        }));

        // Confirm dbCheck logs nDocs batches.
        assert.soon(function() {
            return (healthlog.find({"operation": "dbCheckBatch"}).itcount() == nDocs);
        }, "dbCheck doesn't seem to complete", 60 * 1000);

        assert.eq(db.getSiblingDB('local')
                      .system.healthlog.find({"operation": "dbCheckBatch", "data.count": 1})
                      .itcount(),
                  nDocs);
    }
}

testDbCheckParameters();

// Now, test some unusual cases where the command should fail.
function testErrorOnNonexistent() {
    let primary = replSet.getPrimary();
    let db = primary.getDB("this-probably-doesnt-exist");
    assert.commandFailed(db.runCommand({dbCheck: 1}),
                         "dbCheck spuriously succeeded on nonexistent database");
    db = primary.getDB(dbName);
    assert.commandFailed(db.runCommand({dbCheck: "this-also-probably-doesnt-exist"}),
                         "dbCheck spuriously succeeded on nonexistent collection");
}

function testErrorOnSecondary() {
    let secondary = replSet.getSecondary();
    let db = secondary.getDB(dbName);
    assert.commandFailed(db.runCommand({dbCheck: collName}));
}

function testErrorOnUnreplicated() {
    let primary = replSet.getPrimary();
    let db = primary.getDB("local");

    assert.commandFailed(db.runCommand({dbCheck: "oplog.rs"}),
                         "dbCheck spuriously succeeded on oplog");
    assert.commandFailed(primary.getDB(dbName).runCommand({dbCheck: "system.profile"}),
                         "dbCheck spuriously succeeded on system.profile");
}

testErrorOnNonexistent();
testErrorOnSecondary();
testErrorOnUnreplicated();

// Test stepdown.
function testSucceedsOnStepdown() {
    let primary = replSet.getPrimary();
    let db = primary.getDB(dbName);

    let nodeId = replSet.getNodeId(primary);
    assert.commandWorked(db.runCommand({dbCheck: multiBatchSimpleCollName}));

    // Step down the primary.
    assert.commandWorked(primary.getDB("admin").runCommand({replSetStepDown: 0, force: true}));

    // Wait for the cluster to come up.
    replSet.awaitSecondaryNodes();

    // Find the node we ran dbCheck on.
    db = replSet.getSecondaries()
             .filter(function isPreviousPrimary(node) {
                 return replSet.getNodeId(node) === nodeId;
             })[0]
             .getDB(dbName);

    // Check that it's still responding.
    try {
        assert.commandWorked(db.runCommand({ping: 1}), "ping failed after stepdown during dbCheck");
    } catch (e) {
        doassert("cannot connect after dbCheck with stepdown");
    }

    // And that our dbCheck completed.
    assert(dbCheckCompleted(db), "dbCheck failed to terminate on stepdown");
}

testSucceedsOnStepdown();

// Temporarily restart the secondary as a standalone, inject an inconsistency and
// restart it back as a secondary.
function injectInconsistencyOnSecondary(cmd) {
    const secondaryConn = replSet.getSecondary();
    const secondaryNodeId = replSet.getNodeId(secondaryConn);
    replSet.stop(secondaryNodeId, {forRestart: true /* preserve dbPath */});

    const standaloneConn = MongoRunner.runMongod({
        dbpath: secondaryConn.dbpath,
        noCleanData: true,
    });

    const standaloneDB = standaloneConn.getDB(dbName);
    assert.commandWorked(standaloneDB.runCommand(cmd));

    // Shut down the secondary and restart it as a member of the replica set.
    MongoRunner.stopMongod(standaloneConn);
    replSet.start(secondaryNodeId, {}, true /*restart*/);
    replSet.awaitNodesAgreeOnPrimaryNoAuth();
}

// Just add an extra document, and test that it catches it.
function simpleTestCatchesExtra() {
    {
        const primary = replSet.getPrimary();
        const db = primary.getDB(dbName);
        db[collName].drop();
        clearLog();

        // Create the collection on the primary.
        db.createCollection(collName, {validationLevel: "off"});
    }

    replSet.awaitReplication();
    injectInconsistencyOnSecondary({insert: collName, documents: [{}]});
    replSet.awaitReplication();

    {
        const primary = replSet.getPrimary();
        const db = primary.getDB(dbName);

        assert.commandWorked(db.runCommand({dbCheck: collName}));
        awaitDbCheckCompletion(db, collName);
    }
    assert.soon(function() {
        return (replSet.getSecondary()
                    .getDB("local")
                    .system.healthlog.find({"operation": "dbCheckStop"})
                    .itcount() === 1);
    }, "dbCheck didn't complete on secondary");
    const errors = replSet.getSecondary().getDB("local").system.healthlog.find(
        {operation: /dbCheck.*/, severity: "error"});

    assert.eq(errors.count(),
              1,
              "expected exactly 1 inconsistency after single inconsistent insertion, found: " +
                  JSON.stringify(errors.toArray()));
}

simpleTestCatchesExtra();

replSet.stopSet();
})();
