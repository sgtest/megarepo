/**
 * Tests the serverStatus and FTDC metrics for multi planner execution (both classic and SBE).
 *
 * @tags: [featureFlagSbeFull]
 */

import {FeatureFlagUtil} from "jstests/libs/feature_flag_util.js";

function sumHistogramBucketCounts(histogram) {
    let sum = 0;
    for (const bucket of histogram) {
        sum += bucket.count;
    }
    return sum;
}

import {verifyGetDiagnosticData} from "jstests/libs/ftdc.js";

const collName = jsTestName();
const dbName = jsTestName();

// Use an isolated server instance to obtain predictible serverStatus planning metrics.
const conn = MongoRunner.runMongod({});
assert.neq(conn, null, "mongod failed to start");
const db = conn.getDB(dbName);
let coll = db.getCollection(collName);
coll.drop();

assert.commandWorked(coll.insert({_id: 3, a: 1, b: 1}));
assert.commandWorked(coll.insert({_id: 5, a: 1, b: 1}));
assert.commandWorked(coll.createIndex({a: 1}));
assert.commandWorked(coll.createIndex({b: 1}));

function assertSbeMultiPlannerMetrics(multiPlannerMetrics, expectedCount, checkHistograms = true) {
    if (checkHistograms) {
        assert.eq(sumHistogramBucketCounts(multiPlannerMetrics.histograms.sbeMicros),
                  expectedCount);
        assert.eq(sumHistogramBucketCounts(multiPlannerMetrics.histograms.sbeNumReads),
                  expectedCount);
        assert.eq(sumHistogramBucketCounts(multiPlannerMetrics.histograms.sbeNumPlans),
                  expectedCount);
    }
    assert.eq(multiPlannerMetrics.sbeCount, expectedCount);
    if (expectedCount > 0) {
        assert.gt(multiPlannerMetrics.sbeMicros, 0);
        assert.gt(multiPlannerMetrics.sbeNumReads, 0);
    } else {
        assert.eq(multiPlannerMetrics.sbeMicros, 0);
        assert.eq(multiPlannerMetrics.sbeNumReads, 0);
    }
}

function assertClassicMultiPlannerMetrics(
    multiPlannerMetrics, expectedCount, checkHistograms = true) {
    if (checkHistograms) {
        assert.eq(sumHistogramBucketCounts(multiPlannerMetrics.histograms.classicMicros),
                  expectedCount);
        assert.eq(sumHistogramBucketCounts(multiPlannerMetrics.histograms.classicNumPlans),
                  expectedCount);
        assert.eq(sumHistogramBucketCounts(multiPlannerMetrics.histograms.classicWorks),
                  expectedCount);
    }
    assert.eq(multiPlannerMetrics.classicCount, expectedCount);
    if (expectedCount > 0) {
        assert.gt(multiPlannerMetrics.classicMicros, 0);
        assert.gt(multiPlannerMetrics.classicWorks, 0);
    } else {
        assert.eq(multiPlannerMetrics.classicMicros, 0);
        assert.eq(multiPlannerMetrics.classicWorks, 0);
    }
}

// Verify initial metrics.
let multiPlannerMetrics = db.serverStatus().metrics.query.multiPlanner;
assertSbeMultiPlannerMetrics(multiPlannerMetrics, 0);
assertClassicMultiPlannerMetrics(multiPlannerMetrics, 0);

// Run with classic engine and verify metrics.
{
    assert.commandWorked(
        db.adminCommand({setParameter: 1, internalQueryFrameworkControl: "forceClassicEngine"}));
    assert.commandWorked(coll.find({a: 1, b: 1, c: 1}).explain());

    multiPlannerMetrics = db.serverStatus().metrics.query.multiPlanner;
    assertSbeMultiPlannerMetrics(multiPlannerMetrics, 0);
    assertClassicMultiPlannerMetrics(multiPlannerMetrics, 1);
}

// Run with SBE and verify metrics.
{
    assert.commandWorked(
        db.adminCommand({setParameter: 1, internalQueryFrameworkControl: "trySbeEngine"}));
    assert.commandWorked(coll.find({a: 1, b: 1, c: 1}).explain());

    multiPlannerMetrics = db.serverStatus().metrics.query.multiPlanner;
    if (FeatureFlagUtil.isPresentAndEnabled(db, "ClassicRuntimePlanningForSbe")) {
        assertSbeMultiPlannerMetrics(multiPlannerMetrics, 0);
        assertClassicMultiPlannerMetrics(multiPlannerMetrics, 2);
    } else {
        assertSbeMultiPlannerMetrics(multiPlannerMetrics, 1);
        assertClassicMultiPlannerMetrics(multiPlannerMetrics, 1);
    }
}

assert.soon(() => {
    // Verify FTDC includes aggregate metrics.
    const multiPlannerMetricsFtdc =
        verifyGetDiagnosticData(conn.getDB("admin")).serverStatus.metrics.query.multiPlanner;

    let expectedClassicCount = 0;
    let expectedSbeCount = 0;
    if (FeatureFlagUtil.isPresentAndEnabled(db, "ClassicRuntimePlanningForSbe")) {
        expectedSbeCount = 0;
        expectedClassicCount = 2;
    } else {
        expectedSbeCount = 1;
        expectedClassicCount = 1;
    }
    if (multiPlannerMetricsFtdc.sbeCount != expectedSbeCount ||
        multiPlannerMetricsFtdc.classicCount != expectedClassicCount) {
        // This is an indication we haven't retrieve the expected serverStatus metrics yet.
        return false;
    }

    assertSbeMultiPlannerMetrics(
        multiPlannerMetricsFtdc, expectedSbeCount, false /*checkHistograms*/);
    assertClassicMultiPlannerMetrics(
        multiPlannerMetricsFtdc, expectedClassicCount, false /*checkHistograms*/);
    // Verify FTDC omits detailed histograms.
    assert(!multiPlannerMetricsFtdc.hasOwnProperty("histograms"));
    return true;
}, "FTDC output should eventually reflect observed serverStatus metrics.");

MongoRunner.stopMongod(conn);
