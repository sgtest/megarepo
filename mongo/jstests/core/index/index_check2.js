// @tags: [
//   assumes_balancer_off,
//   requires_getmore
// ]

let t = db.index_check2;
t.drop();

// Include helpers for analyzing explain output.
import {getWinningPlan, getOptimizer, isIxscan} from "jstests/libs/analyze_plan.js";

for (var i = 0; i < 1000; i++) {
    var a = [];
    for (var j = 1; j < 5; j++) {
        a.push("tag" + (i * j % 50));
    }
    t.save({num: i, tags: a});
}

let q1 = {tags: "tag6"};
let q2 = {tags: "tag12"};
let q3 = {tags: {$all: ["tag6", "tag12"]}};

assert.eq(120, t.find(q1).itcount(), "q1 a");
assert.eq(120, t.find(q2).itcount(), "q2 a");
assert.eq(60, t.find(q3).itcount(), "q3 a");

t.createIndex({tags: 1});

assert.eq(120, t.find(q1).itcount(), "q1 a");
assert.eq(120, t.find(q2).itcount(), "q2 a");
assert.eq(60, t.find(q3).itcount(), "q3 a");

// We expect these queries to use index scans over { tags: 1 }.
assert(isIxscan(db, getWinningPlan(t.find(q1).explain().queryPlanner)), "e1");
assert(isIxscan(db, getWinningPlan(t.find(q2).explain().queryPlanner)), "e2");
assert(isIxscan(db, getWinningPlan(t.find(q3).explain().queryPlanner)), "e3");

let scanned1 = t.find(q1).explain("executionStats").executionStats.totalKeysExamined;
let scanned2 = t.find(q2).explain("executionStats").executionStats.totalKeysExamined;
let scanned3 = t.find(q3).explain("executionStats").executionStats.totalKeysExamined;

// print( "scanned1: " + scanned1 + " scanned2: " + scanned2 + " scanned3: " + scanned3 );

switch (getOptimizer(t.find(q1).explain())) {
    case "classic":
        // $all should just iterate either of the words
        assert(scanned3 <= Math.max(scanned1, scanned2),
               "$all makes query optimizer not work well");
        break;
    case "CQF":
        // TODO SERVER-77719: Ensure that the decision for using the scan lines up with CQF
        // optimizer. M2: allow only collscans, M4: check bonsai behavior for index scan.
        assert(scanned3 == scanned1 + scanned2);
        break;
    default:
        break
}
