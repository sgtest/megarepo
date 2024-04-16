/**
 * Tests that 'defaultMaxTimeMS' is correctly bypassed when the 'bypassDefaultMaxTimeMS' privilege
 * is granted.
 *
 * @tags: [
 *   creates_and_authenticates_user,
 *   featureFlagDefaultReadMaxTimeMS,
 *   # Transactions aborted upon fcv upgrade or downgrade; cluster parameters use internal txns.
 *   requires_auth,
 *   requires_replication,
 *   requires_sharding,
 *   uses_transactions,
 * ]
 */

function setDefaultReadMaxTimeMS(db, newValue) {
    assert.commandWorked(
        db.runCommand({setClusterParameter: {defaultMaxTimeMS: {readOperations: newValue}}}));

    // Currently, the mongos cluster parameter cache is not updated on setClusterParameter. An
    // explicit call to getClusterParameter will refresh the cache.
    assert.commandWorked(db.runCommand({getClusterParameter: "defaultMaxTimeMS"}));
}

function runBypassTests(conn) {
    const dbName = jsTestName();
    const adminDB = conn.getDB("admin");

    // Create the admin user, which is used to insert.
    adminDB.createUser({user: 'admin', pwd: 'admin', roles: ['root']});
    assert.eq(1, adminDB.auth("admin", "admin"));

    const testDB = adminDB.getSiblingDB(dbName);
    const collName = "test";
    const coll = testDB.getCollection(collName);

    for (let i = 0; i < 10; ++i) {
        assert.commandWorked(coll.insert({a: 1}));
    }

    const slowStage = {
        $match: {
            $expr: {
                $function: {
                    body: function() {
                        sleep(1000);
                        return true;
                    },
                    args: [],
                    lang: "js"
                }
            }
        }
    };

    // Sets the default maxTimeMS for read operations with a small value.
    setDefaultReadMaxTimeMS(adminDB, 1);

    // Prepare a regular user without the 'bypassDefaultMaxtimeMS' privilege.
    adminDB.createUser({user: 'regularUser', pwd: 'password', roles: ["readAnyDatabase"]});

    // Prepare a user with the 'bypassDefaultMaxtimeMS' privilege.
    adminDB.createRole({
        role: "bypassDefaultMaxtimeMSRole",
        privileges: [
            {resource: {cluster: true}, actions: ["bypassDefaultMaxTimeMS"]},
        ],
        roles: []
    });

    adminDB.createUser({
        user: 'bypassUser',
        pwd: 'password',
        roles: ["readAnyDatabase", "bypassDefaultMaxtimeMSRole"]
    });

    // Prepare command.
    const commandToRun = {
        aggregate: collName,
        pipeline: [slowStage],
        cursor: {},
    };

    // Expect failure for the regular user.
    const regularUserConn = new Mongo(conn.host).getDB('admin');
    assert(regularUserConn.auth('regularUser', 'password'), "Auth failed");
    const regularUserDB = regularUserConn.getSiblingDB(dbName);
    // Note the error could manifest as an Interrupted error sometimes due to the JavaScript
    // execution being interrupted.
    assert.commandFailedWithCode(regularUserDB.runCommand(commandToRun),
                                 [ErrorCodes.Interrupted, ErrorCodes.MaxTimeMSExpired]);

    // Expect a user with 'bypassDefaultMaxTimeMS' to succeed.
    const bypassUserConn = new Mongo(conn.host).getDB('admin');
    assert(bypassUserConn.auth('bypassUser', 'password'), "Auth failed");
    const bypassUserDB = bypassUserConn.getSiblingDB(dbName);
    assert.commandWorked(bypassUserDB.runCommand(commandToRun));

    // Expect a user with 'bypassDefaultMaxTimeMS', but that specified a maxTimeMS on the query, to
    // fail due to timeout.
    assert.commandFailedWithCode(bypassUserDB.runCommand({...commandToRun, maxTimeMS: 1}),
                                 [ErrorCodes.Interrupted, ErrorCodes.MaxTimeMSExpired]);

    // Expect root user to bypass the default.
    const rootUserDB = adminDB.getSiblingDB(dbName);
    assert.commandWorked(rootUserDB.runCommand(commandToRun));

    // Unsets the default MaxTimeMS to make queries not to time out in the
    // following code.
    setDefaultReadMaxTimeMS(adminDB, 0);
}

const rst = new ReplSetTest({nodes: 1, keyFile: "jstests/libs/key1"});
rst.startSet();
rst.initiate();
runBypassTests(rst.getPrimary());
rst.stopSet();

const st = new ShardingTest({
    mongos: 1,
    shards: {nodes: 1},
    config: {nodes: 1},
    keyFile: 'jstests/libs/key1',
});
runBypassTests(st.s);
st.stop();
