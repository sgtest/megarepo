/**
 * Concurrently performs DDL commands and FCV changes and verifies guarantees are
 * not broken.
 *
 * @tags: [
 *   requires_sharding,
 *   # TODO (SERVER-56879) Support add/remove shards in new DDL paths
 *   does_not_support_add_remove_shards,
 *   # Requires all nodes to be running the latest binary.
 *   multiversion_incompatible,
 *   # TODO (SERVER-88539) Remove the 'assumes_balancer_off' tag
 *   assumes_balancer_off,
 *  ]
 */

import {extendWorkload} from "jstests/concurrency/fsm_libs/extend_workload.js";
import {
    uniformDistTransitions
} from "jstests/concurrency/fsm_workload_helpers/state_transition_utils.js";
import {$config as $baseConfig} from "jstests/concurrency/fsm_workloads/random_DDL_operations.js";

export const $config = extendWorkload($baseConfig, function($config, $super) {
    $config.states.setFCV = function(db, collName, connCache) {
        const fcvValues = [lastLTSFCV, lastContinuousFCV, latestFCV];
        const targetFCV = fcvValues[Random.randInt(3)];
        jsTestLog('Executing FCV state, setting to:' + targetFCV);
        try {
            assert.commandWorked(
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
                assert.eq(latestFCV, targetFCV);
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

    $config.transitions = uniformDistTransitions($config.states);

    $config.teardown = function(db, collName, cluster) {
        assert.commandWorked(
            db.adminCommand({setFeatureCompatibilityVersion: latestFCV, confirm: true}));
    };

    // TODO SERVER-84271: The downgrade for featureFlagReplicateVectoredInsertsTransactionally is
    // what causes the Interrupted error, so this can be removed when that feature flag is removed.
    $config.data.movePrimaryAllowedErrorCodes =
        [...$super.data.movePrimaryAllowedErrorCodes, ErrorCodes.Interrupted];

    return $config;
});
