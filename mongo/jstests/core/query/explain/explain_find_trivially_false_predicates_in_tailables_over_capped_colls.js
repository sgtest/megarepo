/**
 * Tests for optimizations applied to trivially false predicates specifically when using tailable
 * cursors over capped collections.
 * @tags: [
 *   requires_fcv_73,
 *   requires_capped,
 *   # Explain command does not support read concerns other than local
 *   assumes_read_concern_local
 * ]
 */
import {getWinningPlanFromExplain, isEofPlan} from "jstests/libs/analyze_plan.js";
import {assertDropAndRecreateCollection} from "jstests/libs/collection_drop_recreate.js"

const collName = "explain_find_trivially_false_predicates_in_tailables_over_capped_colls";

jsTestLog("Testing trivially false optimization with tailable cursors over capped collections");
assertDropAndRecreateCollection(db, collName, {capped: true, size: 1024});
const coll = db[collName];

assert.commandWorked(coll.insert(Array.from({length: 10}, (_, i) => ({_id: i, a: i}))));

// Finding something trivially false (e.g: alwaysFalse) is optimized using an EOF plan.
let explain = coll.find({$alwaysFalse: 1}).tailable({awaitData: true}).explain();
let winningPlan = getWinningPlanFromExplain(explain);
assert(isEofPlan(db, explain));

// It also uses EOF for queries including projection, sorting, limit and skip arguments.
explain = coll.find({$alwaysFalse: 1}, {_id: 0, a: 1})
              .skip(1)
              .limit(2)
              .tailable({awaitData: true})
              .explain();
winningPlan = getWinningPlanFromExplain(explain);
assert(isEofPlan(db, winningPlan));
