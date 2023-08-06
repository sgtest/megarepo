// Test that you can still authenticate a replset connection to a RS with no primary (SERVER-6665).
var NODE_COUNT = 3;
const rs = new ReplSetTest({"nodes": NODE_COUNT, keyFile: "jstests/libs/key1"});
var nodes = rs.startSet();
rs.initiate();

// Add user
var primary = rs.getPrimary();
primary.getDB("admin").createUser({user: "admin", pwd: "pwd", roles: ["root"]}, {w: NODE_COUNT});

// Can authenticate replset connection when whole set is up.
var conn = new Mongo(rs.getURL());
assert(conn.getDB('admin').auth('admin', 'pwd'));
assert.commandWorked(conn.getDB('admin').foo.insert({a: 1}, {writeConcern: {w: NODE_COUNT}}));

// Make sure there is no primary
rs.stop(0);
rs.stop(1);
rs.waitForState(nodes[2], ReplSetTest.State.SECONDARY);

// Make sure you can still authenticate a replset connection with no primary
var conn2 = new Mongo(rs.getURL());
conn2.setSecondaryOk();
assert(conn2.getDB('admin').auth({user: 'admin', pwd: 'pwd', mechanism: "SCRAM-SHA-1"}));
assert.eq(1, conn2.getDB('admin').foo.findOne().a);

rs.stopSet();