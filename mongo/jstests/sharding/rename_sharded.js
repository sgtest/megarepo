/**
 * Test all the possible succeed/fail cases around sharded collections renaming.
 */

import {getUUIDFromConfigCollections} from "jstests/libs/uuid_util.js";
import {
    moveDatabaseAndUnshardedColls
} from "jstests/sharding/libs/move_database_and_unsharded_coll_helper.js";

/**
 * Initialize a "from" sharded collection with 2 chunks - on 2 different nodes - each containing 1
 * document. Rename to `toNs` with the provided options and get sure it succeeds/fails as expected.
 */
function testRename(st, dbName, toNs, dropTarget, mustFail) {
    const mongos = st.s0;

    const fromNs = dbName + '.from';
    assert.commandWorked(
        mongos.adminCommand({enablesharding: dbName, primaryShard: st.shard0.shardName}));
    assert.commandWorked(mongos.adminCommand({shardCollection: fromNs, key: {x: 1}}));

    const fromColl = mongos.getCollection(fromNs);
    fromColl.insert({x: 0});
    fromColl.insert({x: 2});
    assert.commandWorked(mongos.adminCommand({split: fromNs, middle: {x: 1}}));

    const fromUUID = getUUIDFromConfigCollections(mongos, fromNs);
    const aChunk = mongos.getDB('config').chunks.findOne({uuid: fromUUID});
    assert.commandWorked(mongos.adminCommand({
        moveChunk: fromNs,
        bounds: [aChunk.min, aChunk.max],
        to: st.shard1.shardName,
    }));

    const res = fromColl.renameCollection(toNs.split('.')[1], dropTarget);
    if (mustFail) {
        assert.commandFailed(res);
        return;
    }

    assert.commandWorked(res);

    const toUUID = getUUIDFromConfigCollections(mongos, toNs);
    const chunks = mongos.getDB('config').chunks.find({uuid: toUUID});
    const chunk0 = chunks.next();
    const chunk1 = chunks.next();

    assert(!chunks.hasNext(), 'Target collection expected to have exactly 2 chunks');
    assert(chunk0.shard != chunk1.shard, 'Chunks expected to be on different shards');

    const toColl = mongos.getCollection(toNs);
    assert.eq(toColl.find({x: 0}).itcount(), 1, 'Expected exactly one document on the shard');
    assert.eq(toColl.find({x: 2}).itcount(), 1, 'Expected exactly one document on the shard');
}

// Never use the third shard, but leave it in order to indirectly check that rename participants
// properly handle the following cases:
// - Locally unknown source collection to rename
// - Locally unknown target collection to drop
const st = new ShardingTest({shards: 3, mongos: 1, other: {enableBalancer: false}});

const mongos = st.s0;

// Rename non-existing source collection to a target collection/view (dropTarget=false) must
// fail with NamespaceNotFound. Make sure the check on the source is done before any check on the
// target for consistency with replicaset. We cannot delegate this to passthrough suite since in
// those suites any non-existing collection will always be implicitely sharded at the first access
{
    const dbName = 'notExistingSource';
    assert.commandWorked(
        mongos.adminCommand({enablesharding: dbName, primaryShard: st.shard0.shardName}));

    // Rename non-existing source to non-existing target
    assert.commandFailedWithCode(
        st.getDB(dbName).adminCommand(
            {renameCollection: dbName + ".source", to: dbName + ".target"}),
        ErrorCodes.NamespaceNotFound);

    // Rename non-existing source to existing collection
    const toCollName = dbName + ".target";
    const toColl = mongos.getCollection(toCollName);
    toColl.insert({a: 0});

    assert.commandFailedWithCode(
        st.getDB(dbName).adminCommand({renameCollection: dbName + ".source", to: toCollName}),
        ErrorCodes.NamespaceNotFound);

    // Rename non-existing source to existing view
    const toViewName = dbName + ".target_view";
    assert.commandWorked(st.getDB(dbName).createView(toViewName, toCollName, []));

    assert.commandFailedWithCode(
        st.getDB(dbName).adminCommand({renameCollection: dbName + ".source", to: toViewName}),
        ErrorCodes.NamespaceNotFound);
}

// Rename to existing sharded target collection with dropTarget=true must succeed
{
    const dbName = 'testRenameToExistingShardedCollection';
    const toNs = dbName + '.to';
    assert.commandWorked(
        mongos.adminCommand({enablesharding: dbName, primaryShard: st.shard0.shardName}));
    assert.commandWorked(mongos.adminCommand({shardCollection: toNs, key: {a: 1}}));

    const toColl = mongos.getCollection(toNs);
    toColl.insert({a: 0});
    toColl.insert({a: 2});
    assert.commandWorked(mongos.adminCommand({split: toNs, middle: {a: 1}}));

    const toUUID = getUUIDFromConfigCollections(mongos, toNs);
    const aChunk = mongos.getDB('config').chunks.findOne({uuid: toUUID});
    assert.commandWorked(mongos.adminCommand({
        moveChunk: toNs,
        bounds: [aChunk.min, aChunk.max],
        to: st.shard1.shardName,
    }));

    testRename(st, dbName, toNs, true /* dropTarget */, false /* mustFail */);
}
// Rename to existing unsharded target collection with dropTarget=true must succeed
{
    const dbName = 'testRenameToExistingUnshardedCollection';
    const toNs = dbName + '.to';
    assert.commandWorked(
        mongos.adminCommand({enablesharding: dbName, primaryShard: st.shard0.shardName}));
    const toColl = mongos.getCollection(toNs);
    toColl.insert({a: 0});

    testRename(st, dbName, toNs, true /* dropTarget */, false /* mustFail */);
}

// Rename to existing unsharded target collection with dropTarget=false must fail
{
    const dbName = 'testRenameToUnshardedCollectionWithoutDropTarget';
    const toNs = dbName + '.to';
    assert.commandWorked(
        mongos.adminCommand({enablesharding: dbName, primaryShard: st.shard0.shardName}));
    const toColl = mongos.getCollection(toNs);
    toColl.insert({a: 0});

    testRename(st, dbName, toNs, false /* dropTarget */, true /* mustFail */);
}

// Rename unsharded collection to sharded target collection with dropTarget=true must succeed
{
    const dbName = 'testRenameUnshardedToShardedTargetCollection';
    const fromNs = dbName + '.from';
    const toNs = dbName + '.to';
    assert.commandWorked(
        mongos.adminCommand({enablesharding: dbName, primaryShard: st.shard0.shardName}));
    assert.commandWorked(mongos.adminCommand({shardCollection: toNs, key: {a: 1}}));

    const toColl = mongos.getCollection(toNs);
    toColl.insert({a: 0});
    toColl.insert({a: 2});
    assert.commandWorked(mongos.adminCommand({split: toNs, middle: {a: 1}}));
    const toUUID = getUUIDFromConfigCollections(mongos, toNs);
    const aChunk = mongos.getDB('config').chunks.findOne({uuid: toUUID});
    assert.commandWorked(mongos.adminCommand(
        {moveChunk: toNs, bounds: [aChunk.min, aChunk.max], to: st.shard1.shardName}));

    const fromColl = mongos.getCollection(fromNs);
    fromColl.insert({x: 0});

    assert.commandWorked(fromColl.renameCollection(toNs.split('.')[1], true /* dropTarget */));

    // Source collection just has documents with field `x`
    assert.eq(toColl.find({x: {$exists: true}}).itcount(), 1, 'Expected one source document');
    // Source collection just has documents with field `a`
    assert.eq(toColl.find({a: {$exists: true}}).itcount(), 0, 'Expected no target documents');
}

// Successful rename must pass tags from source to the target collection
{
    const dbName = 'testRenameFromTaggedCollection';
    const fromNs = dbName + '.from';
    const toNs = dbName + '.to';

    assert.commandWorked(
        mongos.adminCommand({enablesharding: dbName, primaryShard: st.shard0.shardName}));
    assert.commandWorked(mongos.adminCommand({addShardToZone: st.shard0.shardName, zone: 'x'}));
    assert.commandWorked(mongos.adminCommand({addShardToZone: st.shard1.shardName, zone: 'y'}));
    assert.commandWorked(
        mongos.adminCommand({updateZoneKeyRange: fromNs, min: {x: 0}, max: {x: 2}, zone: 'x'}));
    assert.commandWorked(
        mongos.adminCommand({updateZoneKeyRange: fromNs, min: {x: 2}, max: {x: 4}, zone: 'y'}));
    assert.commandWorked(mongos.adminCommand({shardCollection: fromNs, key: {x: 1}}));

    var fromTags = mongos.getDB('config').tags.find({ns: fromNs}).toArray();

    const fromColl = mongos.getCollection(fromNs);
    fromColl.insert({x: 1});

    assert.commandWorked(fromColl.renameCollection(toNs.split('.')[1], false /* dropTarget */));

    const toTags = mongos.getDB('config').tags.find({ns: toNs}).toArray();
    assert.eq(toTags.length, 2, "Expected 2 tags associated to the target collection");

    function deleteDifferentTagFields(tag, index, array) {
        delete tag['_id'];
        delete tag['ns'];
    }
    fromTags.forEach(deleteDifferentTagFields);
    toTags.forEach(deleteDifferentTagFields);

    // Compare field by field because keys can potentially be in different order
    for (let field in Object.keys(fromTags[0])) {
        assert.eq(fromTags[0][field],
                  toTags[0][field],
                  "Expected source tags to be passed to target collection");
        assert.eq(fromTags[1][field],
                  toTags[1][field],
                  "Expected source tags to be passed to target collection");
    }

    fromTags = mongos.getDB('config').tags.find({ns: fromNs}).toArray();
    assert.eq(fromTags.length, 0, "Expected no tags associated to the source collection");
}

// Rename to target collection with tags must fail
{
    const dbName = 'testRenameToTaggedCollection';
    const fromNs = dbName + '.from';
    const toNs = dbName + '.to';
    assert.commandWorked(mongos.adminCommand({addShardToZone: st.shard0.shardName, zone: 'x'}));
    assert.commandWorked(
        mongos.adminCommand({updateZoneKeyRange: toNs, min: {x: 0}, max: {x: 10}, zone: 'x'}));

    assert.commandWorked(mongos.adminCommand({enablesharding: dbName}));
    assert.commandWorked(mongos.adminCommand({shardCollection: fromNs, key: {x: 1}}));

    const fromColl = mongos.getCollection(fromNs);
    fromColl.insert({x: 1});
    assert.commandFailed(fromColl.renameCollection(toNs.split('.')[1], false /* dropTarget*/));
}

// Rename to target collection with very a long name
{
    const dbName = 'testRenameToCollectionWithVeryLongName';

    const testDB = st.rs0.getPrimary().getDB(dbName);
    const fcvDoc = testDB.adminCommand({getParameter: 1, featureCompatibilityVersion: 1});
    if (MongoRunner.compareBinVersions(fcvDoc.featureCompatibilityVersion.version, '5.3') >= 0) {
        const longEnoughNs = dbName + '.' +
            'x'.repeat(235 - dbName.length - 1);
        testRename(st, dbName, longEnoughNs, false /* dropTarget */, false /* mustFail */);

        const tooLongNs = longEnoughNs + 'x';
        testRename(st, dbName, tooLongNs, false /* dropTarget */, true /* mustFail */);
    }
}

// For C2C: rename of existing collection with correct uuid as argument must succeed
// (Also creating target collection to test target UUID internal check)
{
    const dbName = 'testRenameToUnshardedCollectionWithSourceUUID';
    const fromCollName = 'from';
    const fromNs = dbName + '.' + fromCollName;
    const toNs = dbName + '.to';
    assert.commandWorked(
        mongos.adminCommand({enablesharding: dbName, primaryShard: st.shard0.shardName}));
    const fromColl = mongos.getCollection(fromNs);
    fromColl.insert({a: 0});

    const toColl = mongos.getCollection(toNs);
    toColl.insert({b: 0});

    const sourceUUID = assert.commandWorked(st.getDB(dbName).runCommand({listCollections: 1}))
                           .cursor.firstBatch.find(c => c.name === fromCollName)
                           .info.uuid;

    // The command succeeds when the correct UUID is provided.
    assert.commandWorked(mongos.adminCommand({
        renameCollection: fromNs,
        to: toNs,
        dropTarget: true,
        collectionUUID: sourceUUID,
    }));
}

// Rename between DBs after movePrimary works correctly
{
    const db0Name = 'testMovePrimaryWithRenameDB0';
    const db1Name = 'testMovePrimaryWithRenameDB1';
    const collOnDB0 = db0Name + '.from';
    const collOnDB1 = db1Name + '.to';
    assert.commandWorked(
        mongos.adminCommand({enablesharding: db0Name, primaryShard: st.shard0.shardName}));
    assert.commandWorked(
        mongos.adminCommand({enablesharding: db1Name, primaryShard: st.shard0.shardName}));

    mongos.getCollection(collOnDB0).insert({a: 0});

    moveDatabaseAndUnshardedColls(mongos.getDB(db0Name), st.shard1.shardName);
    moveDatabaseAndUnshardedColls(mongos.getDB(db1Name), st.shard1.shardName);

    assert.commandWorked(mongos.adminCommand({
        renameCollection: collOnDB0,
        to: collOnDB1,
    }));

    moveDatabaseAndUnshardedColls(mongos.getDB(db0Name), st.shard0.shardName);
    moveDatabaseAndUnshardedColls(mongos.getDB(db1Name), st.shard0.shardName);

    assert.commandWorked(mongos.adminCommand({
        renameCollection: collOnDB1,
        to: collOnDB0,
    }));
}

st.stop();
