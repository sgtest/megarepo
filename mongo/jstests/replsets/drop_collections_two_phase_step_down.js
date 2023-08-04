/**
 * Test to ensure that a primary does not erroneously commit a two phase collection drop if it
 * steps down and back up again before the drop has been committed.
 *
 * Test steps:
 *
 * 1. Pause oplog application on secondary.
 * 2. Drop collection 'collToDrop' on the primary.
 * 3. Force primary to step down.
 * 4. Wait for primary to step up again.
 * 5. Make sure that collection 'collToDrop' has not been physically dropped on the primary.
 * 6. Resume oplog application on secondary and make sure collection drop is eventually committed.
 */

import {TwoPhaseDropCollectionTest} from "jstests/replsets/libs/two_phase_drops.js";

// Set up a two phase drop test.
let testName = "drop_collection_two_phase_step_down";
let dbName = testName;
let collName = "collToDrop";
let twoPhaseDropTest = new TwoPhaseDropCollectionTest(testName, dbName);

// Initialize replica set.
let replTest = twoPhaseDropTest.initReplSet();

// Check for 'system.drop' two phase drop support.
if (!twoPhaseDropTest.supportsDropPendingNamespaces()) {
    jsTestLog('Drop pending namespaces not supported by storage engine. Skipping test.');
    twoPhaseDropTest.stop();
    quit();
}

// Create the collection that will be dropped.
twoPhaseDropTest.createCollection(collName);

// PREPARE collection drop.
twoPhaseDropTest.prepareDropCollection(collName);

// Step primary down using {force: true} and wait for the same node to become primary again.
// We use {force: true} because the current secondary has oplog application disabled and will
// not be able to take over as primary.
try {
    const primary = replTest.getPrimary();
    const primaryId = replTest.getNodeId(primary);

    // Force step down primary.
    jsTestLog('Stepping down primary ' + primary.host + ' with {force: true}.');
    // The amount of time the node has to wait before becoming primary again.
    const stepDownSecs = 1;
    assert.commandWorked(primary.adminCommand({replSetStepDown: stepDownSecs, force: true}));

    // Wait for the node that stepped down to regain PRIMARY status.
    jsTestLog('Waiting for node ' + primary.host + ' to become primary again');
    assert.eq(replTest.nodes[primaryId], replTest.getPrimary());

    jsTestLog('Node ' + primary.host + ' is now PRIMARY again. Checking if drop-pending' +
              ' collection still exists.');
    assert(twoPhaseDropTest.collectionIsPendingDrop(collName),
           'After stepping down and back up again, the primary ' + primary.host +
               ' removed drop-pending collection unexpectedly');

    // COMMIT collection drop.
    twoPhaseDropTest.commitDropCollection(collName);
} finally {
    twoPhaseDropTest.stop();
}