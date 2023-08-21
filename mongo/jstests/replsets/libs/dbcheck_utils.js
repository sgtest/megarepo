/**
 * Contains helper functions for testing dbCheck.
 */

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

export const dbCheckCompleted = (db) => {
    return db.getSiblingDB("admin").currentOp().inprog.filter(x => x["desc"] == "dbCheck")[0] ===
        undefined;
};

// Wait for dbCheck to complete (on both primaries and secondaries).
export const awaitDbCheckCompletion = (replSet, db, withClearedHealthLog = true) => {
    assert.soon(() => dbCheckCompleted(db), "dbCheck timed out");
    replSet.awaitSecondaryNodes();
    replSet.awaitReplication();

    if (withClearedHealthLog) {
        forEachNonArbiterNode(replSet, function(node) {
            const healthlog = node.getDB('local').system.healthlog;
            assert.soon(function() {
                return (healthlog.find({"operation": "dbCheckStop"}).itcount() == 1);
            }, "dbCheck command didn't complete");
        });
    }
};

// Clear health log and insert nDocs documents.
export const resetAndInsert = (replSet, db, collName, nDocs) => {
    db[collName].drop();
    clearHealthLog(replSet);

    assert.commandWorked(
        db[collName].insertMany([...Array(nDocs).keys()].map(x => ({a: x})), {ordered: false}));
    replSet.awaitReplication();
};

// Run dbCheck with given parameters and potentially wait for completion.
export const runDbCheck = (replSet,
                           db,
                           collName,
                           parameters = {},
                           awaitCompletion = false,
                           withClearedHealthLog = true,
                           allowedErrorCodes = []) => {
    let dbCheckCommand = {dbCheck: collName};
    for (let parameter in parameters) {
        dbCheckCommand[parameter] = parameters[parameter];
    }

    assert.commandWorkedOrFailedWithCode(db.runCommand(dbCheckCommand), allowedErrorCodes);
    if (awaitCompletion) {
        awaitDbCheckCompletion(replSet, db, withClearedHealthLog);
    }
};

export const checkHealthLog = (healthlog, query, numExpected, timeout = 60 * 1000) => {
    let query_count;

    assert.soon(
        function() {
            query_count = healthlog.find(query).count();
            if (query_count != numExpected) {
                jsTestLog("dbCheck command didn't complete, health log query returned " +
                          query_count + " entries, expected " + numExpected +
                          "  query: " + tojson(query) +
                          " found: " + JSON.stringify(healthlog.find(query).toArray()));
            }
            return query_count == numExpected;
        },
        "dbCheck command didn't complete, health log query returned " + query_count +
            " entries, expected " + numExpected + "  query: " + tojson(query) +
            " found: " + JSON.stringify(healthlog.find(query).toArray()),
        timeout);
};

// Returns a list of all collections in a given database excluding views.
function listCollectionsWithoutViews(database) {
    var failMsg = "'listCollections' command failed";
    var res = assert.commandWorked(database.runCommand("listCollections"), failMsg);
    return res.cursor.firstBatch.filter(c => c.type == "collection");
}

// Run dbCheck for all collections in the database with given parameters and potentially wait for
// completion.
export const runDbCheckForDatabase = (replSet, db, awaitCompletion = false) => {
    listCollectionsWithoutViews(db)
        .map(c => c.name)
        .forEach(collName => runDbCheck(
                     replSet,
                     db,
                     collName,
                     {} /* parameters */,
                     false /* awaitCompletion */,
                     false /* withClearedHealthLog */,
                     [
                         ErrorCodes.NamespaceNotFound /* collection got dropped. */,
                         ErrorCodes.CommandNotSupportedOnView /* collection got dropped and a view
                                                                 got created with the same name. */
                         ,
                         40619 /* collection is not replicated error. */
                     ] /* allowedErrorCodes */));

    if (awaitCompletion) {
        awaitDbCheckCompletion(replSet, db, false /*withClearedHealthLog*/);
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
