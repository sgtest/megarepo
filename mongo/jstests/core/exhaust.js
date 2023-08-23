// @tags: [
//   requires_getmore,
//   no_selinux,
//   # Exhaust does not use runCommand which is required by the `simulate_atlas_proxy` override.
//   simulate_atlas_proxy_incompatible,
// ]

var coll = db.exhaustColl;
coll.drop();

const docCount = 4;
for (var i = 0; i < docCount; i++) {
    assert.commandWorked(coll.insert({a: i}));
}

// Check that the query works without exhaust set
assert.eq(coll.find().batchSize(1).itcount(), docCount);

// Now try to run the same query with exhaust
assert.eq(coll.find().batchSize(1).addOption(DBQuery.Option.exhaust).itcount(), docCount);

// Test a case where the amount of data requires a response to the initial find operation as well as
// three getMore reply batches.
(function() {
coll.drop();

// Include a long string in each document so that the documents are a bit bigger than 16KB.
const strSize = 16 * 1024;
// The docs are ~16KB and each getMore response is 16MB. Therefore, a full getMore batch will
// contain about 1000 documents. Since the initial find response is limited to only 101 documents,
// by inserting 3000 we ensure that three subsequent getMore replies are required. Roughly speaking,
// the initial reply will consist of the first 100, the first getMore reply 1000 more, then another
// 1000, and then the remaining 900.
const numDocs = 3000;

const str = "A".repeat(strSize);

let bulk = coll.initializeUnorderedBulkOp();
for (let i = 0; i < numDocs; ++i) {
    bulk.insert({key: str});
}
assert.commandWorked(bulk.execute());

assert.eq(numDocs, coll.find().addOption(DBQuery.Option.exhaust).itcount());

// Test that exhaust query with a limit is allowed.
assert.eq(numDocs, coll.find().addOption(DBQuery.Option.exhaust).limit(numDocs + 1).itcount());
assert.eq(numDocs - 1, coll.find().addOption(DBQuery.Option.exhaust).limit(numDocs - 1).itcount());

// Test that exhaust with batchSize and limit is allowed.
assert.eq(
    numDocs,
    coll.find().addOption(DBQuery.Option.exhaust).limit(numDocs + 1).batchSize(100).itcount());
assert.eq(
    numDocs - 1,
    coll.find().addOption(DBQuery.Option.exhaust).limit(numDocs - 1).batchSize(100).itcount());

// Test that exhaust with negative limit is allowed. A negative limit means "single batch": the
// server will return just a single batch and then close the cursor, even if the limit has not yet
// been reached. When the batchSize is not specified explicitly, we expect the default initial batch
// size of 101 to be used.
assert.eq(Math.min(101, TestData.batchSize || Infinity),
          coll.find().addOption(DBQuery.Option.exhaust).limit(-numDocs).itcount());
assert.eq(50,
          coll.find().addOption(DBQuery.Option.exhaust).limit(-numDocs).batchSize(50).itcount());
assert.eq(1, coll.find().addOption(DBQuery.Option.exhaust).limit(-1).itcount());
}());
