/**
 * Test that $queryStats properly applies hmac to find commands, on mongod and mongos.
 */
load("jstests/libs/telemetry_utils.js");
(function() {
"use strict";

const kHashedCollName = "w6Ax20mVkbJu4wQWAMjL8Sl+DfXAr2Zqdc3kJRB7Oo0=";
const kHashedFieldName = "lU7Z0mLRPRUL+RfAD5jhYPRRpXBsZBxS/20EzDwfOG4=";

function runTest(conn) {
    const db = conn.getDB("test");
    const admin = conn.getDB("admin");

    db.test.drop();
    db.test.insert({v: 1});

    db.test.find({v: 1}).toArray();

    let telemetry = getQueryStatsFindCmd(admin, /*transformIdentifiers*/ true);

    assert.eq(1, telemetry.length);
    assert.eq("find", telemetry[0].key.queryShape.command);
    assert.eq({[kHashedFieldName]: {$eq: "?number"}}, telemetry[0].key.queryShape.filter);

    db.test.insert({v: 2});

    const cursor = db.test.find({v: {$gt: 0, $lt: 3}}).batchSize(1);
    telemetry = getQueryStatsFindCmd(admin, /*transformIdentifiers*/ true);
    // Cursor isn't exhausted, so there shouldn't be another entry yet.
    assert.eq(1, telemetry.length);

    assert.commandWorked(
        db.runCommand({getMore: cursor.getId(), collection: db.test.getName(), batchSize: 2}));

    telemetry = getQueryStatsFindCmd(admin, /*transformIdentifiers*/ true);
    assert.eq(2, telemetry.length);
    assert.eq("find", telemetry[1].key.queryShape.command);
    assert.eq({
        "$and": [{[kHashedFieldName]: {"$gt": "?number"}}, {[kHashedFieldName]: {"$lt": "?number"}}]
    },
              telemetry[1].key.queryShape.filter);
}

const conn = MongoRunner.runMongod({
    setParameter: {
        internalQueryStatsRateLimit: -1,
        featureFlagQueryStats: true,
    }
});
runTest(conn);
MongoRunner.stopMongod(conn);

const st = new ShardingTest({
    mongos: 1,
    shards: 1,
    config: 1,
    rs: {nodes: 1},
    mongosOptions: {
        setParameter: {
            internalQueryStatsRateLimit: -1,
            featureFlagQueryStats: true,
            'failpoint.skipClusterParameterRefresh': "{'mode':'alwaysOn'}"
        }
    },
});
runTest(st.s);
st.stop();
}());
