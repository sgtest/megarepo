/**
 * Runs updateOne, deleteOne, and findAndModify without shard key against a sharded cluster while
 * the collection reshards concurrently.
 *
 * @tags: [
 *  requires_fcv_71,
 *  requires_sharding,
 *  uses_transactions,
 * ]
 */

import {extendWorkload} from "jstests/concurrency/fsm_libs/extend_workload.js";
import {executeReshardCollection} from "jstests/concurrency/fsm_libs/reshard_collection_util.js";
import {
    $config as $baseConfig
} from "jstests/concurrency/fsm_workloads/write_without_shard_key_base.js";
import {FeatureFlagUtil} from "jstests/libs/feature_flag_util.js";

export const $config = extendWorkload($baseConfig, function($config, $super) {
    $config.startState = "init";

    const customShardKeyFieldName = "customShardKey";

    $config.data.shardKeys = [];
    $config.data.currentShardKeyIndex = -1;
    $config.data.reshardingCount = 0;

    $config.states.init = function init(db, collName, connCache) {
        $super.states.init.apply(this, arguments);
        this.shardKeys.push({[this.defaultShardKeyField]: 1});
        this.shardKeys.push({[customShardKeyFieldName]: 1});
        this.currentShardKeyIndex = 0;
        this._allowSameKeyResharding =
            FeatureFlagUtil.isPresentAndEnabled(db.getMongo(), 'ReshardingImprovements');
    };

    $config.data.generateRandomDocument = function generateRandomDocument(tid, partition) {
        const doc = $super.data.generateRandomDocument.apply(this, arguments);
        assert.neq(partition, null);
        doc[customShardKeyFieldName] = this.generateRandomInt(partition.lower, partition.upper - 1);
        return doc;
    };

    /**
     * Returns a random boolean.
     */
    $config.data.generateRandomBool = function generateRandomBool() {
        return Math.random() > 0.5;
    };

    $config.data.shouldSkipWriteResponseValidation = function shouldSkipWriteResponseValidation(
        res) {
        let shouldSkip = $super.data.shouldSkipWriteResponseValidation.apply(this, arguments);

        // This workload does in-place resharding so a retry that is sent
        // reshardingMinimumOperationDurationMillis after resharding completes is expected to fail
        // with IncompleteTransactionHistory.
        if (!shouldSkip && (res.code == ErrorCodes.IncompleteTransactionHistory)) {
            return res.errmsg.includes("Incomplete history detected for transaction");
        }

        return shouldSkip;
    };

    $config.states.reshardCollection = function reshardCollection(db, collName, connCache) {
        executeReshardCollection(this, db, collName, connCache, false /*sameKeyResharding*/);
    };

    $config.states.reshardCollectionSameKey = function reshardCollectionSameKey(
        db, collName, connCache) {
        executeReshardCollection(this, db, collName, connCache, this._allowSameKeyResharding);
    };

    $config.transitions = {
        init: {
            reshardCollection: 0.2,
            reshardCollectionSameKey: 0.1,
            updateOne: 0.3,
            deleteOne: 0.2,
            findAndModify: 0.2
        },
        updateOne: {
            reshardCollection: 0.2,
            reshardCollectionSameKey: 0.1,
            updateOne: 0.3,
            deleteOne: 0.2,
            findAndModify: 0.2
        },
        deleteOne: {
            reshardCollection: 0.2,
            reshardCollectionSameKey: 0.1,
            updateOne: 0.3,
            deleteOne: 0.2,
            findAndModify: 0.2
        },
        findAndModify: {
            reshardCollection: 0.2,
            reshardCollectionSameKey: 0.1,
            updateOne: 0.3,
            deleteOne: 0.2,
            findAndModify: 0.2
        },
        reshardCollection: {
            reshardCollection: 0.2,
            reshardCollectionSameKey: 0.1,
            updateOne: 0.3,
            deleteOne: 0.2,
            findAndModify: 0.2
        },
        reshardCollectionSameKey: {
            reshardCollection: 0.2,
            reshardCollectionSameKey: 0.1,
            updateOne: 0.3,
            deleteOne: 0.2,
            findAndModify: 0.2
        },
    };

    return $config;
});
