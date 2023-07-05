/**
 * Tests the eligibility of certain queries to use a columnstore index.
 * @tags: [
 *   # Column store indexes are still under a feature flag.
 *   featureFlagColumnstoreIndexes,
 *   # Refusing to run a test that issues an aggregation command with explain because it may return
 *   # incomplete results if interrupted by a stepdown.
 *   does_not_support_stepdowns,
 *   # Cannot run aggregate with explain in a transaction.
 *   does_not_support_transactions,
 *   # Columnstore tests set server parameters to disable columnstore query planning heuristics -
 *   # server parameters are stored in-memory only so are not transferred onto the recipient.
 *   tenant_migration_incompatible,
 *   not_allowed_with_security_token,
 *   # Logic for when a COLUMN_SCAN plan is generated changed slightly as part of enabling more
 *   # queries in SBE in the 7.0 release.
 *   requires_fcv_70,
 * ]
 */
import {aggPlanHasStage, planHasStage} from "jstests/libs/analyze_plan.js";
import {setUpServerForColumnStoreIndexTest} from "jstests/libs/columnstore_util.js";
load("jstests/libs/fixture_helpers.js");  // For FixtureHelpers.isMongos.

if (!setUpServerForColumnStoreIndexTest(db)) {
    quit();
}

const coll = db.columnstore_eligibility;
coll.drop();

assert.commandWorked(coll.createIndex({"$**": "columnstore"}));
assert.commandWorked(coll.insert({_id: 0, x: 1, y: 1, dummyData: true}));

let explain = coll.find({}, {x: 1}).explain();
assert(planHasStage(db, explain, "COLUMN_SCAN"));

//
// Examples extracted from the project's design doc.
//

// Column index can be used to just scan the 'a' column. Note it will also have to
// scan The "RowId Column" to find any documents missing 'a'. The column index is not performing the
// grouping operation, it is simply providing an optimized data access layer.
explain = coll.explain().aggregate([{$group: {_id: "$a"}}]);
assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);

// $match+$group queries are expected to be the typical use case for a column index. The predicate
// can be applied to the 'a' values directly in the index, but the index is not sorted on the 'a'
// values, so this will still need to scan the entire 'a' range.
explain = coll.explain().aggregate([{$match: {a: 2}}, {$group: {_id: "$b.c"}}]);
assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);
explain = coll.explain().aggregate([{$match: {a: {$gte: 2}}}, {$group: {_id: "$b.c"}}]);
assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);
explain = coll.explain().aggregate([{$match: {a: {$in: [2, 3, 4]}}}, {$group: {_id: "$b.c"}}]);
assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);

// The column index can also be used for find() queries.

// No filter, just projection.
explain = coll.find({}, {_id: 0, a: 1, b: 1}).explain();
assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);

// Filter is not eligible for use during column scan, but set of fields is limited enough. Filter
// will be applied after assembling an intermediate result containing both "a" and "b".
explain = coll.find({$or: [{a: 2}, {b: 2}]}, {_id: 0, a: 1}).explain();

// COLUMN_SCAN is used for top-level $or queries.
assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);

explain = coll.explain().aggregate([
    {$match: {$or: [{a: {$gt: 0}}, {b: {$gt: 0}}]}},
    {$project: {_id: 0, computedField: {$add: ["$a", "$b"]}}},
]);
assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);

explain = coll.explain().aggregate([
    {$match: {$or: [{a: {$gt: 0}}, {b: {$gt: 0}}]}},
    {$project: {_id: 0, computedField: {$add: ["$a", "$b"]}}},
    {$group: {_id: "$computedField"}}
]);
assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);

// Simplest case: just scan "a" column.
explain = coll.find({a: {$exists: true}}, {_id: 0, a: 1}).explain();
assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);

// Scan the "a" column with a predicate.
explain = coll.find({a: 2}, {_id: 0, a: 1}).explain();
assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);

// A complex predicate.
explain = coll.find({
                  $and: [
                      {a: {$gt: 2, $mod: [2, 0]}},
                      {$or: [{a: {$lte: 0}}, {b: 42}]},
                      {a: {$type: "number"}},
                      {b: {$in: [0, 3, []]}}
                  ]
              },
                    {_id: 1})
              .explain();
assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);

// Scan the "a.b" column with a predicate. Dotted paths are supported even if there are arrays
// encountered. See IS_SPARSE Encoding for more details.
explain = coll.find({'a.b': 2}, {_id: 0, 'a.b': 1}).explain();
assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);

// 'aggregate' command is supported.
explain = coll.explain().aggregate([{$match: {a: 2}}, {$project: {_id: 0, a: 1}}]);
assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);

//
// Now test some queries which are NOT eligible to use a columnstore index.
//
explain = coll.find({a: 2}).explain();  // No projection.
assert(!planHasStage(db, explain, "COLUMN_SCAN"), explain);

// Any other index should be preferred over a columnstore index.
assert.commandWorked(coll.createIndex({a: 1}));
explain = coll.find({a: 1}, {a: 1, b: 1, c: 1}).explain();
assert(!planHasStage(db, explain, "COLUMN_SCAN"), explain);
assert(planHasStage(db, explain, "IXSCAN"), explain);
assert.commandWorked(coll.dropIndex({a: 1}));

// Referenced more than internalQueryMaxNumberOfFieldsToChooseUnfilteredColumnScan fields (default
// 5) - note _id is implicitly included in this projection, causing this query to need 6 fields
explain = coll.find({}, {f0: 1, f1: 1, f2: 1, f3: 1, f4: 1}).explain();
assert(!planHasStage(db, explain, "COLUMN_SCAN"), explain);

// There was a query predicate which can be filtered during a columnstore index scan, but the query
// referenced more than internalQueryMaxNumberOfFieldsToChooseFilteredColumnScan fields (default 12)
explain = coll.find({f0: {$eq: "target"}}, {
                  f0: 1,
                  f1: 1,
                  f2: 1,
                  f3: 1,
                  f4: 1,
                  f5: 1,
                  f6: 1,
                  f7: 1,
                  f8: 1,
                  f9: 1,
                  f10: 1,
                  f11: 1,
                  f12: 1
              })
              .explain();
assert(!planHasStage(db, explain, "COLUMN_SCAN"), explain);

// Same as above - the fields here are split across the query and the projection, but there are
// still >12 needed to answer the query.
explain = coll.find({
                  f0: {$eq: "target"},
                  f1: {$eq: "target"},
                  f2: {$eq: "target"},
                  f3: {$eq: "target"},
                  f4: {$eq: "target"},
              },
                    {f3: 1, f4: 1, f5: 1, f6: 1, f7: 1, f8: 1, f9: 1, f10: 1, f11: 1, f12: 1})
              .explain();
assert(!planHasStage(db, explain, "COLUMN_SCAN"), explain);

// The query depends on the 'comments' field, which has sub objects.  There is a useful operation
// that could use the column index: a "parallel unwind" on comments.author and comments.views. No
// such operator exists in MQL today, however.
explain = coll.explain().aggregate([
    {$unwind: "$comments"},
    {$group: {_id: "$comments.author", total_views: {$sum: "$comments.views"}}}
]);
assert(!planHasStage(db, explain, "COLUMN_SCAN"), explain);

// SBE is not supported for update operations. Also this update would require the whole document.
// Be sure to update by _id to preseve the sharded collection passthrough coverage. Targeting by
// shard key is required for non-multi updates.
explain = coll.explain().update({_id: 0, a: 2}, {$set: {b: 2}});
assert(!planHasStage(db, explain, "COLUMN_SCAN"), explain);

// SBE is not supported for delete operations.
explain = coll.explain().remove({a: 2});
assert(!planHasStage(db, explain, "COLUMN_SCAN"), explain);

// SBE is not supported for the count command.
explain = coll.explain().find({a: 2}).count();
assert(!planHasStage(db, explain, "COLUMN_SCAN"), explain);

// Count-like queries that can use another index should prefer that to column scan.
assert.commandWorked(coll.createIndex({a: 1}));
explain = coll.explain().aggregate([{$match: {a: 1}}, {$count: "count"}]);
assert(aggPlanHasStage(explain, "COUNT_SCAN") || aggPlanHasStage(explain, "IXSCAN"), explain);
assert.commandWorked(coll.dropIndex({a: 1}));

// CSI Can never be used for a filter/$match on numeric components because that represents a
// filter over 2^k columns, where k is the number of numeric components. E.g.
// find({a.0.b.1: 123}) represents 4 paths:
//
// {a: {0: {b: {1: 123}}}}
// {a: [{b: {1: 123}}]}
// {a: {0: {b: [<anything>, 123]}}}
// {a: [{b: [<anything>, 123]}]}
//
// Today we're guaranteed CSI will never be used when numeric components are present in a
// filter because SBE does not support querying on such paths.
explain = coll.find({'a.0': 2}, {_id: 0, 'a.b': 1}).explain();
assert(!planHasStage(db, explain, "COLUMN_SCAN"), explain);

explain = coll.find({'a.0.b.1': 2}, {_id: 0, 'a.b': 1}).explain();
assert(!planHasStage(db, explain, "COLUMN_SCAN"), explain);

// CSI _can_ be used when there's a numeric path component used in a projection. This
// is because MQL projection treats numeric components only as object keys, not array indexes.
explain = coll.find({'a.b': 2}, {_id: 0, 'a.0': 1}).explain();
assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);

explain = coll.find({'a.b': 2}, {_id: 0, 'a.0.b.123': 1}).explain();
assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);

// Test that a column store index on a subpath can be used and hinted when it covers the query.
const subpath_idx_coll = db.columnstore_eligibility_subpath;
subpath_idx_coll.drop();
assert.commandWorked(subpath_idx_coll.insert({_id: 0, a: 1, b: 2}));
assert.commandWorked(subpath_idx_coll.createIndex({"a.$**": "columnstore"}));

// Index covers query, can be used.
// Note that this is only applicable in non-sharded environments, as the index will not be able to
// cover the query if we need the shard key.
explain = subpath_idx_coll.find({a: 1}, {_id: 0, a: 1}).explain();
assert(planHasStage(db, explain, "COLUMN_SCAN") || FixtureHelpers.isMongos(db), explain);

// Index does not cover query.
explain = subpath_idx_coll.find({b: 1}, {_id: 0, b: 1}).explain();
assert(!planHasStage(db, explain, "COLUMN_SCAN"), explain);

// Test hinting the subpath index. Sanity check - should use a traditional index without a hint.
assert.commandWorked(subpath_idx_coll.createIndex({a: 1}));
explain = subpath_idx_coll.find({a: 1}, {_id: 0, a: 1}).explain();
assert(!planHasStage(db, explain, "COLUMN_SCAN"), explain);

// Hint the subpath index.
if (!FixtureHelpers.isMongos(db)) {
    explain =
        subpath_idx_coll.find({a: 1}, {_id: 0, a: 1}).hint({"a.$**": "columnstore"}).explain();
    assert(planHasStage(db, explain, "COLUMN_SCAN"), explain);
}

// Hint when subpath index doesn't cover query.
assert.commandFailedWithCode(db.runCommand({
    find: subpath_idx_coll.getName(),
    filter: {b: 1},
    projection: {_id: 0, b: 1},
    hint: {"a.$**": "columnstore"}
}),
                             6714002);
