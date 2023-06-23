/**
 * This test ensures that hint on the distinct command works.
 *
 * @tags: [
 *  assumes_unsharded_collection,
 *  requires_fcv_71,
 * ]
 */

load("jstests/libs/analyze_plan.js");

const collName = "jstests_explain_distinct_hint";
const coll = db[collName];

coll.drop();

// Insert the data to perform distinct() on.
assert.commandWorked(db.coll.insert({a: 1, b: 2}));
assert.commandWorked(db.coll.insert({a: 1, b: 2, c: 3}));
assert.commandWorked(db.coll.insert({a: 2, b: 2, d: 3}));
assert.commandWorked(db.coll.insert({a: 1, b: 2}));
assert.commandWorked(db.coll.createIndex({a: 1}));
assert.commandWorked(db.coll.createIndex({b: 1}));
assert.commandWorked(db.coll.createIndex({x: 1}, {sparse: true}));

// Use .explain() to make sure the index we specify is being used when we use a hint.
let explain = db.coll.explain().distinct("a", {a: 1, b: 2});
assert.eq(getPlanStage(explain, "IXSCAN").indexName, "a_1");

explain = db.coll.explain().distinct("a", {a: 1, b: 2}, {hint: {b: 1}});
let ixScanStage = getPlanStage(explain, "IXSCAN");
assert(ixScanStage, tojson(explain));
assert.eq(ixScanStage.indexName, "b_1", tojson(ixScanStage));
assert.eq(explain.command.hint, {"b": 1});

explain = db.coll.explain().distinct("a", {a: 1, b: 2}, {hint: "b_1"});
ixScanStage = getPlanStage(explain, "IXSCAN");
assert(ixScanStage, tojson(explain));
assert.eq(ixScanStage.indexName, "b_1");
assert.eq(explain.command.hint, "b_1");

// Make sure the hint produces the right values when the query is run.
let cmdObj = db.coll.runCommand("distinct", {"key": "a", query: {a: 1, b: 2}, hint: {a: 1}});
assert.eq(1, cmdObj.values);

cmdObj = db.coll.runCommand("distinct", {"key": "a", query: {a: 1, b: 2}, hint: "a_1"});
assert.eq(1, cmdObj.values);

cmdObj = db.coll.runCommand("distinct", {"key": "a", query: {a: 1, b: 2}, hint: {b: 1}});
assert.eq(1, cmdObj.values);

cmdObj = db.coll.runCommand("distinct", {"key": "a", query: {a: 1, b: 2}, hint: {x: 1}});
assert.eq([], cmdObj.values);

cmdObj = db.coll.runCommand("distinct", {"key": "a", query: {a: 1, b: 2}, hint: "x_1"});
assert.eq([], cmdObj.values);

assert.throws(function() {
    db.coll.explain().distinct("a", {a: 1, b: 2}, {hint: {bad: 1, hint: 1}});
});

assert.throws(function() {
    db.coll.explain().distinct("a", {a: 1, b: 2}, {hint: "BAD HINT"});
});

let cmdRes =
    db.coll.runCommand("distinct", {"key": "a", query: {a: 1, b: 2}, hint: {bad: 1, hint: 1}});
assert.commandFailedWithCode(cmdRes, ErrorCodes.BadValue, cmdRes);
var regex = new RegExp("hint provided does not correspond to an existing index");
assert(regex.test(cmdRes.errmsg));
