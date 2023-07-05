load('jstests/aggregation/extras/utils.js');  // For assertArrayEq.

import {
    checkCascadesFeatureFlagEnabled,
    extractLogicalCEFromNode,
    getPlanSkeleton,
    navigateToRootNode,
} from "jstests/libs/optimizer_utils.js";

/**
 * Returns a simplified skeleton of the physical plan including intervals & logical CE.
 */
export function summarizeExplainForCE(explain) {
    const node = getPlanSkeleton(navigateToRootNode(explain), {
        extraKeepKeys: ["interval", "properties"],
        printLogicalCE: true,
    });
    return node;
}

/**
 * Extracts the cardinality estimate of the explain root node.
 */
export function getRootCE(explain) {
    const rootNode = navigateToRootNode(explain);
    assert.neq(rootNode, null, tojson(explain));
    assert.eq(rootNode.nodeType, "Root", tojson(rootNode));
    return extractLogicalCEFromNode(rootNode);
}

/**
 * Asserts that expected and actual are equal, within a small tolerance.
 */
export function assertApproxEq(expected, actual, msg, tolerance = 0.01) {
    assert(Math.abs(expected - actual) < tolerance, msg);
}

/**
 * Validates that the results and cardinality estimate for a given $match predicate agree. Note that
 * if the ce parameter is omitted, we expect our estimate to exactly match what the query actually
 * returns.
 */
export function verifyCEForMatch({coll, predicate, expected, ce, hint}) {
    jsTestLog(`Verify CE for match ${tojson(predicate)}`);
    const CEs = ce ? [ce] : undefined;
    return verifyCEForMatchNodes(
        {coll, predicate, expected, getNodeCEs: (explain) => [getRootCE(explain)], CEs, hint});
}

/**
 * Validates that the results and cardinality estimate for a given $match predicate agree.
 * The caller should specify a function 'getNodeCEs' which takes explain output as an input, and
 * returns the cardinality estimates of the nodes the caller wants to verify in an array. The
 * expected estimates should be defined in CEs, or it defaults to the number of documents expected
 * to be returned by the query.
 */
export function verifyCEForMatchNodes({coll, predicate, expected, getNodeCEs, CEs, hint}) {
    // Run aggregation & verify query results.
    const options = hint ? {hint} : {};
    const actual = coll.aggregate([{$match: predicate}], options).toArray();
    assertArrayEq({actual, expected});

    // Obtain explain.
    const explain = coll.explain().aggregate([{$match: predicate}], options);
    const explainSummarized = tojson(summarizeExplainForCE(explain));
    jsTestLog(explainSummarized);

    // Verify expected vs. actual CE.
    const actualCEs = getNodeCEs(explain);
    const expectedCEs = CEs == undefined ? [expected.length] : CEs;
    assert.eq(actualCEs.length, expectedCEs.length);
    for (let i = 0; i < actualCEs.length; i++) {
        const actualCE = actualCEs[i];
        const expectedCE = expectedCEs[i];
        assertApproxEq(actualCE,
                       expectedCE,
                       `${tojson(predicate)} node ${i} should have been estimated as ${
                           expectedCE}, estimated ${actualCE} instead.`);
    }
}

/**
 * Creates a histogram for the given 'coll' along the input field 'key'.
 */
export function createHistogram(coll, key, options = {}) {
    // We can't use forceBonsai here because the new optimizer doesn't know how to handle the
    // analyze command.
    assert.commandWorked(coll.getDB().adminCommand(
        {setParameter: 1, internalQueryFrameworkControl: "tryBonsaiExperimental"}));

    // Set up histogram for test collection.
    const res = coll.getDB().runCommand(Object.assign({analyze: coll.getName(), key}, options));
    assert.commandWorked(res);
}

/**
 * Validates that the generated histogram for the given "coll" has the expected type counters.
 */
export function createAndValidateHistogram({coll, expectedHistogram, empty = false, options = {}}) {
    const field = expectedHistogram._id;
    createHistogram(coll, field, options);

    const stats = db.system.statistics[coll.getName()];

    // Validate histograms.
    const expected = empty ? [] : [expectedHistogram];
    const actual = stats.aggregate({$match: {_id: field}}).toArray();
    assertArrayEq({actual, expected});
}

/**
 * Useful boilerplate code for tests that need to use the analyze command and/or histogram
 * estimation. This ensures that the appropriate flags/query knobs are set and ensures the state of
 * relevant flags is restored after the test.
 */
export function runHistogramsTest(test) {
    if (!checkCascadesFeatureFlagEnabled(db)) {
        jsTestLog("Skipping test because the optimizer is not enabled");
        return;
    }

    // We will be updating some query knobs, so store the old state and restore it after the test.
    const {internalQueryCardinalityEstimatorMode, internalQueryFrameworkControl} = db.adminCommand({
        getParameter: 1,
        internalQueryCardinalityEstimatorMode: 1,
        internalQueryFrameworkControl: 1,
    });
    jsTestLog(`Settings before: internalQueryCardinalityEstimatorMode: ${
        internalQueryCardinalityEstimatorMode}, internalQueryFrameworkControl: ${
        internalQueryFrameworkControl}`);

    try {
        test();
    } finally {
        // Reset query knobs to their original state.
        assert.commandWorked(db.adminCommand({
            setParameter: 1,
            internalQueryCardinalityEstimatorMode,
            internalQueryFrameworkControl
        }));
        let cqfControlAfter = db.adminCommand({getParameter: 1, internalQueryFrameworkControl: 1});
        jsTestLog(`Settings after: ${tojson(cqfControlAfter)}`);
    }
}

/**
 * Creates a single-field index for each field in the 'fields' array.
 */
export function createIndexes(coll, fields) {
    for (const field of fields) {
        assert.commandWorked(coll.createIndex({[field]: 1}));
    }
}

/**
 * Creates statistics for each field in the 'fields' array.
 */
export function analyzeFields(db, coll, fields, bucketCnt = 100) {
    for (const field of fields) {
        assert.commandWorked(
            db.runCommand({analyze: coll.getName(), key: field, numberBuckets: bucketCnt}));
    }
}

/**
 * Given a scalar histogram document print it combining bounds with the corresponding buckets.
 * hist = { buckets: [{boundaryCount: 1, rangeCount: 0, ...}], bounds: [100, 500]}
 */
export function printScalarHistogram(hist) {
    assert.eq(hist.buckets.length, hist.bounds.length);
    let i = 0;
    while (i < hist.buckets.length) {
        print(`BucketId: ${i}, ${hist.bounds[i]}, ${tojsononeline(hist.buckets[i])}\n`);
        i++;
    }
}

export function printHistogram(hist) {
    jsTestLog(`Histogram on field: ${hist._id}`);
    print("Scalar Histogram:\n");
    printScalarHistogram(hist.statistics.scalarHistogram);

    if (hist.statistics.hasOwnProperty("arrayStatistics")) {
        print("Array statistics:\n");
        print("Unique Histogram:\n");
        printScalarHistogram(hist.statistics.arrayStatistics.uniqueHistogram);
        print("Min Histogram:\n");
        printScalarHistogram(hist.statistics.arrayStatistics.minHistogram);
        print("Max Histogram:\n");
        printScalarHistogram(hist.statistics.arrayStatistics.maxHistogram);
    }
}
