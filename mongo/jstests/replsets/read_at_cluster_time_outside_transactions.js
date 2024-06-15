/**
 * Tests that the "dbHash" command support reading at a Timestamp by using the
 * $_internalReadAtClusterTime option.
 *
 * @tags: [
 *   uses_transactions,
 * ]
 */
const rst = new ReplSetTest({nodes: 1});
rst.startSet();
rst.initiate();

const primary = rst.getPrimary();
const db = primary.getDB("test");

const collName = "read_at_cluster_time_outside_transactions";
const collection = db[collName];

// We prevent the replica set from advancing oldest_timestamp. This ensures that the snapshot
// associated with 'clusterTime' is retained for the duration of this test.
rst.nodes.forEach(conn => {
    assert.commandWorked(conn.adminCommand({
        configureFailPoint: "WTPreserveSnapshotHistoryIndefinitely",
        mode: "alwaysOn",
    }));
});

// We insert 3 documents in order to have data to return for both the find and getMore commands
// when using a batch size of 2. We then save the md5sum associated with the opTime of the last
// insert.
assert.commandWorked(collection.insert({_id: 1, comment: "should be seen by find command"}));
assert.commandWorked(collection.insert({_id: 3, comment: "should be seen by find command"}));

// Insert with w: majority so that the insert timestamp is majority committed and can be used as the
// $_internalReadAtClusterTime for dbHash.
const clusterTime = assert
                        .commandWorked(db.runCommand({
                            insert: collName,
                            documents: [{_id: 5, comment: "should be seen by getMore command"}],
                            writeConcern: {w: "majority"}
                        }))
                        .operationTime;

let res = assert.commandWorked(db.runCommand({dbHash: 1}));
const hashAfterOriginalInserts = {
    collections: res.collections,
    md5: res.md5
};

// The documents with _id=1 and _id=3 should be returned by the find command.
let cursor = collection.find().sort({_id: 1}).batchSize(2);
assert.eq({_id: 1, comment: "should be seen by find command"}, cursor.next());
assert.eq({_id: 3, comment: "should be seen by find command"}, cursor.next());

// We then insert documents with _id=2 and _id=4. The document with _id=2 is positioned behind
// the _id index cursor and won't be returned by the getMore command. However, the document with
// _id=4 is positioned ahead and should end up being returned.
assert.commandWorked(collection.insert({_id: 2, comment: "should not be seen by getMore command"}));
assert.commandWorked(
    collection.insert({_id: 4, comment: "should be seen by non-snapshot getMore command"}));
assert.eq({_id: 4, comment: "should be seen by non-snapshot getMore command"}, cursor.next());
assert.eq({_id: 5, comment: "should be seen by getMore command"}, cursor.next());
assert(!cursor.hasNext());

// Using snapshot read concern with dbHash to read at the opTime of the last of the 3
// original inserts should return the same md5sum as it did originally.
res = assert.commandWorked(db.runCommand({
    dbHash: 1,
    $_internalReadAtClusterTime: clusterTime,
}));

const hashAtClusterTime = {
    collections: res.collections,
    md5: res.md5
};
assert.eq(hashAtClusterTime, hashAfterOriginalInserts);

assert.commandFailedWithCode(db.runCommand({
    dbHash: 1,
    $_internalReadAtClusterTime: new Timestamp(0, 1),
}),
                             ErrorCodes.InvalidOptions);

// Attempting to read at a clusterTime in the future should return an error.
const futureClusterTime = new Timestamp(clusterTime.getTime() + 1000, 1);

assert.commandFailedWithCode(db.runCommand({
    dbHash: 1,
    $_internalReadAtClusterTime: futureClusterTime,
}),
                             ErrorCodes.InvalidOptions);

// $_internalReadAtClusterTime is not supported in transactions.
const session = primary.startSession();
const sessionDB = session.getDatabase("test");

// dbHash is not supported in transactions at all.
session.startTransaction();
assert.commandFailedWithCode(
    sessionDB.runCommand({dbHash: 1, $_internalReadAtClusterTime: clusterTime}),
    ErrorCodes.OperationNotSupportedInTransaction);
assert.commandFailedWithCode(session.abortTransaction_forTesting(), ErrorCodes.NoSuchTransaction);

rst.stopSet();