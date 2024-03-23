'use strict';

/**
 * Performs a series of {multi: true} updates/deletes while moving chunks with
 * pauseMigrationsDuringMultiUpdates enabled and verifies no updates or deletes
 * are lost.
 *
 * @tags: [
 * requires_sharding,
 * assumes_balancer_off,
 * incompatible_with_concurrency_simultaneous,
 * featureFlagPauseMigrationsDuringMultiUpdatesAvailable,
 * requires_fcv_80
 * ];
 */
import {extendWorkload} from "jstests/concurrency/fsm_libs/extend_workload.js";
import {$config as $baseConfig} from "jstests/concurrency/fsm_workloads/random_moveChunk_base.js";
import {
    findFirstBatch,
    withSkipRetryOnNetworkError
} from "jstests/concurrency/fsm_workload_helpers/stepdown_suite_helpers.js";
import {migrationsAreAllowed} from "jstests/libs/chunk_manipulation_util.js";

function ignoreErrorsIfInNonTransactionalStepdownSuite(fn) {
    // Even while pauseMigrationsDuringMultiUpdates is enabled, updateMany and deleteMany cannot be
    // resumed after a failover, and therefore may have only partially completed (unless we were
    // running in a transaction). We can't verify any constraints related to the updates actually
    // being made, but this test is still interesting to verify that the migration blocking state is
    // correctly managed even in the presence of failovers.
    if (TestData.runningWithShardStepdowns && !TestData.runInsideTransaction) {
        try {
            withSkipRetryOnNetworkError(fn);
        } catch (e) {
            jsTest.log("Ignoring error: " + e.code);
        }
    } else {
        fn();
    }
}

function createUpdateBatch(self) {
    const updates = [{q: {tid: self.tid}, u: {$inc: {counter: 1}}, multi: true}];
    let expectedUpdateCount = self.initialDocs.length;
    const batchSize = Random.randInt(2);
    for (let i = 0; i < batchSize; i++) {
        const multi = Random.rand() > 0.5;
        updates.push({q: {tid: self.tid}, u: {$inc: {ignoredCounter: 1}}, multi});
        expectedUpdateCount += multi ? self.initialDocs.length : 1;
    }
    Array.shuffle(updates);
    return {
        updates, expectedUpdateCount
    }
}

function createDeleteBatch(self) {
    const deletes = [{q: {tid: self.tid}, limit: 0}];
    const batchSize = Random.randInt(2);
    for (let i = 0; i < batchSize; i++) {
        deletes.push({q: {tid: self.tid}, limit: 1});
    }
    Array.shuffle(deletes);
    return deletes;
}

function getPauseMigrationsClusterParameter(db) {
    const response = assert.commandWorked(
        db.adminCommand({getClusterParameter: "pauseMigrationsDuringMultiUpdates"}));
    return response.clusterParameters[0].enabled;
}

function setPauseMigrationsClusterParameter(db, cluster, enabled) {
    assert.commandWorked(
        db.adminCommand({setClusterParameter: {pauseMigrationsDuringMultiUpdates: {enabled}}}));

    cluster.executeOnMongosNodes((db) => {
        // Ensure all mongoses have refreshed cluster parameter after being set.
        assert.soon(() => {return getPauseMigrationsClusterParameter(db) === enabled});
    });
}

export const $config = extendWorkload($baseConfig, function($config, $super) {
    $config.threadCount = 5;
    $config.iterations = 50;
    $config.data.partitionSize = 100;

    $config.data.isMoveChunkErrorAcceptable = (err) => {
        return err.code === ErrorCodes.Interrupted;
    };

    $config.setup = function setup(db, collName, cluster) {
        $super.setup.apply(this, arguments);
        setPauseMigrationsClusterParameter(db, cluster, true);
    };

    $config.teardown = function teardown(db, collName, cluster) {
        $super.teardown.apply(this, arguments);
        assert(migrationsAreAllowed(db, collName));
        setPauseMigrationsClusterParameter(db, cluster, false);
    };

    $config.states.init = function init(db, collName, connCache) {
        $super.states.init.apply(this, arguments);

        this.expectedCount = 0;
        findFirstBatch(db, collName, {tid: this.tid}, 1000).forEach(doc => {
            db[collName].update({_id: doc._id},
                                {$set: {counter: this.expectedCount, ignoredCounter: 0}});
        });

        this.initialDocs = findFirstBatch(db, collName, {tid: this.tid}, 1000);
        jsTestLog(`Thread with tid ${this.tid} owns ${this.initialDocs.length} documents`);
    };

    $config.states.multiUpdate = function multiUpdate(db, collName, connCache) {
        ignoreErrorsIfInNonTransactionalStepdownSuite(() => {
            const {updates, expectedUpdateCount} = createUpdateBatch(this);
            jsTestLog("Executing updates: " + tojson(updates));
            const result = db.runCommand({update: collName, updates})
            jsTestLog("Result: " + tojson(result));
            assert.commandWorked(result);
            assert.eq(result.n, expectedUpdateCount);
            assert.eq(result.n, result.nModified);
            this.expectedCount++;
        });
    };

    $config.states.multiDelete = function multiDelete(db, collName, connCache) {
        ignoreErrorsIfInNonTransactionalStepdownSuite(() => {
            const deletes = createDeleteBatch(this);
            jsTestLog("Executing deletes: " + tojson(deletes));
            const result = db.runCommand({delete: collName, deletes});
            jsTestLog("Result: " + tojson(result));
            assert.commandWorked(result);
            assert.eq(result.n, this.initialDocs.length);

            const bulk = db[collName].initializeUnorderedBulkOp();
            for (const doc of this.initialDocs) {
                bulk.insert(doc);
            }
            assert.commandWorked(bulk.execute());
            this.expectedCount = 0;
        });
    };

    $config.states.verify = function verify(db, collName, connCache) {
        ignoreErrorsIfInNonTransactionalStepdownSuite(() => {
            findFirstBatch(db, collName, {tid: this.tid}, 1000).forEach(doc => {
                assert.eq(doc.counter, this.expectedCount);
            });
        });
    };

    const weights = {moveChunk: 0.2, multiUpdate: 0.35, multiDelete: 0.35, verify: 0.1};
    $config.transitions = {
        init: weights,
        moveChunk: weights,
        multiUpdate: weights,
        multiDelete: weights,
        verify: weights,
    };

    return $config;
});
