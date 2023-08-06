// Tests the 'changeStreams' cluster-wide configuration parameter on the replica sets and the
// sharded cluster.
// @tags: [
//  requires_replication,
//  requires_sharding,
//  featureFlagServerlessChangeStreams,
//  requires_fcv_63,
//  # TODO SERVER-74811: Re-enable this test.
//   __TEMPORARILY_DISABLED__,
// ]
import {
    ChangeStreamMultitenantReplicaSetTest
} from "jstests/serverless/libs/change_collection_util.js";

// Verifies that the 'getClusterParameter' on the 'changeStreams' cluster-wide parameter returns the
// expected response.
function assertGetResponse(db, expectedChangeStreamParam) {
    const response = assert.commandWorked(db.runCommand({getClusterParameter: "changeStreams"}));
    assert.eq(response.clusterParameters[0].expireAfterSeconds,
              expectedChangeStreamParam.expireAfterSeconds,
              response);
}

// Tests the 'changeStreams' cluster-wide configuration parameter with the 'admin' database.
function testWithAdminDB(conn) {
    const adminDB = conn.getDB("admin");

    // Invalid string value for the 'expireAfterSeconds' parameter should fail.
    assert.commandFailedWithCode(
        adminDB.runCommand({setClusterParameter: {changeStreams: {expireAfterSeconds: "off"}}}),
        ErrorCodes.TypeMismatch);

    // A negative value of 'expireAfterSeconds' should fail.
    assert.commandFailedWithCode(
        adminDB.runCommand(
            {setClusterParameter: {changeStreams: {expireAfterSeconds: NumberLong(-1)}}}),
        ErrorCodes.BadValue);

    // A zero value of 'expireAfterSeconds' should fail.
    assert.commandFailedWithCode(
        adminDB.runCommand(
            {setClusterParameter: {changeStreams: {expireAfterSeconds: NumberLong(0)}}}),
        ErrorCodes.BadValue);

    // A positive value of 'expireAfterSeconds' should succeed.
    assert.commandWorked(adminDB.runCommand(
        {setClusterParameter: {changeStreams: {expireAfterSeconds: NumberLong(36)}}}));

    assertGetResponse(adminDB, {expireAfterSeconds: NumberLong(36)});

    // An empty parameter to 'changeStreams' cluster parameter should reset the 'expireAfterSeconds'
    // to the default value.
    assert.commandWorked(adminDB.runCommand({setClusterParameter: {changeStreams: {}}}));
    assertGetResponse(adminDB, {expireAfterSeconds: NumberLong(3600)});

    // Modifying expireAfterSeconds should succeed.
    assert.commandWorked(adminDB.runCommand(
        {setClusterParameter: {changeStreams: {expireAfterSeconds: NumberLong(100)}}}));
    assertGetResponse(adminDB, {expireAfterSeconds: NumberLong(100)});
}

function testWithoutAdminDB(conn) {
    const db = conn.getDB(jsTestName());
    assert.commandFailedWithCode(db.runCommand({getClusterParameter: "changeStreams"}),
                                 ErrorCodes.Unauthorized);
    assert.commandFailedWithCode(
        db.runCommand(
            {setClusterParameter: {changeStreams: {expireAfterSeconds: NumberLong(3600)}}}),
        ErrorCodes.Unauthorized);
}

// Tests the set and get change streams parameter on the replica-set.
{
    const rst = new ChangeStreamMultitenantReplicaSetTest({name: "replSet", nodes: 2});

    const primary = rst.getPrimary();
    const secondary = rst.getSecondaries()[0];

    // Verify that the set and get commands cannot be issued on database other than the 'admin'.
    [primary, secondary].forEach(conn => {
        testWithoutAdminDB(conn);
    });

    // Tests the set and get commands on the primary node.
    testWithAdminDB(primary);

    rst.stopSet();
}

// Tests the set and get change streams parameter on the sharded cluster.
{
    const st = new ShardingTest({
        shards: 1,
        mongos: 1,
        config: 1,
        other: {
            mongosOptions: {setParameter: {internalChangeStreamUseTenantIdForTesting: true}},
            shardOptions: {setParameter: {internalChangeStreamUseTenantIdForTesting: true}},
            configOptions: {setParameter: {internalChangeStreamUseTenantIdForTesting: true}}
        }
    });
    const adminDB = st.rs0.getPrimary().getDB("admin");

    // Test that setClusterParameter cannot be issued directly on shards in the sharded cluster,
    // while getClusterParameter can.
    assert.commandFailedWithCode(
        adminDB.runCommand(
            {setClusterParameter: {changeStreams: {expireAfterSeconds: NumberLong(40)}}}),
        ErrorCodes.NotImplemented);
    assertGetResponse(adminDB, {expireAfterSeconds: NumberLong(3600)});

    // Run the set and get commands on the mongoS.
    testWithAdminDB(st.s);

    st.stop();
}

// Tests that 'changeStreams.expireAfterSeconds' is only available in serverless. The
// 'changeStreams.expireAfterSeconds' cluster wide parameter requires explicit setting of either
// 'multitenancySupport' and 'serverless' or 'internalChangeStreamUseTenantIdForTesting' along with
// the feature flag 'featureFlagServerlessChangeStreams' which has not been done in this scenario.
{
    const rst = new ReplSetTest({nodes: 1});
    rst.startSet();
    rst.initiate();
    const primary = rst.getPrimary();
    const adminDB = primary.getDB("admin");

    assert.commandFailedWithCode(
        adminDB.runCommand(
            {setClusterParameter: {changeStreams: {expireAfterSeconds: NumberLong(10)}}}),
        ErrorCodes.CommandNotSupported);

    rst.stopSet();
}
