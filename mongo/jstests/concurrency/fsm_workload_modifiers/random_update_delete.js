/**
 * This file adds stages to perform random update/delete batches.
 *
 * Updates always increment the counter field of the targeted documents by 1, but otherwise randomly
 * select values for multi and upsert:
 *
 * multi: false, upsert: false -> Targets by _id or shard key and updates a single owned document.
 *
 * multi: true, upsert: false -> Targets by thread id and updates all owned documents.
 *
 * multi: false, upsert: true -> Reinsert an owned document previously deleted by a limit: 1
 * delete if possible, otherwise behaves the same as multi:false, upsert: false.
 *
 * multi: true, upsert: true -> Same as multi: false, upsert: true because the server requires
 * upserts target by _id or shard key.
 *
 * Deletes randomly vary by limit:
 *
 * limit: 0 -> Targets by thread id and deletes all owned documents. After all deletes in a batch
 * containing a limit: 0 delete are complete, all documents owned by this thread will be reinserted
 * according to their initial state (i.e. counter 0).
 *
 * limit: 1 -> Targets by _id or shard key and deletes a single owned document. Documents deleted in
 * this manner will be prioritized by future upserts.
 */

import {
    findFirstBatch,
    inNonTransactionalStepdownSuite,
    runWithManualRetriesIfInNonTransactionalStepdownSuite
} from "jstests/concurrency/fsm_workload_helpers/stepdown_suite_helpers.js";

function chooseRandomMapValue(map) {
    const keys = Array.from(map.keys());
    const index = Random.randInt(keys.length);
    const doc = map.get(keys[index]);
    return doc;
}

export function randomUpdateDelete($config, $super) {
    $config.threadCount = 5;
    $config.iterations = 50;

    $config.data.expectPartialMultiWrites = inNonTransactionalStepdownSuite();
    $config.data.expectExtraMultiWrites = false;

    $config.data.getShardKey = function getShardKey(collName) {
        // If the final workload is operating on a sharded collection and doesn't always use the
        // shardKey field as its shard key, override this function.
        if (this.shardKey !== undefined) {
            return this.shardKey;
        }
        // Unsharded (i.e. unsplittable) collections always use {_id: 1} as the shard key.
        return {_id: 1};
    };

    $config.data.getShardKeyField = function getShardKeyField(collName) {
        const shardKey = this.getShardKey(collName);
        const fields = Object.keys(shardKey);
        // This test assumes that the shard key is on a single field.
        assert.eq(fields.length, 1);
        return fields[0];
    };

    $config.data.getTargetedDocuments = function getTargetedDocuments(collName, query) {
        const shardKeyField = this.getShardKeyField(collName);
        const keys = new Set();
        if (shardKeyField in query) {
            keys.add(query[shardKeyField]);
        } else {
            for (const [key, _] of this.expectedDocs) {
                keys.add(key);
            }
        }
        return keys;
    };

    $config.data.incrementCounterForDoc = function incrementCounterForDoc(key) {
        const original = (() => {
            if (this.expectedDocs.has(key)) {
                return this.expectedDocs.get(key);
            }
            return this.initialDocs.get(key);
        })();
        this.expectedDocs.set(key, {...original, counter: original.counter + 1});
    };

    $config.data.createRandomUpdateBatch = function createRandomUpdateBatch(collName) {
        const batchSize = 1 + Random.randInt(2);
        const updates = [];
        for (let i = 0; i < batchSize; i++) {
            updates.push(this.createRandomUpdate(collName));
        }
        return updates;
    };

    $config.data.createRandomUpdate = function createRandomUpdate(collName) {
        const multi = Random.rand() > 0.5;
        const upsert = Random.rand() > 0.5;
        const q = this.createUpdateQuery(collName, multi, upsert);
        return {q, u: {$inc: {counter: 1}}, multi, upsert};
    };

    $config.data.createUpdateQuery = function createUpdateQuery(collName, multi, upsert) {
        let query = {tid: this.tid};
        if (multi && !upsert) {
            return query;
        }
        // If we are an updateOne or upsert, we need to choose a specific document to update.
        const documents = (() => {
            if (upsert) {
                // Prefer to reinsert a previously deleted document if possible.
                const deleted = this.getDeletedDocuments();
                if (deleted.size > 0) {
                    return deleted;
                }
            }
            return this.expectedDocs;
        })();
        const shardKeyField = this.getShardKeyField(collName);
        const randomDoc = chooseRandomMapValue(documents);
        query[shardKeyField] = randomDoc[shardKeyField];
        // Upsert will use the fields in the query to create the document, and we assume that at the
        // beginning of the test the shard key matches _id.
        if (shardKeyField !== "_id") {
            query["_id"] = query[shardKeyField];
        }
        return query;
    };

    $config.data.createRandomDeleteBatch = function createRandomDeleteBatch(collName) {
        const batchSize = 1 + Random.randInt(2);
        const deletes = [];
        for (let i = 0; i < batchSize; i++) {
            deletes.push(this.createRandomDelete(collName));
        }
        return deletes;
    };

    $config.data.createRandomDelete = function createRandomDelete(collName) {
        const limit = Random.rand() > 0.5 ? 0 : 1;
        return {q: this.createDeleteQuery(collName, limit), limit};
    };

    $config.data.createDeleteQuery = function createDeleteQuery(collName, limit) {
        let query = {tid: this.tid};
        if (limit === 0) {
            return query;
        }
        if (this.expectedDocs.size === 0) {
            return query;
        }
        // If we are a deleteOne, we need to choose a specific document to delete.
        const shardKeyField = this.getShardKeyField(collName);
        const randomDoc = chooseRandomMapValue(this.expectedDocs);
        query[shardKeyField] = randomDoc[shardKeyField];
        return query;
    };

    $config.data.getDeletedDocuments = function getDeletedDocuments() {
        const deleted = new Map();
        for (const [key, doc] of this.initialDocs) {
            if (!this.expectedDocs.has(key)) {
                deleted.set(key, doc);
            }
        }
        return deleted;
    };

    $config.data.readOwnedDocuments = function readOwnedDocuments(db, collName) {
        const shardKeyField = this.getShardKeyField(collName);
        const documents = findFirstBatch(db, collName, {tid: this.tid}, 1000);
        const map = new Map();
        for (const doc of documents) {
            map.set(doc[shardKeyField], doc);
        }
        return map;
    };

    $config.data.verifyDocumentCounters = function verifyDocumentCounters(onDisk) {
        for (const key of this.expectedDocs.keys()) {
            // Verify the counter separately from the rest of the document to account for differing
            // semantics around retries.
            const expectedDoc = this.expectedDocs.get(key);
            const actualDoc = onDisk.get(key);
            const {counter: expectedCounter, ...expectedDocNoCounter} = expectedDoc;
            const {counter: actualCounter, ...actualDocNoCounter} = actualDoc;
            assert.docEq(expectedDocNoCounter, actualDocNoCounter);
            assert(this.counterWithinRange(expectedCounter, actualCounter),
                   `Expected counter ${tojson(expectedDoc)} and on disk counter ${
                       tojson(actualDoc)} differ by more than allowed`);
        }
    };

    $config.data.counterWithinRange = function counterWithinRange(expected, actual) {
        // It's always correct for the counter to be the expected value.
        if (actual === expected) {
            return true;
        }
        // For sharded collections, we expect to retry multi updates in some cases, which can lead
        // to updating the counter additional times.
        if (actual > expected) {
            return this.expectExtraMultiWrites;
        }
        // For unsharded collections or when failovers are enabled, we expect that sometimes a multi
        // update operation will be killed and fail to update some of the documents that match its
        // filter.
        if (actual === expected - 1) {
            return this.expectPartialMultiWrites;
        }
        return false;
    };

    $config.data.verifyUpdateResult = function verifyUpdateResult(expected, actual) {
        if (this.expectPartialMultiWrites) {
            assert.lte(actual.n, expected.n);
            assert.lte(actual.nModified, expected.nModified);
            return;
        }
        // Even if we expect extra multi updates due to retries, a single operation can only update
        // as many documents as actually exist.
        assert.eq(actual.n, expected.n);
        assert.eq(actual.nModified, expected.nModified);
    };

    $config.data.verifyDeleteResult = function verifyDeleteResult(expected, actual) {
        // In either case, if we can't expect a deleteMany to delete each matched document exactly
        // once, then we expect that the number must be less. If partial operations are possible,
        // then we could have been killed before deleting each document. If retried operations are
        // possible, we could have deleted some documents on the first pass, so they no longer exist
        // in the second pass.
        if (this.expectPartialMultiWrites || this.expectExtraMultiWrites) {
            assert.lte(actual, expected);
            return;
        }
        assert.eq(actual, expected);
    };

    $config.setup = function setup(db, collName, cluster) {
        $super.setup.apply(this, arguments);
        findFirstBatch(db, collName, {}, 1000).forEach(doc => {
            const q = {_id: doc._id};
            let mods = {};
            if (!("tid" in doc)) {
                mods = {...mods, tid: Random.randInt($config.threadCount)};
            }
            if (!("counter" in doc)) {
                mods = {...mods, counter: 0};
            }
            db[collName].update(q, {$set: mods});
        });
    };

    $config.states.init = function init(db, collName, connCache) {
        $super.states.init.apply(this, arguments);
        this.initialDocs = this.readOwnedDocuments(db, collName);
        this.expectedDocs = new Map(this.initialDocs);
        jsTestLog(`Thread with tid ${this.tid} owns ${this.initialDocs.size} documents`);
    };

    $config.states.performUpdates = function multiUpdate(db, collName, connCache) {
        runWithManualRetriesIfInNonTransactionalStepdownSuite(() => {
            this.expectedDocs = this.readOwnedDocuments(db, collName);
            const updates = this.createRandomUpdateBatch(collName);
            jsTestLog("Executing updates: " + tojson(updates));
            const result = db.runCommand({update: collName, updates});
            jsTestLog("Result: " + tojson(result));
            assert.commandWorked(result);
            let totalUpdates = 0;
            let totalUpserts = 0;
            for (const update of updates) {
                const updatedKeys = this.getTargetedDocuments(collName, update.q);
                totalUpdates += updatedKeys.size;
                for (const key of updatedKeys) {
                    if (!this.expectedDocs.has(key)) {
                        totalUpserts++;
                    }
                    this.incrementCounterForDoc(key);
                }
            }
            const onDisk = this.readOwnedDocuments(db, collName);
            this.verifyDocumentCounters(onDisk);
            this.verifyUpdateResult({n: totalUpdates, nModified: totalUpdates - totalUpserts},
                                    result);
        });
    };

    $config.states.performDeletes = function multiDelete(db, collName, connCache) {
        runWithManualRetriesIfInNonTransactionalStepdownSuite(() => {
            this.expectedDocs = this.readOwnedDocuments(db, collName);
            const deletes = this.createRandomDeleteBatch(collName);
            jsTestLog("Executing deletes: " + tojson(deletes));
            const result = db.runCommand({delete: collName, deletes});
            jsTestLog("Result: " + tojson(result));
            assert.commandWorked(result);
            let uniqueDeletes = new Set();
            for (const deleteOp of deletes) {
                const deletedKeys = this.getTargetedDocuments(collName, deleteOp.q);
                for (const key of deletedKeys) {
                    uniqueDeletes.add(key);
                    this.expectedDocs.delete(key);
                }
            }
            const onDisk = this.readOwnedDocuments(db, collName);
            this.verifyDocumentCounters(onDisk);
            this.verifyDeleteResult(uniqueDeletes.size, result.n);

            if (this.expectedDocs.size > 0) {
                return;
            }
            jsTestLog(`Thread ${this.tid} has deleted all of its documents and will now reset to its initial state`);
            const bulk = db[collName].initializeUnorderedBulkOp();
            for (const [_, doc] of this.initialDocs) {
                bulk.insert(doc);
            }
            assert.commandWorked(bulk.execute());
        });
    };

    return $config;
}
