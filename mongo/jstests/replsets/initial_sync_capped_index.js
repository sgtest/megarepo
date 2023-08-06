/**
 * Test to ensure that initial sync builds indexes correctly when syncing a capped collection that
 * is receiving concurrent inserts.
 *
 * The main goal of this test is to have the SECONDARY clone more documents than would actually fit
 * in a specific capped collection, leading to the deletion of documents (i.e. 'capping') on the
 * SECONDARY *during* the collection cloning process. This scenario is encountered when a SECONDARY
 * opens a cursor on a capped collection, begins iterating on that cursor, and, before the cursor is
 * exhausted, new documents get appended to the capped collection that it is cloning.
 *
 * Test Setup:
 * 1-node replica set that is reconfigured to a 2-node replica set.
 *
 * 1. Initiate replica set.
 * 2. Create a capped collection on the PRIMARY and overflow it.
 * 4. Add a SECONDARY node to the replica set.
 * 5. Set fail point on SECONDARY that hangs capped collection clone after first 'find' response.
 * 6. Let SECONDARY start initial sync.
 * 7. Wait for initial 'find' response during the cloning of the capped collection.
 * 8. Insert documents to the capped collection on the PRIMARY.
 * 9, Disable fail point on SECONDARY so the rest of the capped collection documents are cloned.
 * 8. Once initial sync completes, ensure that capped collection indexes on the SECONDARY are valid.
 *
 * This is a regression test for SERVER-29197.
 *
 * @tags: [
 *   uses_full_validation,
 * ]
 */
import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {waitForState} from "jstests/replsets/rslib.js";

/**
 * Overflow a capped collection 'coll' by continuously inserting a given document,
 * 'docToInsert'.
 */
function overflowCappedColl(coll, docToInsert) {
    // Insert one document and save its _id.
    assert.commandWorked(coll.insert(docToInsert));
    var origFirstDocId = coll.findOne()["_id"];

    // Detect overflow by seeing if the original first doc of the collection is still present.
    while (coll.findOne({_id: origFirstDocId})) {
        assert.commandWorked(coll.insert(docToInsert));
    }
}

// Set up replica set.
var testName = "initial_sync_capped_index";
var dbName = testName;
var replTest = new ReplSetTest({name: testName, nodes: 1});
replTest.startSet();
replTest.initiate();

var primary = replTest.getPrimary();
var primaryDB = primary.getDB(dbName);
var cappedCollName = "capped_coll";
var primaryCappedColl = primaryDB[cappedCollName];

// Create a capped collection of the minimum allowed size.
var cappedCollSize = 4096;

jsTestLog("Creating capped collection of size " + cappedCollSize + " bytes.");
assert.commandWorked(
    primaryDB.createCollection(cappedCollName, {capped: true, size: cappedCollSize}));

// Overflow the capped collection.
jsTestLog("Overflowing the capped collection.");

var docSize = cappedCollSize / 8;
var largeDoc = {a: new Array(docSize).join("*")};
overflowCappedColl(primaryCappedColl, largeDoc);

// Check that there are more than two documents in the collection. This will ensure the
// secondary's collection cloner will send a getMore.
assert.gt(primaryCappedColl.find().itcount(), 2);

// Add a SECONDARY node. It should use batchSize=2 for its initial sync queries.
jsTestLog("Adding secondary node.");
replTest.add({rsConfig: {votes: 0, priority: 0}, setParameter: "collectionClonerBatchSize=2"});

var secondary = replTest.getSecondary();
var collectionClonerFailPoint = "initialSyncHangCollectionClonerAfterHandlingBatchResponse";

// Make the collection cloner pause after its initial 'find' response on the capped collection.
var nss = dbName + "." + cappedCollName;
jsTestLog("Enabling collection cloner fail point for " + nss);
let failPoint = configureFailPoint(secondary, collectionClonerFailPoint, {nss: nss});

// Let the SECONDARY begin initial sync.
jsTestLog("Re-initiating replica set with new secondary.");
replTest.reInitiate();

jsTestLog("Waiting for the initial 'find' response of capped collection cloner to complete.");
failPoint.wait();

// Append documents to the capped collection so that the SECONDARY will clone these
// additional documents.
var docsToAppend = 2;
for (var i = 0; i < docsToAppend; i++) {
    assert.commandWorked(primaryDB[cappedCollName].insert(largeDoc));
}

// Let the 'getMore' requests for the capped collection clone continue.
jsTestLog("Disabling collection cloner fail point for " + nss);
failPoint.off();

// Wait until initial sync completes.
replTest.awaitReplication();

// Before validating the secondary, confirm that it is in the SECONDARY state. Otherwise, the
// validate command will fail.
waitForState(secondary, ReplSetTest.State.SECONDARY);

// Make sure the indexes created during initial sync are valid.
var secondaryCappedColl = secondary.getDB(dbName)[cappedCollName];
var validate_result = secondaryCappedColl.validate({full: true});
var failMsg =
    "Index validation of '" + secondaryCappedColl.name + "' failed: " + tojson(validate_result);
assert(validate_result.valid, failMsg);
replTest.stopSet();
