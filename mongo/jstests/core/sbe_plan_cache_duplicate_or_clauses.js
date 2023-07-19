/**
 * This test was originally designed to reproduce SERVER-78752. It ensures that the correct plan is
 * added to the cache for similar queries that only differ in their IETs and if there is a
 * dependency between the values of the $or expression
 *
 * @tags: [
 *   assumes_unsharded_collection,
 *   # Plan cache state is node-local and will not get migrated alongside tenant data.
 *   tenant_migration_incompatible,
 *   cqf_incompatible,
 * ]
 */
(function() {
"use strict";

const coll = db.sbe_plan_cache_duplicate_or_clauses;
coll.drop();
assert.commandWorked(coll.createIndex({a: 1, b: 1, c: 1}));

assert.commandWorked(coll.insert({a: 1, b: 1, c: 1}));
assert.commandWorked(coll.insert({a: 1, b: 1, c: 2}));
assert.commandWorked(coll.insert({a: 1, b: 1, c: 3}));

// Show that this query returns 2 results when the plan cache is empty.
assert.eq(2, coll.find({a: 1, b: 1, $or: [{c: 2}, {c: 3, d: {$eq: null}}]}).itcount());

// Create a cached plan. Run the query twice to make sure that the plan cache entry is active.
assert.eq(1, coll.find({a: 1, b: 1, $or: [{c: 1}, {c: 1, d: {$eq: null}}]}).itcount());
assert.eq(1, coll.find({a: 1, b: 1, $or: [{c: 1}, {c: 1, d: {$eq: null}}]}).itcount());

// Check that we have 2 distinct plans in the cache.
let cacheEntries = coll.getPlanCache().list();
assert.eq(2, cacheEntries.length);

// The query from above should still return 2 results.
assert.eq(2, coll.find({a: 1, b: 1, $or: [{c: 2}, {c: 3, d: {$eq: null}}]}).itcount());
}());