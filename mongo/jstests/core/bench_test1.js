// Cannot implicitly shard accessed collections because of extra shard key index in sharded
// collection.
// The test runs commands that are not allowed with security token: benchRun.
// @tags: [
//   not_allowed_with_security_token,
//   assumes_no_implicit_index_creation,
//   uses_multiple_connections,
//   # This test uses benchRun which does not use runCommand (required by the
//   # inject_tenant_prefix.js override).
//   tenant_migration_incompatible,
// ]
const t = db.bench_test1;
t.drop();

assert.commandWorked(t.insert({_id: 1, x: 1}));
assert.commandWorked(t.insert({_id: 2, x: 1}));

const ops = [
    {op: "findOne", ns: t.getFullName(), query: {_id: 1}, readCmd: true},
    {op: "update", ns: t.getFullName(), query: {_id: 1}, update: {$inc: {x: 1}}, writeCmd: true}
];

const seconds = 2;

const benchArgs = {
    ops: ops,
    parallel: 2,
    seconds: seconds,
    host: db.getMongo().host
};

if (jsTest.options().auth) {
    benchArgs['db'] = 'admin';
    benchArgs['username'] = jsTest.options().authUser;
    benchArgs['password'] = jsTest.options().authPassword;
}
const res = benchRun(benchArgs);

assert.lte(seconds * res.update, t.findOne({_id: 1}).x * 1.5, "A1");

assert.eq(1, t.getIndexes().length, "B1");
benchArgs['ops'] = [{op: "createIndex", ns: t.getFullName(), key: {x: 1}}];
benchArgs['parallel'] = 1;
benchRun(benchArgs);
assert.eq(2, t.getIndexes().length, "B2");
benchArgs['ops'] = [{op: "dropIndex", ns: t.getFullName(), key: {x: 1}}];
benchRun(benchArgs);
assert.soon(function() {
    return t.getIndexes().length == 1;
});