// Test that the in memory sort capacity limit is checked for all "top N" sort candidates.
// SERVER-4716
(function() {
"use strict";

load("jstests/libs/fixture_helpers.js");  // For FixtureHelpers.

const t = db[jsTestName()];
t.drop();

assert.commandWorked(t.createIndex({b: 1}));

let docs = Array.from({length: 100}, (_, i) => {
    return {a: i, b: i};
});
assert.commandWorked(t.insert(docs));

const numShards = FixtureHelpers.numberOfShardsForCollection(t);
const numLargeDocumentsToWrite = 120 * numShards;

jsTestLog('numShards = ' + numShards + '; numLargeDocumentsToWrite = ' + numLargeDocumentsToWrite);

// These large documents will not be part of the initial set of "top 100" matches, and they will
// not be part of the final set of "top 100" matches returned to the client.  However, they are
// an intermediate set of "top 100" matches and should trigger an in memory sort capacity
// exception.
const big = new Array(1024 * 1024).toString();
docs = Array.from({length: numLargeDocumentsToWrite}, (_, i) => {
    const k = 100 + i;
    return {a: k, b: k, big: big};
});
assert.commandWorked(t.insert(docs));

docs = Array.from({length: 100}, (_, i) => {
    const k = 100 + numLargeDocumentsToWrite + i;
    return {a: k, b: k};
});
assert.commandWorked(t.insert(docs));

jsTestLog('Collection ' + t.getFullName() + ' populated with ' + t.countDocuments({}) +
          ' documents. Checking allowDiskUse=false behavior.');

assert.throwsWithCode(
    () => t.find().sort({a: -1}).allowDiskUse(false).hint({b: 1}).limit(100).itcount(),
    ErrorCodes.QueryExceededMemoryLimitNoDiskUseAllowed);
assert.throwsWithCode(
    () =>
        t.find().sort({a: -1}).allowDiskUse(false).hint({b: 1}).showDiskLoc().limit(100).itcount(),
    ErrorCodes.QueryExceededMemoryLimitNoDiskUseAllowed);
})();
