// Test basic query stage collection scan functionality.
//
// @tags: [
//   # The test runs commands that are not allowed with security token: stageDebug.
//   not_allowed_with_security_token,
//   does_not_support_stepdowns,
//   uses_testing_only_commands,
//   tenant_migration_incompatible,
//   no_selinux,
// ]

let t = db.stages_collection_scan;
t.drop();
var collname = "stages_collection_scan";

var N = 50;
for (var i = 0; i < N; ++i) {
    t.insert({foo: i});
}

let forward = {cscan: {args: {direction: 1}}};
let res = db.runCommand({stageDebug: {collection: collname, plan: forward}});
assert.eq(res.ok, 1);
assert.eq(res.results.length, N);
assert.eq(res.results[0].foo, 0);
assert.eq(res.results[49].foo, 49);

// And, backwards.
let backward = {cscan: {args: {direction: -1}}};
res = db.runCommand({stageDebug: {collection: collname, plan: backward}});
assert.eq(res.ok, 1);
assert.eq(res.results.length, N);
assert.eq(res.results[0].foo, 49);
assert.eq(res.results[49].foo, 0);

let forwardFiltered = {cscan: {args: {direction: 1}, filter: {foo: {$lt: 25}}}};
res = db.runCommand({stageDebug: {collection: collname, plan: forwardFiltered}});
assert.eq(res.ok, 1);
assert.eq(res.results.length, 25);
assert.eq(res.results[0].foo, 0);
assert.eq(res.results[24].foo, 24);

let backwardFiltered = {cscan: {args: {direction: -1}, filter: {foo: {$lt: 25}}}};
res = db.runCommand({stageDebug: {collection: collname, plan: backwardFiltered}});
assert.eq(res.ok, 1);
assert.eq(res.results.length, 25);
assert.eq(res.results[0].foo, 24);
assert.eq(res.results[24].foo, 0);
