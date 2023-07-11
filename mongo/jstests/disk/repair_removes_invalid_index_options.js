/**
 * Tests that starting mongod with --repair can remove invalid options from an index specification.
 */

import {assertRepairSucceeds, startMongodOnExistingPath} from "jstests/disk/libs/wt_file_helper.js";
load("jstests/libs/fail_point_util.js");

const baseName = "repair_removes_invalid_index_options";
const collName = "test";
const dbPath = MongoRunner.dataPath + baseName + "/";

resetDbpath(dbPath);

// Helper function to initialize a test collection with an index specification containing options
// that are not in the list of allowedFieldNames.
const createCollWithInvalidIndex = function(coll) {
    assert.commandWorked(coll.runCommand({
        createIndexes: collName,
        indexes: [{key: {a: 1}, name: 'a_1', invalidIndexOption: 1, anotherInvalidIndexOption: 0}]
    }));
    assert.eq(2, coll.getIndexes().length);
};

let port;

// Start a standalone mongod and insert an index spec that contains an invalid index, using a
// failpoint to bypass validation checks.
(function startStandaloneWithInvalidIndexSpec() {
    jsTestLog("Entering startStandaloneWithInvalidIndexSpec....");
    const mongod = startMongodOnExistingPath(dbPath);
    port = mongod.port;
    const db = mongod.getDB(baseName);
    let testColl = db[collName];

    configureFailPoint(db, "skipIndexCreateFieldNameValidation", {}, "alwaysOn");

    testColl = createCollWithInvalidIndex(testColl);

    MongoRunner.stopMongod(mongod);

    jsTestLog("Exiting startStandaloneWithInvalidIndexSpec.");
})();

/**
 * Test 1: Verify that restarting mongod without --repair will crash due to the invalid options in
 * the index specification.
 */
(function restartAndVerifyMongodCrashes() {
    jsTestLog("Entering restartAndVerifyMongodCrashes....");
    assert.throws(() => {
        startMongodOnExistingPath(dbPath);
    });

    assert(rawMongoProgramOutput().match("Fatal assertion.*28782"),
           "Mongod should have aborted due to an invalid index descriptor.");
    jsTestLog("Exiting restartAndVerifyMongodCrashes.");
})();

/**
 * Test 2: Verify that restarting mongod with --repair will remove the invalid index options.
 */
(function runRepairAndVerifyIndexIsRepaired() {
    jsTestLog("Entering runRepairAndVerifyIndexIsRepaired....");

    // Repair the index spec.
    assertRepairSucceeds(dbPath, port, {});

    const mongod = startMongodOnExistingPath(dbPath);
    const testColl = mongod.getDB(baseName)[collName];

    // Check that the invalid options are removed.
    const res = testColl.runCommand({listIndexes: collName});
    assert.commandWorked(res);
    assert.eq(false, res["cursor"]["firstBatch"][1].hasOwnProperty("invalidIndexOption"));
    assert.eq(false, res["cursor"]["firstBatch"][1].hasOwnProperty("anotherInvalidIndexOption"));

    const validateRes = assert.commandWorked(testColl.runCommand({validate: collName}));
    assert(validateRes.valid,
           "--repair should have removed all invalid index options." + tojson(validateRes));

    MongoRunner.stopMongod(mongod);
    jsTestLog("Exiting runRepairAndVerifyIndexIsRepaired.");
})();
