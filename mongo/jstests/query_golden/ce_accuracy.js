/**
 * Tests for cardinality estimation accuracy.
 * @tags: [
 *   requires_cqf,
 * ]
 */

import {runHistogramsTest} from "jstests/libs/ce_stats_utils.js";
import {runWithFastPathsDisabled, runWithParams} from "jstests/libs/optimizer_utils.js";
import {getCEDocs, getCEDocs1} from "jstests/query_golden/libs/ce_data.js";
import {runCETestForCollection} from "jstests/query_golden/libs/run_queries_ce.js";

await runHistogramsTest(function() {
    runWithParams(
        [
            // Sargable nodes & Filter nodes get different CEs.
            {key: "internalCascadesOptimizerDisableSargableWhenNoIndexes", value: false},
            {key: "internalCascadesOptimizerEnableParameterization", value: false}
        ],
        () => {
            const coll = db.ce_data_20;
            coll.drop();

            jsTestLog("Populating collection");
            assert.commandWorked(coll.insertMany(getCEDocs()));
            assert.commandWorked(coll.insertMany(getCEDocs1()));
            const collSize = coll.find().itcount();
            print(`Collection count: ${collSize}\n`);

            const collMeta = {
                "collectionName": "ce_data_20",
                "fields": [
                    {"fieldName": "a", "dataType": "integer", "indexed": true},
                    {"fieldName": "b", "dataType": "string", "indexed": true},
                    {"fieldName": "c_int", "dataType": "array", "indexed": true},
                    {"fieldName": "mixed", "dataType": "mixed", "indexed": true},
                ],
                "compound_indexes": [],
                "cardinality": 20
            };

            // Flag to show more information for debugging purposes:
            // - adds execution of sampling CE strategy;
            // - prints plan skeleton.
            const ceDebugFlag = false;
            // Cardinality estimation will be skipped if the query is optimized using a fast path.
            runWithFastPathsDisabled(() => runCETestForCollection(db, collMeta, 4, ceDebugFlag));
        });
});
