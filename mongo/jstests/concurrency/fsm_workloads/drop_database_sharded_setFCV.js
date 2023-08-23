/**
 * Repeatedly creates and drops a database in concurrency with FCV changes
 *
 * @tags: [
 *   requires_sharding,
 *   # Requires all nodes to be running the latest binary.
 *   multiversion_incompatible,
 *  ]
 */

import {assertAlways} from "jstests/concurrency/fsm_libs/assert.js";
import {extendWorkload} from "jstests/concurrency/fsm_libs/extend_workload.js";
import {$config as $baseConfig} from "jstests/concurrency/fsm_workloads/drop_database_sharded.js";

export const $config = extendWorkload($baseConfig, function($config, $super) {
    $config.states.setFCV = function(db, collName) {
        const fcvValues = [lastLTSFCV, lastContinuousFCV, latestFCV];
        const targetFCV = fcvValues[Random.randInt(3)];
        jsTestLog('Executing FCV state, setting to:' + targetFCV);
        try {
            assertAlways.commandWorked(
                db.adminCommand({setFeatureCompatibilityVersion: targetFCV, confirm: true}));
        } catch (e) {
            if (e.code === 5147403) {
                // Invalid fcv transition (e.g lastContinuous -> lastLTS)
                jsTestLog('setFCV: Invalid transition');
                return;
            }
            if (e.code === 7428200) {
                // Cannot upgrade FCV if a previous FCV downgrade stopped in the middle of cleaning
                // up internal server metadata.
                assertAlways.eq(latestFCV, targetFCV);
                jsTestLog(
                    'setFCV: Cannot upgrade FCV if a previous FCV downgrade stopped in the middle \
                    of cleaning up internal server metadata');
                return;
            }
            if (e.code === 12587) {
                // Cannot downgrade FCV that requires a collMod command when index builds are
                // concurrently taking place.
                jsTestLog(
                    'setFCV: Cannot downgrade the FCV that requires a collMod command when index \
                    builds are concurrently running');
                return;
            }
            throw e;
        }
        jsTestLog('setFCV state finished');
    };

    // Inherithed methods get overridden to tolerate the interruption of
    // internal transactions on the config server during the execution of setFCV.
    $config.states.enableSharding = function(db, collName) {
        try {
            $super.states.enableSharding.apply(this, arguments);
        } catch (err) {
            if (err.code !== ErrorCodes.Interrupted) {
                throw err;
            }
        }
    };

    $config.states.shardCollection = function(db, collName) {
        try {
            $super.states.shardCollection.apply(this, arguments);
        } catch (err) {
            if (err.code !== ErrorCodes.Interrupted) {
                throw err;
            }
        }
    };

    $config.transitions = {
        init: {enableSharding: 0.3, dropDatabase: 0.3, shardCollection: 0.3, setFCV: 0.1},
        enableSharding: {enableSharding: 0.3, dropDatabase: 0.3, shardCollection: 0.3, setFCV: 0.1},
        dropDatabase: {enableSharding: 0.3, dropDatabase: 0.3, shardCollection: 0.3, setFCV: 0.1},
        shardCollection:
            {enableSharding: 0.3, dropDatabase: 0.3, shardCollection: 0.3, setFCV: 0.1},
        setFCV: {enableSharding: 0.3, dropDatabase: 0.3, shardCollection: 0.3, setFCV: 0.1},
    };

    $config.teardown = function(db, collName) {
        assert.commandWorked(
            db.adminCommand({setFeatureCompatibilityVersion: latestFCV, confirm: true}));
        $super.teardown(db, collName);
    };

    return $config;
});
