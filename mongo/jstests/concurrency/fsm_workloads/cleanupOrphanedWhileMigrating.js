/**
 * Performs range deletions while chunks are being moved.
 *
 * @tags: [requires_sharding, assumes_balancer_on, antithesis_incompatible]
 */

import {extendWorkload} from "jstests/concurrency/fsm_libs/extend_workload.js";
import {
    $config as $baseConfig
} from "jstests/concurrency/fsm_workloads/sharded_base_partitioned.js";
load('jstests/concurrency/fsm_workload_helpers/balancer.js');

export const $config = extendWorkload($baseConfig, function($config, $super) {
    $config.threadCount = 5;
    $config.iterations = 50;

    $config.data.shardKey = {skey: 1};
    $config.data.shardKeyField = 'skey';

    const numChunks = 50;
    const numDocs = 100;

    // Total count of documents when initialized.
    $config.data.initialCount = numChunks * numDocs;

    // Run cleanupOrphaned on a random shard's primary node.
    $config.states.cleanupOrphans = function(db, collName, connCache) {
        const ns = db[collName].getFullName();

        // Get index of random shard.
        const shardNames = Object.keys(connCache.shards);
        const randomIndex = Math.floor(Math.random() * shardNames.length);

        const shardConn = connCache.rsConns.shards[shardNames[randomIndex]];

        // Disable balancing so that waiting for orphan cleanup can converge quickly.
        BalancerHelper.disableBalancerForCollection(db, ns);

        // Ensure the cleanup of all chunk orphans of the primary shard
        assert.soonNoExcept(() => {
            assert.commandWorked(shardConn.adminCommand({cleanupOrphaned: ns}));
            return true;
        }, undefined, 10 * 1000, 100);

        // Reenable balancing.
        BalancerHelper.enableBalancerForCollection(db, ns);
    };

    // Verify that counts are stable.
    $config.states.validate = function(db, collName, connCache) {
        const ns = db[collName].getFullName();

        // Get total count from mongos. Need to specify batch count that is larger than the total
        // number of records to prevent getmore command from being issued since stepdown suites
        // ban it.
        const mongos = ChunkHelper.getRandomMongos(connCache.mongos);
        const coll = mongos.getCollection(ns);
        const totalCount = coll.find({}).batchSize($config.data.initialCount + numDocs).itcount();

        // Verify that sum equals original total.
        assert(this.initialCount === totalCount,
               "Document count doesn't match initial count: " + this.initialCount +
                   " != " + totalCount);
    };

    $config.states.init = function init(db, collName, connCache) {};

    $config.setup = function setup(db, collName, cluster) {
        const ns = db[collName].getFullName();

        // Disallow balancing 'ns' during $setup so it does not interfere with the splits.
        BalancerHelper.disableBalancerForCollection(db, ns);
        BalancerHelper.joinBalancerRound(db);

        for (let chunkIndex = 0; chunkIndex < numChunks; chunkIndex++) {
            let bulk = db[collName].initializeUnorderedBulkOp();

            const splitKey = chunkIndex * numDocs;
            print("splitting at: " + splitKey);

            for (let docIndex = splitKey - numDocs; docIndex < splitKey; docIndex++) {
                bulk.insert({_id: docIndex, skey: docIndex});
            }

            assertAlways.commandWorked(bulk.execute());

            if (chunkIndex > 0) {
                assert.commandWorked(db.adminCommand({split: ns, middle: {skey: splitKey}}));
            }
        }

        // Allow balancing 'ns' again.
        BalancerHelper.enableBalancerForCollection(db, ns);
    };

    $config.transitions = {
        init: {cleanupOrphans: 1},
        cleanupOrphans: {cleanupOrphans: 0.5, validate: 0.5},
        validate: {cleanupOrphans: 1}
    };

    return $config;
});
