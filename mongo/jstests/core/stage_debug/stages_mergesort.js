// Test query stage merge sorting.
//
// @tags: [
//   # The test runs commands that are not allowed with security token: stageDebug.
//   not_allowed_with_signed_security_token,
//   does_not_support_stepdowns,
//   uses_testing_only_commands,
//   no_selinux,
// ]

let t = db.stages_mergesort;
t.drop();
var collname = "stages_mergesort";

var N = 10;
for (var i = 0; i < N; ++i) {
    t.insert({foo: 1, bar: N - i - 1});
    t.insert({baz: 1, bar: i});
}

t.createIndex({foo: 1, bar: 1});
t.createIndex({baz: 1, bar: 1});

// foo == 1
// We would (internally) use "": MinKey and "": MaxKey for the bar index bounds.
let ixscan1 = {
    ixscan: {
        args: {
            keyPattern: {foo: 1, bar: 1},
            startKey: {foo: 1, bar: 0},
            endKey: {foo: 1, bar: 100000},
            startKeyInclusive: true,
            endKeyInclusive: true,
            direction: 1
        }
    }
};
// baz == 1
let ixscan2 = {
    ixscan: {
        args: {
            keyPattern: {baz: 1, bar: 1},
            startKey: {baz: 1, bar: 0},
            endKey: {baz: 1, bar: 100000},
            startKeyInclusive: true,
            endKeyInclusive: true,
            direction: 1
        }
    }
};

let mergesort = {mergeSort: {args: {nodes: [ixscan1, ixscan2], pattern: {bar: 1}}}};
let res = db.runCommand({stageDebug: {plan: mergesort, collection: collname}});
assert.eq(res.ok, 1);
assert.eq(res.results.length, 2 * N);
assert.eq(res.results[0].bar, 0);
assert.eq(res.results[2 * N - 1].bar, N - 1);
