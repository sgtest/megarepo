/**
 * Tests that when the 'movePrimaryFailIfNeedToCloneMovableCollections' failpoint is enabled, a
 * movePrimary command would only fail if:
 * - The command is run with the same 'comment' as specified in the failpoint.
 * - The donor shard still has user data for that database (i.e. untracked unsharded collections).
 *
 * @tags: [requires_fcv_80]
 */
import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {extractUUIDFromObject} from "jstests/libs/uuid_util.js";

function getCollectionUuid(db, collName) {
    const listCollectionRes =
        assert.commandWorked(db.runCommand({listCollections: 1, filter: {name: collName}}));
    return listCollectionRes.cursor.firstBatch[0].info.uuid;
}

function runTest({featureFlagTrackUnshardedCollectionsUponCreation}) {
    jsTest.log("Running test " + tojson({featureFlagTrackUnshardedCollectionsUponCreation}));
    const st = new ShardingTest({
        shards: 3,
        configShard: true,
        rs: {setParameter: {featureFlagTrackUnshardedCollectionsUponCreation}},
    });
    const shard0Primary = st.rs0.getPrimary();
    const shard1Primary = st.rs1.getPrimary();

    const dbName0 = "testDb0";
    const dbName1 = "testDb1";
    const dbName2 = "testDb2";
    const collName0 = "testColl0";
    const collName1 = "testColl1";

    jsTest.log("Testing 'movePrimaryFailIfNeedToCloneMovableCollections' with 'comment'");

    // Make shard0 the primary shard for both dbName0 and dbName1.
    assert.commandWorked(
        st.s.adminCommand({enableSharding: dbName0, primaryShard: st.shard0.shardName}));
    assert.commandWorked(
        st.s.adminCommand({enableSharding: dbName1, primaryShard: st.shard0.shardName}));

    // Make dbName0 have one collection n1, which is an unsharded collection.
    const ns0 = dbName0 + "." + collName0;
    assert.commandWorked(st.s.getCollection(ns0).insert([{x: 1}]));

    // Make dbName1 have two collections ns1 and ns2, where ns1 is an unsharded collection and ns2
    // is a sharded collection with [MinKey, 0] on shard0 and [0, MaxKey] on shard1.
    const ns1 = dbName1 + "." + collName0;
    assert.commandWorked(st.s.getCollection(ns1).insert({x: 1}));
    const ns2 = dbName1 + "." + collName1;
    assert.commandWorked(st.s.adminCommand({shardCollection: ns2, key: {x: 1}}));
    assert.commandWorked(st.s.getCollection(ns2).insert([{x: -1}, {x: 1}]));
    assert.commandWorked(st.s.adminCommand({split: ns2, middle: {x: 0}}));
    assert.commandWorked(st.s.adminCommand(
        {moveChunk: ns2, find: {x: 0}, to: st.shard1.shardName, _waitForDelete: true}));

    const comment = extractUUIDFromObject(UUID());
    const movePrimaryFp0 = configureFailPoint(
        shard0Primary, "movePrimaryFailIfNeedToCloneMovableCollections", {comment});

    // The movePrimary command below should not fail whether not there is data for dbName0 to clone
    // since the command is run without the 'comment' above.
    const movePrimaryRes0 = st.s.adminCommand({movePrimary: dbName0, to: st.shard1.shardName});
    assert.commandWorked(movePrimaryRes0);

    // The movePrimary command below should fail if there is data for dbName1 to clone since the
    // command is run with the 'comment' above. That is, it should fail if the unsharded
    // collection ns1 is untracked since movePrimary doesn't move tracked unsharded collections.
    const movePrimaryRes1 =
        st.s.adminCommand({movePrimary: dbName1, to: st.shard1.shardName, comment});
    if (featureFlagTrackUnshardedCollectionsUponCreation) {
        assert.commandWorked(movePrimaryRes1);
    } else {
        assert.commandFailedWithCode(movePrimaryRes1, 9046501);
        // Manually move the untracked unsharded collection ns1 to shard1.
        assert.commandWorked(
            st.s.adminCommand({moveCollection: ns1, toShard: st.shard1.shardName}));
        // The movePrimary command below should not fail since there is no data to clone. The
        // sharded collection n2 still has a chunk on shard0 but the collection is sharded so it
        // does not prevent shard1 from becoming the primary shard.
        assert.commandWorked(
            st.s.adminCommand({movePrimary: dbName1, to: st.shard1.shardName, comment}));
    }

    movePrimaryFp0.off();

    const transitionRes0 =
        assert.commandWorked(st.s.adminCommand({transitionToDedicatedConfigServer: 1}));
    jsTest.log("The first transitionToDedicatedConfigServer response: " + tojson(transitionRes0));
    assert.eq(transitionRes0.state, "started", transitionRes0);
    assert.eq(transitionRes0.dbsToMove.length, 0, transitionRes0);

    const transitionRes1 =
        assert.commandWorked(st.s.adminCommand({transitionToDedicatedConfigServer: 1}));
    jsTest.log("The second transitionToDedicatedConfigServer response: " + tojson(transitionRes1));
    assert.eq(transitionRes1.state, "ongoing", transitionRes1);
    assert.eq(transitionRes1.remaining.dbs, 0, transitionRes1);
    // There is still data left on shard0.
    assert.gte(transitionRes1.remaining.chunks, 0, transitionRes1);
    assert.eq(transitionRes1.remaining.jumboChunks, 0, transitionRes1);
    assert.eq(transitionRes1.dbsToMove.length, 0, transitionRes1);

    // Move the remaining data on shard0 to shard1.
    assert.commandWorked(st.s.adminCommand(
        {moveChunk: ns2, find: {x: -1}, to: st.shard1.shardName, _waitForDelete: true}));
    if (featureFlagTrackUnshardedCollectionsUponCreation) {
        // movePrimary doesn't move tracked unsharded collections so n0 and n1 still need to moved
        // manually to shard1.
        assert.commandWorked(
            st.s.adminCommand({moveCollection: ns0, toShard: st.shard1.shardName}));
        assert.commandWorked(
            st.s.adminCommand({moveCollection: ns1, toShard: st.shard1.shardName}));
    }
    // Move the chunk(s) for config.system.sessions if needed.
    const collUuid = getCollectionUuid(st.s.getDB("config"), "system.sessions");
    const chunkDocs =
        st.s.getCollection("config.chunks").find({uuid: collUuid, shard: "config"}).toArray();
    for (let chunkDoc of chunkDocs) {
        assert.commandWorked(st.s.adminCommand({
            moveChunk: "config.system.sessions",
            find: chunkDoc.min,
            to: st.shard1.shardName,
            _waitForDelete: true
        }));
    }

    const transitionRes2 =
        assert.commandWorked(st.s.adminCommand({transitionToDedicatedConfigServer: 1}));
    jsTest.log("The third transitionToDedicatedConfigServer response: " + tojson(transitionRes2));
    assert.eq(transitionRes2.state, "completed", transitionRes2);

    jsTest.log("Testing 'movePrimaryFailIfNeedToCloneMovableCollections' without 'comment'");

    // Make shard1 the primary shard for dbName2.
    assert.commandWorked(
        st.s.adminCommand({enableSharding: dbName2, primaryShard: st.shard1.shardName}));

    // Make dbName2 have one collection n3, which is an unsharded collection.
    const ns3 = dbName2 + "." + collName0;
    assert.commandWorked(st.s.getCollection(ns3).insert([{x: 1}]));

    const movePrimaryFp1 =
        configureFailPoint(shard1Primary, "movePrimaryFailIfNeedToCloneMovableCollections");

    // The movePrimary command below should fail if there is data for dbName2 to clone. That is, it
    // should fail if the unsharded collection ns3 is untracked since movePrimary doesn't move
    // tracked unsharded collections.
    const movePrimaryRes2 = st.s.adminCommand({movePrimary: dbName2, to: st.shard2.shardName});
    if (featureFlagTrackUnshardedCollectionsUponCreation) {
        assert.commandWorked(movePrimaryRes2);
    } else {
        assert.commandFailedWithCode(movePrimaryRes2, 9046501);
    }

    movePrimaryFp1.off();

    st.stop();
}

runTest({featureFlagTrackUnshardedCollectionsUponCreation: false});
runTest({featureFlagTrackUnshardedCollectionsUponCreation: true});
