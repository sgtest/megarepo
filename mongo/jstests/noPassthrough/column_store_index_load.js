/**
 * Test that different methods of loading a column store index all produce the same valid results.
 * Indexes are validated by comparing query results that use the index with results from a control
 * query that uses a collection scan.
 * @tags: [
 *   # We could potentially need to resume an index build in the event of a stepdown, which is not
 *   # yet implemented.
 *   does_not_support_stepdowns,
 *   featureFlagColumnstoreIndexes,
 * ]
 */
import {planHasStage} from "jstests/libs/analyze_plan.js";
import {setUpServerForColumnStoreIndexTest} from "jstests/libs/columnstore_util.js";

const mongod = MongoRunner.runMongod({});
const db = mongod.getDB("test");

if (!setUpServerForColumnStoreIndexTest(db)) {
    MongoRunner.stopMongod(mongod);
    quit();
}

//
// Create test documents.
//

const testValues = [
    {foo: 1, bar: 2},
    {bar: 2, baz: 3},
    {foo: 3, baz: 4},
    {foo: 5, bar: 6},
    {bar: 7, baz: [7, 8]},
];

// We create our test documents by choosing k-permutations from the 'testValues' array. The
// kPermutations() function returns an array of all possible permutations with 'k' choices from all
// values in the 'arr' input. The 'choices' input stores the values chosen at previous levels of
// recursion.
function kPermutations(arr, n, choices = []) {
    if (n == 0) {
        return [choices];
    }

    const permutations = [];
    for (let i = 0; i < arr.length; ++i) {
        const subSequence = arr.slice(0, i).concat(arr.slice(i + 1));
        permutations.push(...kPermutations(subSequence, n - 1, choices.concat([arr[i]])));
    }
    return permutations;
}

const testDocs = kPermutations(testValues, 4).map((permutation, idx) => ({
                                                      idx: idx,
                                                      foo: [permutation[0], permutation[1]],
                                                      bar: [permutation[2], permutation[3]]
                                                  }));

// Compute the number of paths that this document will add to the column store index, assuming it is
// structured according to the above testDocs template that generates two-level documents.
let numPathsToProcess = 0;
for (const doc of testDocs) {
    numPathsToProcess += 5;  // _id, idx, foo, bar, special dense field

    for (const arr of [doc.foo, doc.bar]) {
        let keys = {};
        for (const obj of arr) {
            for (const field of Object.keys(obj)) {
                keys[field] = true;
            }
        }
        numPathsToProcess += Object.keys(keys).length;
    }
}

// Test queries use a projection that includes every possible leaf field. Projections on fields that
// have sub-documents fall back to the row store, which would not serve to validate the contents of
// the index.
const testProjection = {
    _id: 0,
    idx: 1,
    "foo.foo": 1,
    "foo.bar": 1,
    "foo.baz": 1,
    "bar.foo": 1,
    "bar.bar": 1,
    "bar.baz": 1,
};

const maxMemUsageBytes = 20000;
const numDocs = testDocs.length;
const approxDocSize = 500;
const approxMemoryUsage = numDocs * approxDocSize;
const expectedSpilledRanges = Math.ceil(approxMemoryUsage / maxMemUsageBytes);

// The test query would normally not qualify for a column store index plan, because it projects a
// large number of fields. We raise the limit on the number of fields to allow column store plans
// for the purposes of this test.
db.adminCommand({
    setParameter: 1,
    internalQueryMaxNumberOfFieldsToChooseUnfilteredColumnScan: Object.keys(testProjection).length
});

function loadDocs(coll, documents) {
    const bulk = coll.initializeUnorderedBulkOp();
    for (const doc of documents) {
        bulk.insert(doc);
    }
    assert.commandWorked(bulk.execute());
}

//
// We load the same documents into 4 collections:
//
//   1. a control collection with no index,
const noIndexColl = db.column_store_index_load_no_index;

//   2. a collection whose column store index is populated with an in-memory bulk load,
const bulkLoadInMemoryColl = db.column_store_index_load_in_memory;

//   3. a collection whose column store index is populated with a bulk load that uses an external
//   merge sort (i.e., one that "spills" to disk), and
const bulkLoadExternalColl = db.column_store_index_load_external;

//   4. a collection whose column store index is populated as documents are inserted.
const onlineLoadColl = db.column_store_index_online_load;

// Load the control collection.
noIndexColl.drop();
loadDocs(noIndexColl, testDocs);

// Perform the in-memory bulk load.
bulkLoadInMemoryColl.drop();
loadDocs(bulkLoadInMemoryColl, testDocs);
assert.commandWorked(bulkLoadInMemoryColl.createIndex({"$**": "columnstore"}));

const statsAfterInMemoryBuild = assert.commandWorked(db.runCommand({serverStatus: 1}));
let indexBulkBuilderSection = statsAfterInMemoryBuild.indexBulkBuilder;
assert.eq(indexBulkBuilderSection.count, 1, tojson(indexBulkBuilderSection));
assert.eq(indexBulkBuilderSection.resumed, 0, tojson(indexBulkBuilderSection));
assert.eq(indexBulkBuilderSection.filesOpenedForExternalSort, 0, tojson(indexBulkBuilderSection));
assert.eq(indexBulkBuilderSection.filesClosedForExternalSort, 0, tojson(indexBulkBuilderSection));
assert.eq(indexBulkBuilderSection.spilledRanges, 0, tojson(indexBulkBuilderSection));
assert.eq(indexBulkBuilderSection.bytesSpilledUncompressed, 0, tojson(indexBulkBuilderSection));
assert.eq(indexBulkBuilderSection.bytesSpilled, 0, tojson(indexBulkBuilderSection));
assert.eq(indexBulkBuilderSection.numSorted, numPathsToProcess, tojson(indexBulkBuilderSection));
assert.between(0.7 * approxMemoryUsage,
               indexBulkBuilderSection.bytesSorted,
               1.3 * approxMemoryUsage,
               tojson(indexBulkBuilderSection));
assert.between(0.7 * approxMemoryUsage,
               indexBulkBuilderSection.memUsage,
               1.3 * approxMemoryUsage,
               tojson(indexBulkBuilderSection),
               /*inclusive=*/ true);
assert.eq(Object.keys(indexBulkBuilderSection).length, 10, tojson(indexBulkBuilderSection));

// Perform the external bulk load. The server config won't allow a memory limit lower than 50MB, so
// we use a failpoint to set it lower than that for the purposes of this test.
bulkLoadExternalColl.drop();
assert.commandWorked(db.adminCommand({
    configureFailPoint: "constrainMemoryForBulkBuild",
    mode: "alwaysOn",
    data: {maxBytes: maxMemUsageBytes},
}));
loadDocs(bulkLoadExternalColl, testDocs);
assert.commandWorked(bulkLoadExternalColl.createIndex({"$**": "columnstore"}));

const statsAfterExternalLoad = assert.commandWorked(db.runCommand({serverStatus: 1}));
indexBulkBuilderSection = statsAfterExternalLoad.indexBulkBuilder;
assert.eq(indexBulkBuilderSection.count, 2, tojson(indexBulkBuilderSection));
assert.eq(indexBulkBuilderSection.resumed, 0, tojson(indexBulkBuilderSection));
assert.eq(indexBulkBuilderSection.filesOpenedForExternalSort, 1, tojson(indexBulkBuilderSection));
assert.eq(indexBulkBuilderSection.filesClosedForExternalSort, 1, tojson(indexBulkBuilderSection));
// Note: The number of spills in the external sorter depends on the size of C++ data structures,
// which can be different between architectures. The test allows a range of reasonable values.
assert.between(expectedSpilledRanges - 1,
               indexBulkBuilderSection.spilledRanges,
               expectedSpilledRanges + 1,
               tojson(indexBulkBuilderSection),
               /*inclusive=*/ true);
// We can only approximate the memory usage and bytes that will be spilled.
assert.between(0,
               indexBulkBuilderSection.bytesSpilled,
               approxMemoryUsage,
               tojson(indexBulkBuilderSection),
               /*inclusive=*/ true);
assert.gte(indexBulkBuilderSection.bytesSpilledUncompressed,
           indexBulkBuilderSection.bytesSpilled,
           tojson(indexBulkBuilderSection));
// Multiply expected values by 2 to account for the previous index build.
assert.eq(
    indexBulkBuilderSection.numSorted, numPathsToProcess * 2, tojson(indexBulkBuilderSection));
assert.between(approxMemoryUsage * 0.7 * 2,
               indexBulkBuilderSection.bytesSorted,
               approxMemoryUsage * 1.3 * 2,
               tojson(indexBulkBuilderSection));
assert.between(approxMemoryUsage * 0.7,
               indexBulkBuilderSection.memUsage,
               approxMemoryUsage * 1.3,
               tojson(indexBulkBuilderSection),
               /*inclusive=*/ true);

// Perfom the online load.
onlineLoadColl.drop();
onlineLoadColl.createIndex({"$**": "columnstore"});
loadDocs(onlineLoadColl, testDocs);

//
// Verify that our test query uses the column store.
//
[bulkLoadInMemoryColl, bulkLoadExternalColl, onlineLoadColl].forEach(function(coll) {
    const explain = coll.find({}, testProjection).sort({idx: 1}).explain();
    assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);
});

//
// Run a query on each of the test collections, including the "no index" control collection.
//
const noIndexResults = noIndexColl.find({}, testProjection).sort({idx: 1}).toArray();
const bulkLoadInMemoryResults =
    bulkLoadInMemoryColl.find({}, testProjection).sort({idx: 1}).toArray();
const bulkLoadExternalResults =
    bulkLoadExternalColl.find({}, testProjection).sort({idx: 1}).toArray();
const onlineLoadResults = onlineLoadColl.find({}, testProjection).sort({idx: 1}).toArray();

//
// Verify that the test query produces the same results in all test configurations.
//
assert.eq(testDocs.length, noIndexResults.length);
assert.eq(testDocs.length, bulkLoadInMemoryResults.length);
assert.eq(testDocs.length, bulkLoadExternalResults.length);
assert.eq(testDocs.length, onlineLoadResults.length);

for (let i = 0; i < noIndexResults.length; ++i) {
    assert.docEq(noIndexResults[i], bulkLoadInMemoryResults[i]);
    assert.docEq(noIndexResults[i], bulkLoadExternalResults[i]);
    assert.docEq(noIndexResults[i], onlineLoadResults[i]);
}

MongoRunner.stopMongod(mongod);
