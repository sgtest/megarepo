/*
 * yield_rooted_or.js (extends yield.js)
 *
 * Intersperse queries which use a rooted OR stage with updates and deletes of documents they may
 * match.
 * Other workloads that need an index on c and d can inherit from this.
 */
import {extendWorkload} from "jstests/concurrency/fsm_libs/extend_workload.js";
import {$config as $baseConfig} from "jstests/concurrency/fsm_workloads/yield.js";

export const $config = extendWorkload($baseConfig, function($config, $super) {
    /*
     * Issue a query with an or stage as the root.
     */
    $config.states.query = function rootedOr(db, collName) {
        var nMatches = 100;

        var cursor = db[collName]
                         .find({$or: [{c: {$lte: nMatches / 2}}, {d: {$lte: nMatches / 2}}]})
                         .batchSize(this.batchSize);

        var verifier = function rootedOrVerifier(doc, prevDoc) {
            return (doc.c <= nMatches / 2 || doc.d <= nMatches / 2);
        };

        this.advanceCursor(cursor, verifier);
    };

    $config.data.genUpdateDoc = function genUpdateDoc() {
        var newC = Random.randInt(this.nDocs);
        var newD = Random.randInt(this.nDocs);
        return {$set: {c: newC, d: newD}};
    };

    $config.setup = function setup(db, collName, cluster) {
        $super.setup.apply(this, arguments);

        assertAlways.commandWorked(db[collName].createIndex({c: 1}));
        assertAlways.commandWorked(db[collName].createIndex({d: 1}));
    };

    return $config;
});
