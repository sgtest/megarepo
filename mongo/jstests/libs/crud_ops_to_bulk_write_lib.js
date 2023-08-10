/**
 * Utility functions used to convert CRUD ops into a bulkWrite command.
 * Converts the bulkWrite responses into the original CRUD response.
 */
export const BulkWriteUtils = (function() {
    const commandsToBulkWriteOverride = new Set(["insert", "update", "delete"]);

    const commandsToAlwaysFlushBulkWrite = new Set([
        "aggregate",
        "mapreduce",
        "authenticate",
        "logout",
        "applyops",
        "checkshardingindex",
        "cleanuporphaned",
        "cleanupreshardcollection",
        "commitreshardcollection",
        "movechunk",
        "moveprimary",
        "moverange",
        "mergechunks",
        "refinecollectionshardkey",
        "split",
        "splitvector",
        "killallsessions",
        "killallsessionsbypattern",
        "dropconnections",
        "filemd5",
        "fsync",
        "fsyncunlock",
        "killop",
        "setfeaturecompatibilityversion",
        "shutdown",
        "currentop",
        "listdatabases",
        "listcollections",
        "committransaction",
        "aborttransaction",
        "preparetransaction",
        "endsessions",
        "killsessions"
    ]);

    let numOpsPerResponse = [];
    let nsInfos = [];
    let bufferedOps = [];
    let letObj = null;
    let wc = null;
    let ordered = true;
    let bypassDocumentValidation = null;

    function canProcessAsBulkWrite(cmdName) {
        return commandsToBulkWriteOverride.has(cmdName);
    }

    function commandToFlushBulkWrite(cmdName) {
        return commandsToAlwaysFlushBulkWrite.has(cmdName);
    }

    function resetBulkWriteBatch() {
        numOpsPerResponse = [];
        nsInfos = [];
        bufferedOps = [];
        letObj = null;
        wc = null;
        bypassDocumentValidation = null;
        ordered = true;
    }

    function getCurrentBatchSize() {
        return numOpsPerResponse.length;
    }

    function getBulkWriteState() {
        return {
            bypassDocumentValidation: bypassDocumentValidation,
            letObj: letObj,
            ordered: ordered
        };
    }

    function getNamespaces() {
        return nsInfos;
    }

    function flushCurrentBulkWriteBatch(
        conn, originalRunCommand, makeRunCommandArgs, additionalParameters = {}) {
        if (bufferedOps.length == 0) {
            return {};
        }

        // Should not be possible to reach if bypassDocumentValidation is not set.
        assert(bypassDocumentValidation != null);

        let bulkWriteCmd = {
            "bulkWrite": 1,
            "ops": bufferedOps,
            "nsInfo": nsInfos,
            "ordered": (ordered != null) ? ordered : true,
            "bypassDocumentValidation": bypassDocumentValidation,
        };

        if (letObj != null) {
            bulkWriteCmd["let"] = letObj;
        }

        if (wc != null) {
            bulkWriteCmd["writeConcern"] = wc;
        }

        // Add in additional parameters to the bulkWrite command.
        bulkWriteCmd = {...bulkWriteCmd, ...additionalParameters};

        let resp = {};
        resp = originalRunCommand.apply(conn, makeRunCommandArgs(bulkWriteCmd, "admin"));

        let response = convertBulkWriteResponse(bulkWriteCmd, resp);
        let finalResponse = response;

        let expectedResponseLength = numOpsPerResponse.length;

        // Retry on ordered:true failures by re-running subset of original bulkWrite command.
        while (finalResponse.length != expectedResponseLength) {
            // Need to figure out how many ops we need to subset out. Every entry in
            // numOpsPerResponse represents a number of bulkWrite ops that correspond to an initial
            // CRUD op. We need to make sure we split at a CRUD op boundary in the bulkWrite.
            for (let i = 0; i < response.length; i++) {
                let target = numOpsPerResponse.shift();
                for (let j = 0; j < target; j++) {
                    bufferedOps.shift();
                }
            }
            bulkWriteCmd.ops = bufferedOps;

            resp = originalRunCommand.apply(conn, makeRunCommandArgs(bulkWriteCmd, "admin"));
            response = convertBulkWriteResponse(bulkWriteCmd, resp);
            finalResponse = finalResponse.concat(response);
        }

        return finalResponse;
    }

    function initializeResponse(op) {
        if (op.hasOwnProperty("update")) {
            // Update always has nModified field set.
            return {"n": 0, "nModified": 0, "ok": 1};
        }
        return {"n": 0, "ok": 1};
    }

    /**
     * The purpose of this function is to take a server response from a bulkWrite command and to
     * transform it to an array of responses for the corresponding CRUD commands that make up the
     * bulkWrite.
     *
     * 'cmd' is the bulkWrite that was executed to generate the response
     * 'orig' is the bulkWrite command response
     */
    function convertBulkWriteResponse(cmd, bulkWriteResponse) {
        // a w0 write concern bulkWrite can result in just {ok: 1}, so if a response does not have
        // 'cursor' field then just return the response as is
        if (!bulkWriteResponse.cursor) {
            return [bulkWriteResponse];
        }

        let responses = [];
        if (bulkWriteResponse.ok == 1) {
            let cursorIdx = 0;
            for (let numOps of numOpsPerResponse) {
                let num = 0;
                let resp = initializeResponse(cmd.ops[cursorIdx]);
                while (num < numOps) {
                    if (cursorIdx >= bulkWriteResponse.cursor.firstBatch.length) {
                        // this can happen if the bulkWrite encountered an error processing
                        // an op with ordered:true set. This means we have no more op responses
                        // left to process so push the current response we were building and
                        // return.
                        // If the last response has writeErrors set then it was in the middle of an
                        // op otherwise we are beginning a new op response and should not push it.
                        if (resp.writeErrors) {
                            responses.push(resp);
                        }
                        return responses;
                    }

                    let current = bulkWriteResponse.cursor.firstBatch[cursorIdx];

                    if (current.ok == 0) {
                        // Normal write contains an error.
                        if (!resp.hasOwnProperty("writeErrors")) {
                            resp["writeErrors"] = [];
                        }
                        let writeError = {index: num, code: current.code, errmsg: current.errmsg};

                        // Include optional error fields if they exist.
                        ["errInfo",
                         "db",
                         "collectionUUID",
                         "expectedCollection",
                         "actualCollection"]
                            .forEach(property => {
                                if (current.hasOwnProperty(property)) {
                                    writeError[property] = current[property];
                                }
                            });

                        resp["writeErrors"].push(writeError);
                    } else {
                        resp.n += current.n;
                        if (current.hasOwnProperty("nModified")) {
                            resp.nModified += current.nModified;
                        }
                        if (current.hasOwnProperty("upserted")) {
                            if (!resp.hasOwnProperty("upserted")) {
                                resp["upserted"] = [];
                            }
                            // Need to update the index of the upserted doc.
                            current.upserted.index = cursorIdx;
                            resp["upserted"].push(current.upserted);
                        }
                    }

                    ["writeConcernError",
                     "opTime",
                     "$clusterTime",
                     "electionId",
                     "operationTime",
                     "errorLabels",
                     "_mongo"]
                        .forEach(property => {
                            if (bulkWriteResponse.hasOwnProperty(property)) {
                                resp[property] = bulkWriteResponse[property];
                            }
                        });

                    cursorIdx += 1;
                    num += 1;
                }
                responses.push(resp);
            }
        }
        return responses;
    }

    function getNsInfoIdx(nsInfoEntry, collectionUUID, encryptionInformation) {
        let idx = nsInfos.findIndex((element) => element.ns == nsInfoEntry);
        if (idx == -1) {
            idx = nsInfos.length;
            let nsInfo = {ns: nsInfoEntry};
            if (collectionUUID) {
                nsInfo["collectionUUID"] = collectionUUID;
            }
            if (encryptionInformation) {
                nsInfo["encryptionInformation"] = encryptionInformation;
            }
            nsInfos.push(nsInfo);
        }
        return idx;
    }

    function processInsertOp(nsInfoIdx, doc) {
        return {insert: nsInfoIdx, document: doc};
    }

    function processUpdateOp(nsInfoIdx, cmdObj, update) {
        let op = {
            "update": nsInfoIdx,
            "filter": update.q,
            "updateMods": update.u,
            "multi": update.multi ? update.multi : false,
            "upsert": update.upsert ? update.upsert : false,
            "upsertSupplied": update.upsertSupplied ? update.upsertSupplied : false,
        };

        ["arrayFilters", "collation", "hint", "sampleId"].forEach(property => {
            if (update.hasOwnProperty(property)) {
                op[property] = update[property];
            }
        });

        if (update.hasOwnProperty("c")) {
            op["constants"] = update.c;
        }

        if (cmdObj.hasOwnProperty("let")) {
            letObj = cmdObj.let;
        }

        return op;
    }

    function processDeleteOp(nsInfoIdx, cmdObj, deleteCmd) {
        let op = {"delete": nsInfoIdx, "filter": deleteCmd.q, "multi": deleteCmd.limit == 0};

        ["sampleId"].forEach(property => {
            if (cmdObj.hasOwnProperty(property)) {
                op[property] = cmdObj[property];
            }
        });

        ["collation", "hint"].forEach(property => {
            if (deleteCmd.hasOwnProperty(property)) {
                op[property] = deleteCmd[property];
            }
        });

        if (cmdObj.hasOwnProperty("let")) {
            letObj = cmdObj.let;
        }

        return op;
    }

    function processCRUDOp(dbName, cmdName, cmdObj) {
        // Set bypassDocumentValidation if necessary.
        if (bypassDocumentValidation == null) {
            bypassDocumentValidation = cmdObj.hasOwnProperty("bypassDocumentValidation")
                ? cmdObj.bypassDocumentValidation
                : false;
        }

        ordered = cmdObj.hasOwnProperty("ordered") ? cmdObj.ordered : true;

        if (cmdObj.hasOwnProperty("writeConcern")) {
            wc = cmdObj.writeConcern;
        }

        let nsInfoEntry = dbName + "." + cmdObj[cmdName];
        let nsInfoIdx =
            getNsInfoIdx(nsInfoEntry, cmdObj.collectionUUID, cmdObj.encryptionInformation);

        let numOps = 0;

        if (cmdName === "insert") {
            assert(cmdObj.documents);
            for (let doc of cmdObj.documents) {
                bufferedOps.push(processInsertOp(nsInfoIdx, doc));
                numOps += 1;
            }
        } else if (cmdName === "update") {
            assert(cmdObj.updates);
            for (let update of cmdObj.updates) {
                bufferedOps.push(processUpdateOp(nsInfoIdx, cmdObj, update));
                numOps += 1;
            }
        } else if (cmdName === "delete") {
            assert(cmdObj.deletes);
            for (let deleteCmd of cmdObj.deletes) {
                bufferedOps.push(processDeleteOp(nsInfoIdx, cmdObj, deleteCmd));
                numOps += 1;
            }
        } else {
            throw new Error("Unrecognized command in bulkWrite override");
        }

        numOpsPerResponse.push(numOps);
    }

    return {
        processCRUDOp: processCRUDOp,
        getNsInfoIdx: getNsInfoIdx,
        flushCurrentBulkWriteBatch: flushCurrentBulkWriteBatch,
        resetBulkWriteBatch: resetBulkWriteBatch,
        commandToFlushBulkWrite: commandToFlushBulkWrite,
        canProcessAsBulkWrite: canProcessAsBulkWrite,
        getCurrentBatchSize: getCurrentBatchSize,
        getBulkWriteState: getBulkWriteState,
        getNamespaces: getNamespaces
    };
})();
