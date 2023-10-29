// @tags: [
//   # The test runs commands that are not allowed with security token: isbdgrid.
//   not_allowed_with_security_token,
//   requires_capped,
//   requires_getmore,
//   # This test has statements that do not support non-local read concern.
//   does_not_support_causal_consistency,
// ]

import {FixtureHelpers} from "jstests/libs/fixture_helpers.js";

// Tests for the behavior of tailable cursors when a collection is dropped or the cursor is
// otherwise invalidated.

const collName = "tailable_cursor_invalidation";
const coll = db[collName];
coll.drop();

// Test that you cannot open a tailable cursor on a non-existent collection.
assert.eq(0, assert.commandWorked(db.runCommand({find: collName})).cursor.id);
assert.eq(0, assert.commandWorked(db.runCommand({find: collName, tailable: true})).cursor.id);
assert.eq(0,
          assert.commandWorked(db.runCommand({find: collName, tailable: true, awaitData: true}))
              .cursor.id);

// TODO SERVER-82107 stop dropping the database when testing on sharded cluster.
if (FixtureHelpers.isMongos(db)) {
    // In sharded cluster, if the database exists, the mongos will let you establish a cursor with
    // batch size 0 and return to you before it realizes the shard's cursor is exhausted. The next
    // getMore should return a 0 cursor id though.
    //
    // On the other hand on non-sharded deployments, the mongod will know immediately that the
    // collection doesn't exist, and return a 0 cursor id.
    //
    // Drop the database to make sure mongos will bheave the same as mongod would.
    db.dropDatabase();
}

assert.eq(0,
          assert
              .commandWorked(
                  db.runCommand({find: collName, tailable: true, awaitData: true, batchSize: 0}))
              .cursor.id);

function dropAndRecreateColl() {
    coll.drop();
    assert.commandWorked(db.createCollection(collName, {capped: true, size: 1024}));
    const numDocs = 4;
    const bulk = coll.initializeUnorderedBulkOp();
    for (let i = 0; i < numDocs; ++i) {
        bulk.insert({_id: i});
    }
    assert.commandWorked(bulk.execute());
}
dropAndRecreateColl();

/**
 * Runs a find command to establish a cursor. Asserts that the command worked and that the
 * cursor id is not 0, then returns the cursor id.
 */
function openCursor({tailable, awaitData}) {
    const findRes = assert.commandWorked(
        db.runCommand({find: collName, tailable: tailable, awaitData: awaitData}));
    assert.neq(findRes.cursor.id, 0);
    assert.eq(findRes.cursor.ns, coll.getFullName());
    return findRes.cursor.id;
}

// Test that the cursor dies on getMore if the collection has been dropped.
let cursorId = openCursor({tailable: true, awaitData: false});
dropAndRecreateColl();
assert.commandFailedWithCode(db.runCommand({getMore: cursorId, collection: collName}),
                             [ErrorCodes.QueryPlanKilled, ErrorCodes.NamespaceNotFound]);
cursorId = openCursor({tailable: true, awaitData: true});
dropAndRecreateColl();
assert.commandFailedWithCode(db.runCommand({getMore: cursorId, collection: collName}),
                             [ErrorCodes.QueryPlanKilled, ErrorCodes.NamespaceNotFound]);
