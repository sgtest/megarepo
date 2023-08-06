/**
 * Tests for explaining find through the explain command.
 * @tags: [
 *   assumes_read_concern_local,
 * ]
 */

var collName = "jstests_explain_find";
var t = db[collName];
t.drop();

t.createIndex({a: 1});

for (var i = 0; i < 10; i++) {
    t.insert({_id: i, a: i});
}

var explain =
    db.runCommand({explain: {find: collName, filter: {a: {$lte: 2}}}, verbosity: "executionStats"});
assert.commandWorked(explain);
assert.eq(3, explain.executionStats.nReturned);

explain = db.runCommand({
    explain: {find: collName, min: {a: 4}, max: {a: 6}, hint: {a: 1}},
    verbosity: "executionStats",
});
assert.commandWorked(explain);
assert.eq(2, explain.executionStats.nReturned);

// Invalid verbosity string.
let error = assert.throws(function() {
    t.explain("foobar").find().finish();
});
assert.commandFailedWithCode(error, ErrorCodes.BadValue);

error = assert.throws(function() {
    t.find().explain("foobar");
});
assert.commandFailedWithCode(error, ErrorCodes.BadValue);