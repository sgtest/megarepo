// @tags: [
//   assumes_balancer_off,
//   requires_multi_updates,
//   requires_non_retryable_writes,
// ]
// Include helpers for analyzing explain output.
import {
    getOptimizer,
    getWinningPlan,
    isExpress,
    isIdhackOrExpress
} from "jstests/libs/analyze_plan.js";
import {checkSbeFullyEnabled} from "jstests/libs/sbe_util.js";

const t = db.idhack;
t.drop();

assert.commandWorked(t.insert({_id: {x: 1}, z: 1}));
assert.commandWorked(t.insert({_id: {x: 2}, z: 2}));
assert.commandWorked(t.insert({_id: {x: 3}, z: 3}));
assert.commandWorked(t.insert({_id: 1, z: 4}));
assert.commandWorked(t.insert({_id: 2, z: 5}));
assert.commandWorked(t.insert({_id: 3, z: 6}));

assert.eq(2, t.findOne({_id: {x: 2}}).z);
assert.eq(2, t.find({_id: {$gte: 2}}).count());
assert.eq(2, t.find({_id: {$gte: 2}}).itcount());

t.update({_id: {x: 2}}, {$set: {z: 7}});
assert.eq(7, t.findOne({_id: {x: 2}}).z);

t.update({_id: {$gte: 2}}, {$set: {z: 8}}, false, true);
assert.eq(4, t.findOne({_id: 1}).z);
assert.eq(8, t.findOne({_id: 2}).z);
assert.eq(8, t.findOne({_id: 3}).z);

// explain output should show that the ID hack was applied.
const query = {
    _id: {x: 2}
};
let explain = t.find(query).explain("allPlansExecution");
assert.eq(1, explain.executionStats.nReturned, explain);

switch (getOptimizer(explain)) {
    case "classic": {
        assert.eq(1, explain.executionStats.totalKeysExamined, explain);
        let winningPlan = getWinningPlan(explain.queryPlanner);
        assert(isIdhackOrExpress(db, winningPlan), winningPlan);
        break;
    }
    case "CQF":
        // TODO SERVER-70847, how to recognize the case of an IDHACK for Bonsai?
        // TODO SERVER-77719: Ensure that the decision for using the scan lines up with CQF
        // optimizer. M2: allow only collscans, M4: check bonsai behavior for index scan.
        break;
}

// ID hack cannot be used with hint().
t.createIndex({_id: 1, a: 1});
explain = t.find(query).hint({_id: 1, a: 1}).explain();
let winningPlan = getWinningPlan(explain.queryPlanner);
assert(!isIdhackOrExpress(db, winningPlan), winningPlan);

// ID hack cannot be used with skip().
explain = t.find(query).skip(1).explain();
winningPlan = getWinningPlan(explain.queryPlanner);
assert(!isIdhackOrExpress(db, winningPlan), winningPlan);

// ID hack cannot be used with a regex predicate.
assert.commandWorked(t.insert({_id: "abc"}));
explain = t.find({_id: /abc/}).explain();
assert.eq({_id: "abc"}, t.findOne({_id: /abc/}));
winningPlan = getWinningPlan(explain.queryPlanner);
assert(!isIdhackOrExpress(db, winningPlan), winningPlan);

// Express is an 8.0+ feature.
const hasExpress = isExpress(db, getWinningPlan(t.find({_id: 1}).explain().queryPlanner))
if (hasExpress) {
    // Express is used for simple _id queries.
    explain = t.find({_id: 1}).explain();
    winningPlan = getWinningPlan(explain.queryPlanner);
    assert(isExpress(db, winningPlan), winningPlan);

    // Express is used _id queries with simple projections.
    explain = t.find({_id: 1}, {_id: 1}).explain();
    winningPlan = getWinningPlan(explain.queryPlanner);
    assert(isExpress(db, winningPlan), winningPlan);

    explain = t.find({_id: 1}, {_id: 0}).explain();
    winningPlan = getWinningPlan(explain.queryPlanner);
    assert(isExpress(db, winningPlan), winningPlan);

    // Express is not supported with non-simple projections, Idhack is.
    explain = t.find({_id: 1}, {"foo.bar": 0}).explain();
    winningPlan = getWinningPlan(explain.queryPlanner);
    assert(!isExpress(db, winningPlan), winningPlan);
    assert(isIdhackOrExpress(db, winningPlan), winningPlan);

    // Express is not supported with batchSize, Idhack is.
    explain = t.find({_id: 1}).batchSize(10).explain();
    winningPlan = getWinningPlan(explain.queryPlanner);
    assert(!isExpress(db, winningPlan), winningPlan);
    assert(isIdhackOrExpress(db, winningPlan), winningPlan);

    // Express is not supported with returnKey, Idhack is.
    explain = t.find({_id: 1}).returnKey().explain();
    winningPlan = getWinningPlan(explain.queryPlanner);
    assert(!isExpress(db, winningPlan), winningPlan);
    assert(isIdhackOrExpress(db, winningPlan), winningPlan);
}

// Covered query returning _id field only can be handled by ID hack.
const parentStage = checkSbeFullyEnabled(db) ? "PROJECTION_COVERED" : "FETCH";
explain = t.find(query, {_id: 1}).explain();
winningPlan = getWinningPlan(explain.queryPlanner);

switch (getOptimizer(explain)) {
    case "classic": {
        assert(isIdhackOrExpress(db, winningPlan), winningPlan);
        break;
    }
    case "CQF":
        // TODO SERVER-70847, how to recognize the case of an IDHACK for Bonsai?
        // TODO SERVER-77719: Ensure that the decision for using the scan lines up with CQF
        // optimizer. M2: allow only collscans, M4: check bonsai behavior for index scan.
        break;
}

// Check doc from covered ID hack query.
assert.eq({_id: {x: 2}}, t.findOne(query, {_id: 1}), explain);

//
// Non-covered projection for idhack.
//

t.drop();
assert.commandWorked(t.insert({_id: 0, a: 0, b: [{c: 1}, {c: 2}]}));
assert.commandWorked(t.insert({_id: 1, a: 1, b: [{c: 3}, {c: 4}]}));

// Simple inclusion.
assert.eq({_id: 1, a: 1}, t.find({_id: 1}, {a: 1}).next());
assert.eq({a: 1}, t.find({_id: 1}, {_id: 0, a: 1}).next());
assert.eq({_id: 0, a: 0}, t.find({_id: 0}, {_id: 1, a: 1}).next());
assert.eq({_id: 1}, t.find({_id: 1}, {foobar: 1}).next());
assert.eq({}, t.find({_id: 1}, {_id: 0, foobar: 1}).next());
assert.eq(false, t.find({_id: 8}, {_id: 1}).hasNext());

// Simple exclusion.
assert.eq({a: 1, b: [{c: 3}, {c: 4}]}, t.find({_id: 1}, {_id: 0}).next());
assert.eq({_id: 1, b: [{c: 3}, {c: 4}]}, t.find({_id: 1}, {a: 0}).next());
assert.eq({_id: 1, a: 1, b: [{c: 3}, {c: 4}]}, t.find({_id: 1}, {foobar: 0}).next());
assert.eq({_id: 1, a: 1}, t.find({_id: 1}, {b: 0}).next());
assert.eq({_id: 0}, t.find({_id: 0}, {a: 0, b: 0}).next());
assert.eq({}, t.find({_id: 0}, {a: 0, b: 0, _id: 0}).next());
assert.eq(false, t.find({_id: 8}, {_id: 0}).hasNext());

// Non-simple: dotted fields.
assert.eq({b: [{c: 1}, {c: 2}]}, t.find({_id: 0}, {_id: 0, "b.c": 1}).next());
assert.eq({_id: 1}, t.find({_id: 1}, {"foo.bar": 1}).next());

// Non-simple: elemMatch projection.
assert.eq({_id: 1, b: [{c: 4}]}, t.find({_id: 1}, {b: {$elemMatch: {c: 4}}}).next());

// Non-simple: .returnKey().
assert.eq({_id: 1}, t.find({_id: 1}).returnKey().next());

// Non-simple: .returnKey() overrides other projections.
assert.eq({_id: 1}, t.find({_id: 1}, {a: 1}).returnKey().next());

// Test that equality queries on _id with min() or max() require hint().
let err = assert.throws(() => t.find({_id: 2}).min({_id: 1}).itcount());
assert.commandFailedWithCode(err, 51173);
err = assert.throws(() => t.find({_id: 2}).max({_id: 3}).itcount());
assert.commandFailedWithCode(err, 51173);

// Test that equality queries on _id respect min() and max().
assert.eq({_id: 1}, t.find({_id: 1}).hint({_id: 1}).min({_id: 0}).returnKey().next());
assert.eq({_id: 1}, t.find({_id: 1}).hint({_id: 1}).min({_id: 0}).max({_id: 2}).returnKey().next());
assert.eq(0, t.find({_id: 1}).hint({_id: 1}).max({_id: 0}).itcount());
assert.eq(0, t.find({_id: 1}).hint({_id: 1}).min({_id: 2}).itcount());

explain = t.find({_id: 2}).hint({_id: 1}).min({_id: 1}).max({_id: 3}).explain();
winningPlan = getWinningPlan(explain.queryPlanner);
assert(!isIdhackOrExpress(db, winningPlan), winningPlan);
