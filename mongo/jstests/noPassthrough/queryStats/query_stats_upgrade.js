/**
 * Test that telemetry doesn't work on a lower FCV version but works after an FCV upgrade.
 * @tags: [featureFlagQueryStats]
 */
import {FeatureFlagUtil} from "jstests/libs/feature_flag_util.js";

const dbpath = MongoRunner.dataPath + jsTestName();
let conn = MongoRunner.runMongod({dbpath: dbpath});
let testDB = conn.getDB(jsTestName());
// This test should only be run with the flag enabled.
assert(FeatureFlagUtil.isEnabled(testDB, "QueryStats"));

function testLower(restart = false) {
    let adminDB = conn.getDB("admin");
    assert.commandWorked(adminDB.runCommand(
        {setFeatureCompatibilityVersion: binVersionToFCV("last-lts"), confirm: true}));
    if (restart) {
        MongoRunner.stopMongod(conn);
        conn = MongoRunner.runMongod({dbpath: dbpath, noCleanData: true});
        testDB = conn.getDB(jsTestName());
        adminDB = conn.getDB("admin");
    }

    assert.commandFailedWithCode(
        testDB.adminCommand({aggregate: 1, pipeline: [{$queryStats: {}}], cursor: {}}), 6579000);

    // Upgrade FCV.
    assert.commandWorked(adminDB.runCommand(
        {setFeatureCompatibilityVersion: binVersionToFCV("latest"), confirm: true}));

    // We should be able to run a telemetry pipeline now that the FCV is correct.
    assert.commandWorked(
        testDB.adminCommand({aggregate: 1, pipeline: [{$queryStats: {}}], cursor: {}}),
    );
}
testLower(true);
testLower(false);
MongoRunner.stopMongod(conn);