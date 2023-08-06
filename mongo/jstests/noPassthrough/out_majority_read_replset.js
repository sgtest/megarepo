// Tests the $out and read concern majority.
// @tags: [
//   requires_majority_read_concern,
// ]
import {
    restartReplicationOnSecondaries,
    stopReplicationOnSecondaries
} from "jstests/libs/write_concern_util.js";

const rst = new ReplSetTest({nodes: 2, nodeOptions: {enableMajorityReadConcern: ""}});

rst.startSet();
rst.initiate();

const name = "out_majority_read";
const db = rst.getPrimary().getDB(name);
const sourceColl = db.sourceColl;

assert.commandWorked(sourceColl.insert({_id: 1, state: 'before'}));
rst.awaitLastOpCommitted();

stopReplicationOnSecondaries(rst);

// Rename the collection temporarily and then back to its original name. This advances the minimum
// visible snapshot and forces the $out to block until its snapshot advances.
const tempColl = db.getName() + '.temp';
assert.commandWorked(db.adminCommand({
    renameCollection: sourceColl.getFullName(),
    to: tempColl,
}));
assert.commandWorked(db.adminCommand({
    renameCollection: tempColl,
    to: sourceColl.getFullName(),
}));

// Create the index that is not majority committed
assert.commandWorked(sourceColl.createIndex({state: 1}, {name: "secondIndex"}, 0));

// Run the $out in the parallel shell as it will block in the metadata until the snapshot is
// advanced. This will no longer block with point-in-time reads as a new collection instance is
// created internally when reading before the minimum visible snapshot.
const awaitShell = startParallelShell(`{
        const testDB = db.getSiblingDB("${name}");
        const sourceColl = testDB.sourceColl;

        // Run $out and make sure the {state:1} index is carried over.
        const res = sourceColl.aggregate([{$out: sourceColl.getName()}],
                                         {readConcern: {level: 'majority'}});

        assert.eq(res.itcount(), 0);

        const indexes = sourceColl.getIndexes();
        assert.eq(indexes.length, 2);
        assert.eq(indexes[0].name, "_id_");
        assert.eq(indexes[1].name, "secondIndex");
    }`,
                                          db.getMongo().port);

// Restart data replication and wait until the new write becomes visible.
restartReplicationOnSecondaries(rst);
rst.awaitLastOpCommitted();

awaitShell();

rst.stopSet();