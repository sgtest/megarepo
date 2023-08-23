// Test query stage sorting.
//
// @tags: [
//   # The test runs commands that are not allowed with security token: stageDebug.
//   not_allowed_with_security_token
// ]

if (false) {
    let t = db.stages_sort;
    t.drop();

    var N = 50;
    for (var i = 0; i < N; ++i) {
        t.insert({foo: i, bar: N - i});
    }

    t.createIndex({foo: 1});

    // Foo <= 20, descending.
    let ixscan1 = {
        ixscan: {
            args: {
                name: "stages_sort",
                keyPattern: {foo: 1},
                startKey: {"": 20},
                endKey: {},
                startKeyInclusive: true,
                endKeyInclusive: true,
                direction: -1
            }
        }
    };

    // Sort with foo ascending.
    let sort1 = {sort: {args: {node: ixscan1, pattern: {foo: 1}}}};
    let res = db.runCommand({stageDebug: sort1});
    assert.eq(res.ok, 1);
    assert.eq(res.results.length, 21);
    assert.eq(res.results[0].foo, 0);
    assert.eq(res.results[20].foo, 20);

    // Sort with a limit.
    // sort2 = {sort: {args: {node: ixscan1, pattern: {foo: 1}, limit: 2}}};
    // res = db.runCommand({stageDebug: sort2});
    // assert.eq(res.ok, 1);
    // assert.eq(res.results.length, 2);
    // assert.eq(res.results[0].foo, 0);
    // assert.eq(res.results[1].foo, 1);
}
