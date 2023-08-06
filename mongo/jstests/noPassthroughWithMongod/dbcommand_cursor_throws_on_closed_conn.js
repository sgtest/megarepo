var testDB = db.getSiblingDB('dbcommand_cursor_throws_on_closed_conn');
testDB.dropDatabase();
var coll = testDB.collection;
var conn = testDB.getMongo();
assert.commandWorked(coll.save({}));
var res = assert.commandWorked(testDB.runCommand({
    find: coll.getName(),
    batchSize: 0,
}));

conn.close();
assert.throws(() => new DBCommandCursor(testDB, res));
