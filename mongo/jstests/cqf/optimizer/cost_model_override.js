/**
 * Tests that when we modify Cost Model Coefficents using `internalCostModelCoefficients` parameter
 * the cost of produced query plan changed.
 */

import {
    assertValueOnPath,
    checkCascadesOptimizerEnabled,
    navigateToPlanPath,
    runWithFastPathsDisabled,
} from "jstests/libs/optimizer_utils.js";

if (!checkCascadesOptimizerEnabled(db)) {
    jsTestLog("Skipping test because the optimizer is not enabled");
    quit();
}

const coll = db.cost_model_override;
coll.drop();

const nDocuments = 100;
assert.commandWorked(coll.insert(Array.from({length: nDocuments}, (_, i) => {
    return {a: 3, b: 3, c: i};
})));

function executeAndGetScanCost(scanIncrementalCost) {
    const getScanCost = function() {
        // Cost estimation will be skipped if the query is optimized using a fast path.
        const explain =
            runWithFastPathsDisabled(() => coll.explain("executionStats").aggregate([]));
        assert.eq(nDocuments, explain.executionStats.nReturned);

        const scanNode = navigateToPlanPath(explain, "child");
        assertValueOnPath("PhysicalScan", scanNode, "nodeType");

        return scanNode.properties.cost;
    };
    const initCost = getScanCost();
    try {
        assert.commandWorked(db.adminCommand({
            'setParameter': 1,
            'internalCostModelCoefficients': `{"scanIncrementalCost": ${scanIncrementalCost}}`
        }));

        return getScanCost();
    } finally {
        // Empty "internalCostModelCoefficients" should reset the cost model to default.
        assert.commandWorked(
            db.adminCommand({'setParameter': 1, 'internalCostModelCoefficients': ''}));
        const resetCost = getScanCost();

        assert.close(initCost, resetCost, 8 /*decimal places*/);
    }
}

const scanCost1 = executeAndGetScanCost(0.2);
const scanCost2 = executeAndGetScanCost(0.4);
assert.lt(scanCost1, scanCost2);
