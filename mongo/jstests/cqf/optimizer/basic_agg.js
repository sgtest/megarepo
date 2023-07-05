import {
    assertValueOnPlanPath,
    checkCascadesOptimizerEnabled
} from "jstests/libs/optimizer_utils.js";

if (!checkCascadesOptimizerEnabled(db)) {
    jsTestLog("Skipping test because the optimizer is not enabled");
    quit();
}

const coll = db.cqf_basic_index;
coll.drop();

const documents = [{a: {b: 1}}, {a: {b: 2}}, {a: {b: 3}}, {a: {b: 4}}, {a: {b: 5}}];

const extraDocCount = 500;
// Add extra docs to make sure indexes can be picked.
for (let i = 0; i < extraDocCount; i++) {
    documents.push({a: {b: i + 10}});
}

assert.commandWorked(coll.insertMany(documents));

assert.commandWorked(coll.createIndex({'a.b': 1}));

let res = coll.explain("executionStats").aggregate([{$match: {'a.b': 2}}]);
assert.eq(1, res.executionStats.nReturned);
assertValueOnPlanPath("IndexScan", res, "child.leftChild.nodeType");

res = coll.explain("executionStats").aggregate([{$match: {'a.b': {$gt: 2}}}]);
assert.eq(3 + extraDocCount, res.executionStats.nReturned);
assertValueOnPlanPath("PhysicalScan", res, "child.child.nodeType");

res = coll.explain("executionStats").aggregate([{$match: {'a.b': {$gte: 2}}}]);
assert.eq(4 + extraDocCount, res.executionStats.nReturned);
assertValueOnPlanPath("PhysicalScan", res, "child.child.nodeType");

res = coll.explain("executionStats").aggregate([{$match: {'a.b': {$lt: 2}}}]);
assert.eq(1, res.executionStats.nReturned);
assertValueOnPlanPath("IndexScan", res, "child.leftChild.nodeType");

res = coll.explain("executionStats").aggregate([{$match: {'a.b': {$lte: 2}}}]);
assert.eq(2, res.executionStats.nReturned);
assertValueOnPlanPath("IndexScan", res, "child.leftChild.nodeType");

res = coll.explain("executionStats").aggregate([{$match: {$and: [{'a.b': 2}]}}]);
assert.eq(1, res.executionStats.nReturned);
assertValueOnPlanPath("IndexScan", res, "child.leftChild.nodeType");
