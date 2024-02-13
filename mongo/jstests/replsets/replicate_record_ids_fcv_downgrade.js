/**
 * Tests that we cannot downgrade the FCV while there are collections present
 * collection with the 'recordIdsReplicated' flag set.
 *
 * @tags: [
 *   featureFlagRecordIdsReplicated,
 * ]
 */
const replSet = new ReplSetTest({nodes: [{}, {rsConfig: {votes: 0, priority: 0}}]});
replSet.startSet();
replSet.initiate();

const primary = replSet.getPrimary();

const collName = 'replRecIdCollForDowngrade';

const testDB = primary.getDB('test');
testDB.runCommand({create: collName, recordIdsReplicated: true});
const coll = testDB.getCollection(collName);
assert.commandWorked(coll.insert({_id: 1}));

// For the coll the recordId should be in the oplog, and should match the actual recordId on disk.
const primOplog = replSet.findOplog(primary, {ns: coll.getFullName()}).toArray()[0];
const doc = coll.find().showRecordId().toArray()[0];
assert.eq(
    primOplog.rid,
    doc["$recordId"],
    `Mismatching recordIds. Primary's oplog entry: ${tojson(primOplog)}, on disk: ${tojson(doc)}`);

const error = assert.commandFailedWithCode(
    testDB.adminCommand({setFeatureCompatibilityVersion: lastLTSFCV, confirm: true}),
    ErrorCodes.CannotDowngrade);
jsTestLog('Error from failed setFCV command: ' + tojson(error));

// Downgrade should work after dropping the collection.
coll.drop();
assert.commandWorked(
    testDB.adminCommand({setFeatureCompatibilityVersion: lastLTSFCV, confirm: true}));

replSet.stopSet();
