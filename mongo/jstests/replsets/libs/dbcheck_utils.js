/**
 * Contains helper functions for testing dbCheck.
 */
import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {FeatureFlagUtil} from "jstests/libs/feature_flag_util.js";

export const defaultSnapshotSize = 1000;
export const logQueries = {
    allErrorsOrWarningsQuery: {$or: [{"severity": "warning"}, {"severity": "error"}]},
    recordNotFoundQuery: {
        "severity": "error",
        "msg": "found extra index key entry without corresponding document",
        "data.context.indexSpec": {$exists: true}
    },
    missingIndexKeysQuery: {
        "severity": "error",
        "msg": "Document has missing index keys",
        "data.context.missingIndexKeys": {$exists: true},
    },
    recordDoesNotMatchQuery: {
        "severity": "error",
        "msg":
            "found index key entry with corresponding document/keystring set that does not contain the expected key string",
        "data.context.indexSpec": {$exists: true}
    },
    collNotFoundWarningQuery: {
        severity: "warning",
        "msg": "abandoning dbCheck extra index keys check because collection no longer exists"
    },
    indexNotFoundWarningQuery: {
        severity: "warning",
        "msg": "abandoning dbCheck extra index keys check because index no longer exists"
    },
    duringInitialSyncQuery:
        {severity: "warning", "msg": "cannot execute dbcheck due to ongoing initial sync"},
    errorQuery: {"severity": "error"},
    warningQuery: {"severity": "warning"},
    infoOrErrorQuery:
        {$or: [{"severity": "info", "operation": "dbCheckBatch"}, {"severity": "error"}]},
    infoBatchQuery: {"severity": "info", "operation": "dbCheckBatch"},
    inconsistentBatchQuery: {"severity": "error", "msg": "dbCheck batch inconsistent"},
};

// Apply function on all secondary nodes except arbiters.
export const forEachNonArbiterSecondary = (replSet, f) => {
    for (let secondary of replSet.getSecondaries()) {
        if (!secondary.adminCommand({isMaster: 1}).arbiterOnly) {
            f(secondary);
        }
    }
};

// Apply function on primary and all secondary nodes.
export const forEachNonArbiterNode = (replSet, f) => {
    f(replSet.getPrimary());
    forEachNonArbiterSecondary(replSet, f);
};

// Clear local.system.healthlog.
export const clearHealthLog = (replSet) => {
    forEachNonArbiterNode(replSet, conn => conn.getDB("local").system.healthlog.drop());
    replSet.awaitReplication();
};

export const logEveryBatch =
    (replSet) => {
        forEachNonArbiterNode(replSet, conn => {
            assert.commandWorked(
                conn.adminCommand({setParameter: 1, "dbCheckHealthLogEveryNBatches": 1}));
        })
    }

export const dbCheckCompleted = (db) => {
    const inprog = db.getSiblingDB("admin").currentOp().inprog;
    return inprog == undefined || inprog.filter(x => x["desc"] == "dbCheck")[0] === undefined;
};

// Wait for dbCheck to complete (on both primaries and secondaries).
export const awaitDbCheckCompletion =
    (replSet, db, waitForHealthLogDbCheckStop = true, awaitCompletionTimeoutMs = null) => {
        assert.soon(
            () => dbCheckCompleted(db),
            "dbCheck timed out for database: " + db.getName() + " for RS: " + replSet.getURL(),
            awaitCompletionTimeoutMs);

        const tokens = replSet.nodes.map(node => node._securityToken);
        try {
            // This function might be called with a security token (to specify a tenant) on a
            // connection. Calling tenant agnostic commands to await replication conflict with this
            // token so temporarily remove it.
            replSet.nodes.forEach(node => node._setSecurityToken(undefined));
            replSet.awaitSecondaryNodes();
            replSet.awaitReplication();

            if (waitForHealthLogDbCheckStop) {
                forEachNonArbiterNode(replSet, function(node) {
                    const healthlog = node.getDB('local').system.healthlog;
                    assert.soon(
                        function() {
                            return (healthlog.find({"operation": "dbCheckStop"}).itcount() == 1);
                        },
                        "dbCheck command didn't complete for database: " + db.getName() +
                            " for RS: " + replSet.getURL() +
                            ", found health log: " + tojson(healthlog.find().toArray()));
                });
            }
        } finally {
            replSet.nodes.forEach((node, idx) => {
                node._setSecurityToken(tokens[idx]);
            });
        }
    };

// Clear health log and insert nDocs documents.
export const resetAndInsert = (replSet, db, collName, nDocs, docSuffix = null) => {
    db[collName].drop();
    clearHealthLog(replSet);

    if (docSuffix) {
        assert.commandWorked(db[collName].insertMany(
            [...Array(nDocs).keys()].map(x => ({a: x.toString() + docSuffix})), {ordered: false}));
    } else {
        assert.commandWorked(
            db[collName].insertMany([...Array(nDocs).keys()].map(x => ({a: x})), {ordered: false}));
    }

    replSet.awaitReplication();
    assert.eq(db.getCollection(collName).find({}).count(), nDocs);
};

// Clear health log and insert nDocs documents with two fields `a` and `b`.
export const resetAndInsertTwoFields = (replSet, db, collName, nDocs, docSuffix = null) => {
    db[collName].drop();
    clearHealthLog(replSet);

    if (docSuffix) {
        assert.commandWorked(db[collName].insertMany(
            [...Array(nDocs).keys()].map(
                x => ({a: x.toString() + docSuffix, b: x.toString() + docSuffix})),
            {ordered: false}));
    } else {
        assert.commandWorked(db[collName].insertMany(
            [...Array(nDocs).keys()].map(x => ({a: x, b: x})), {ordered: false}));
    }

    replSet.awaitReplication();
    assert.eq(db.getCollection(collName).find({}).count(), nDocs);
};

// Clear health log and insert nDocs documents with identical 'a' field
export const resetAndInsertIdentical = (replSet, db, collName, nDocs) => {
    db[collName].drop();
    clearHealthLog(replSet);

    assert.commandWorked(db[collName].insertMany(
        [...Array(nDocs).keys()].map(x => ({_id: x, a: 0.1})), {ordered: false}));

    replSet.awaitReplication();
    assert.eq(db.getCollection(collName).find({}).count(), nDocs);
};

// Insert numDocs documents with missing index keys for testing.
export const insertDocsWithMissingIndexKeys =
    (replSet, dbName, collName, doc, numDocs = 1, doPrimary = true, doSecondary = true) => {
        const primaryDb = replSet.getPrimary().getDB(dbName);
        const secondaryDb = replSet.getSecondary().getDB(dbName);

        assert.commandWorked(primaryDb.createCollection(collName));

        // Create an index for every key in the document.
        let index = {};
        for (let key in doc) {
            index[key] = 1;
            assert.commandWorked(primaryDb[collName].createIndex(index));
            index = {};
        }
        replSet.awaitReplication();

        // dbCheck requires the _id index to iterate through documents in a batch.
        let skipIndexNewRecordsExceptIdPrimary;
        let skipIndexNewRecordsExceptIdSecondary;
        if (doPrimary) {
            skipIndexNewRecordsExceptIdPrimary =
                configureFailPoint(primaryDb, "skipIndexNewRecords", {skipIdIndex: false});
        }
        if (doSecondary) {
            skipIndexNewRecordsExceptIdSecondary =
                configureFailPoint(secondaryDb, "skipIndexNewRecords", {skipIdIndex: false});
        }
        for (let i = 0; i < numDocs; i++) {
            assert.commandWorked(primaryDb[collName].insert(doc));
        }
        replSet.awaitReplication();
        if (doPrimary) {
            skipIndexNewRecordsExceptIdPrimary.off();
        }
        if (doSecondary) {
            skipIndexNewRecordsExceptIdSecondary.off();
        }

        // Verify that index has been replicated to all nodes, including _id index.
        forEachNonArbiterNode(replSet, function(node) {
            assert.eq(Object.keys(doc).length + 1,
                      node.getDB(dbName)[collName].getIndexes().length);
        });
    }

// Run dbCheck with given parameters and potentially wait for completion.
export const runDbCheck = (replSet,
                           db,
                           collName,
                           parameters = {},
                           awaitCompletion = false,
                           waitForHealthLogDbCheckStop = true,
                           allowedErrorCodes = []) => {
    if (!parameters.hasOwnProperty('maxBatchTimeMillis')) {
        // Make this huge because stalls and pauses sometimes break this test.
        parameters['maxBatchTimeMillis'] = 20000;
    }
    let dbCheckCommand = {dbCheck: collName};
    for (let parameter in parameters) {
        dbCheckCommand[parameter] = parameters[parameter];
    }

    let res =
        assert.commandWorkedOrFailedWithCode(db.runCommand(dbCheckCommand), allowedErrorCodes);
    if (res.ok && awaitCompletion) {
        awaitDbCheckCompletion(replSet, db, waitForHealthLogDbCheckStop);
    }
};

export const checkHealthLog = (healthlog, query, numExpected, timeout = 60 * 1000) => {
    let query_count;
    assert.soon(
        function() {
            query_count = healthlog.find(query).count();
            if (query_count != numExpected) {
                jsTestLog("health log query returned " + query_count + " entries, expected " +
                          numExpected + "  query: " + tojson(query) +
                          " found: " + tojson(healthlog.find(query).toArray()));
            }
            return query_count == numExpected;
        },
        "health log query returned " + query_count + " entries, expected " + numExpected +
            "  query: " + tojson(query) + " found: " + tojson(healthlog.find(query).toArray()) +
            " HealthLog: " + tojson(healthlog.find().toArray()),
        timeout);
};

// Temporarily restart the secondary as a standalone, inject an inconsistency and
// restart it back as a secondary.
export const injectInconsistencyOnSecondary = (replSet, dbName, cmd, noCleanData = true) => {
    const secondaryConn = replSet.getSecondary();
    const secondaryNodeId = replSet.getNodeId(secondaryConn);
    replSet.stop(secondaryNodeId, {forRestart: true /* preserve dbPath */});

    const standaloneConn = MongoRunner.runMongod({
        dbpath: secondaryConn.dbpath,
        noCleanData: noCleanData,
    });

    const standaloneDB = standaloneConn.getDB(dbName);
    assert.commandWorked(standaloneDB.runCommand(cmd));

    // Shut down the secondary and restart it as a member of the replica set.
    MongoRunner.stopMongod(standaloneConn);
    replSet.start(secondaryNodeId, {}, true /*restart*/);
    replSet.awaitNodesAgreeOnPrimaryNoAuth();
};

// Returns a list of all collections in a given database excluding views.
function listCollectionsWithoutViews(database) {
    var failMsg = "'listCollections' command failed";
    // Some tests adds an invalid view, resulting in a failure of the 'listCollections' operation
    // with an 'InvalidViewDefinition' error.
    let res = assert.commandWorkedOrFailedWithCode(
        database.runCommand("listCollections"), ErrorCodes.InvalidViewDefinition, failMsg);
    if (res.ok) {
        return res.cursor.firstBatch.filter(c => c.type == "collection");
    }
    return [];
}

// Returns a list of names of all indexes.
function getIndexNames(db, collName, allowedErrorCodes) {
    var failMsg = "'listIndexes' command failed";
    let res = assert.commandWorkedOrFailedWithCode(
        db[collName].runCommand("listIndexes"), allowedErrorCodes, failMsg);
    if (res.ok) {
        return new DBCommandCursor(db, res).toArray().map(spec => spec.name);
    }
    return [];
}

// List of collection names that are ignored from dbcheck.
const collNamesIgnoredFromDBCheck = [
    "operationalLatencyHistogramTest_coll_temp",
    "top_coll_temp",
];

// Run dbCheck for all collections in the database with given parameters and potentially wait for
// completion.
export const runDbCheckForDatabase =
    (replSet, db, awaitCompletion = false, awaitCompletionTimeoutMs = null) => {
        const secondaryIndexCheckEnabled =
            checkSecondaryIndexChecksInDbCheckFeatureFlagEnabled(replSet.getPrimary());
        let collDbCheckParameters = {};
        if (secondaryIndexCheckEnabled) {
            collDbCheckParameters = {validateMode: "dataConsistencyAndMissingIndexKeysCheck"};
        }

        const allowedErrorCodes = [
            ErrorCodes.NamespaceNotFound /* collection got dropped. */,
            ErrorCodes.CommandNotSupportedOnView /* collection got dropped and a view
                                                    got created with the same name. */
            ,
            40619 /* collection is not replicated error. */,
            // Some tests adds an invalid view, resulting in a failure of the 'dbcheck'
            // operation with an 'InvalidViewDefinition' error.
            ErrorCodes.InvalidViewDefinition,
            // Might hit stale shardVersion response from shard config while racing with
            // 'dropCollection' command.
            ErrorCodes.StaleConfig
        ];

        listCollectionsWithoutViews(db).map(c => c.name).forEach(collName => {
            if (collNamesIgnoredFromDBCheck.includes(collName)) {
                jsTestLog("dbCheck (" + tojson(collDbCheckParameters) + ") is skipped on ns: " +
                          db.getName() + "." + collName + " for RS: " + replSet.getURL());
                return;
            }

            jsTestLog("dbCheck (" + tojson(collDbCheckParameters) + ") is starting on ns: " +
                      db.getName() + "." + collName + " for RS: " + replSet.getURL());
            runDbCheck(replSet,
                       db,
                       collName,
                       collDbCheckParameters /* parameters */,
                       false /* awaitCompletion */,
                       false /* waitForHealthLogDbCheckStop */,
                       allowedErrorCodes);
            jsTestLog("dbCheck (" + tojson(collDbCheckParameters) + ") is done on ns: " +
                      db.getName() + "." + collName + " for RS: " + replSet.getURL());

            if (!secondaryIndexCheckEnabled) {
                return;
            }

            getIndexNames(db, collName, allowedErrorCodes).forEach(indexName => {
                let extraIndexDbCheckParameters = {
                    validateMode: "extraIndexKeysCheck",
                    secondaryIndex: indexName
                };
                jsTestLog("dbCheck (" + tojson(extraIndexDbCheckParameters) +
                          ") is starting on ns: " + db.getName() + "." + collName +
                          " for RS: " + replSet.getURL());
                runDbCheck(replSet,
                           db,
                           collName,
                           extraIndexDbCheckParameters /* parameters */,
                           false /* awaitCompletion */,
                           false /* waitForHealthLogDbCheckStop */,
                           allowedErrorCodes);
                jsTestLog("dbCheck (" + tojson(extraIndexDbCheckParameters) + ") is done on ns: " +
                          db.getName() + "." + collName + " for RS: " + replSet.getURL());
            });
        });

        if (awaitCompletion) {
            awaitDbCheckCompletion(
                replSet, db, false /*waitForHealthLogDbCheckStop*/, awaitCompletionTimeoutMs);
        }
    };

// Assert no errors/warnings (i.e., found inconsistencies). Tolerate
// SnapshotTooOld errors, as they can occur if the primary is slow enough processing a
// batch that the secondary is unable to obtain the timestamp the primary used.
export const assertForDbCheckErrors = (node,
                                       assertForErrors = true,
                                       assertForWarnings = false,
                                       errorsFound = []) => {
    let severityValues = [];
    if (assertForErrors == true) {
        severityValues.push("error");
    }

    if (assertForWarnings == true) {
        severityValues.push("warning");
    }

    const healthlog = node.getDB('local').system.healthlog;
    // Regex matching strings that start without "SnapshotTooOld"
    const regexStringWithoutSnapTooOld = /^((?!^SnapshotTooOld).)*$/;

    // healthlog is a capped collection, truncation during scan might cause cursor
    // invalidation. Truncated data is most likely from previous tests in the fixture, so we
    // should still be able to catch errors by retrying.
    assert.soon(() => {
        try {
            let errs = healthlog.find(
                {"severity": {$in: severityValues}, "data.error": regexStringWithoutSnapTooOld});
            if (errs.hasNext()) {
                const errMsg = "dbCheck found inconsistency on " + node.host;
                jsTestLog(errMsg + ". Errors/Warnings: ");
                let err;
                for (let count = 0; errs.hasNext() && count < 20; count++) {
                    err = errs.next();
                    errorsFound.push(err);
                    jsTestLog(tojson(err));
                }
                assert(false, errMsg);
            }
            return true;
        } catch (e) {
            if (e.code !== ErrorCodes.CappedPositionLost) {
                throw e;
            }
            jsTestLog(`Retrying on CappedPositionLost error: ${tojson(e)}`);
            return false;
        }
    }, "healthlog scan could not complete.", 60000);

    jsTestLog("Checked health log for on " + node.host);
};

// Check for dbcheck errors for all nodes in a replica set and ignoring arbiters.
export const assertForDbCheckErrorsForAllNodes =
    (rst, assertForErrors = true, assertForWarnings = false) => {
        forEachNonArbiterNode(
            rst, node => assertForDbCheckErrors(node, assertForErrors, assertForWarnings));
    };

/**
 * Utility for checking if the featureFlagSecondaryIndexChecksInDbCheck is on.
 */
export function checkSecondaryIndexChecksInDbCheckFeatureFlagEnabled(conn) {
    return FeatureFlagUtil.isPresentAndEnabled(conn, 'SecondaryIndexChecksInDbCheck');
}

export function checkNumSnapshots(debugBuild, expectedNumSnapshots) {
    if (debugBuild) {
        const actualNumSnapshots =
            rawMongoProgramOutput()
                .split(/7844808.*Catalog snapshot for reverse lookup check ending/)
                .length -
            1;
        assert.eq(actualNumSnapshots,
                  expectedNumSnapshots,
                  "expected " + expectedNumSnapshots +
                      " catalog snapshots during reverse lookup, found " + actualNumSnapshots);
    }
}

export function setSnapshotSize(rst, snapshotSize) {
    forEachNonArbiterNode(rst, conn => {
        assert.commandWorked(conn.adminCommand(
            {"setParameter": 1, "dbCheckMaxTotalIndexKeysPerSnapshot": snapshotSize}));
    });
}

export function resetSnapshotSize(rst) {
    setSnapshotSize(rst, defaultSnapshotSize);
}

// Verifies that the healthlog contains entries that span the entire range that dbCheck should run
// against.
export function assertCompleteCoverage(
    healthlog, nDocs, docSuffix, start, end, inconsistentBatch = false) {
    // For non-empty docSuffix like 'aaa' for instance, if we insert over 10 docs, the lexicographic
    // sorting order would be '0aaa', '1aaa', '10aaa', instead of increasing numerical order. Skip
    // these checks as we have test coverage without needing to account for these specific cases.
    if (nDocs >= 10 && (docSuffix !== null || docSuffix !== "")) {
        return;
    }

    const truncateDocSuffix =
        (batchBoundary, docSuffix) => {
            const index = batchBoundary.indexOf(docSuffix);
            jsTestLog("Index : " + index);
            if (index < 1) {
                return batchBoundary;
            }
            return batchBoundary.substring(0, batchBoundary.indexOf(docSuffix));
        }

    let query = logQueries.infoBatchQuery;
    if (inconsistentBatch) {
        query = {"severity": "error", "msg": "dbCheck batch inconsistent"};
    }

    const batches = healthlog.find(query).toArray();
    let expectedBatchStart = start === null ? 0 : start;
    let batchEnd = "";
    for (let batch of batches) {
        let batchStart = batch.data.batchStart.a;
        if (docSuffix) {
            batchStart = truncateDocSuffix(batchStart, docSuffix);
        }

        // Verify that the batch start is correct.
        assert.eq(expectedBatchStart, batchStart);
        // Set our next expected batch start to the next value after the end of this batch.
        batchEnd = batch.data.batchEnd.a;
        if (docSuffix) {
            batchEnd = truncateDocSuffix(batchEnd, docSuffix);
        }
        expectedBatchStart = batchEnd + 1;
    }

    if (end === null) {
        // User did not issue a custom range, assert that we checked all documents.
        // TODO (SERVER-86323): Fix this behavior and ensure maxKey is logged.
        assert.eq(nDocs - 1, batchEnd);
    } else {
        // User issued a custom end, but we do not know if the documents in the collection actually
        // ended at that range. Verify that we have hit either the end of the collection, or we
        // checked up until the specified range.
        assert((batchEnd === nDocs - 1) || (batchEnd === end),
               `batch end ${batchEnd} did not equal end of collection ${
                   nDocs - 1} nor end of custom range ${end}`);
    }
}
