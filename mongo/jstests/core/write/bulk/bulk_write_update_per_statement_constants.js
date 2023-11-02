/**
 * Tests bulk write cursor response for correct responses.
 *
 * The test runs commands that are not allowed with security token: bulkWrite.
 * @tags: [
 *   not_allowed_with_security_token,
 *   command_not_supported_in_serverless,
 *   # TODO SERVER-52419 Remove this tag.
 *   featureFlagBulkWriteCommand,
 *   # TODO SERVER-80009: Remove this. These tests cannot run against sharded collections because
 *   # mongos update targeting logic does not have access to the per-statement constants when
 *   #  parsing the query filter.
 *   assumes_unsharded_collection,
 * ]
 */
import {cursorEntryValidator, cursorSizeValidator} from "jstests/libs/bulk_write_utils.js";

const coll = db.getCollection("coll");
coll.drop();

// Test constants works in pipeline update.
let res = db.adminCommand({
    bulkWrite: 1,
    ops: [
        {insert: 0, document: {_id: 0, skey: "MongoDB"}},
        {insert: 0, document: {_id: 1, skey: "MongoDB2"}},
        {insert: 0, document: {_id: 2, skey: "MongoDB3"}},
        {
            update: 0,
            filter: {$expr: {$eq: ["$skey", "$$targetKey"]}},
            updateMods: [{$set: {skey: "$$replacedKey"}}],
            constants: {targetKey: "MongoDB", replacedKey: "MongoDB2"}
        },
    ],
    nsInfo: [{ns: "test.coll"}],
});

assert.commandWorked(res);
cursorSizeValidator(res, 4);
assert.eq(res.numErrors, 0, "bulkWrite command response: " + tojson(res));

cursorEntryValidator(res.cursor.firstBatch[0], {ok: 1, idx: 0, n: 1});
cursorEntryValidator(res.cursor.firstBatch[1], {ok: 1, idx: 1, n: 1});
cursorEntryValidator(res.cursor.firstBatch[2], {ok: 1, idx: 2, n: 1});
cursorEntryValidator(res.cursor.firstBatch[3], {ok: 1, idx: 3, n: 1, nModified: 1});

assert.sameMembers(
    coll.find().toArray(),
    [{_id: 0, skey: "MongoDB2"}, {_id: 1, skey: "MongoDB2"}, {_id: 2, skey: "MongoDB3"}]);

coll.drop();

// Test let matches specific document (targetKey) and constants overwrite let (replacedKey).
res = db.adminCommand({
    bulkWrite: 1,
    ops: [
        {insert: 0, document: {_id: 0, skey: "MongoDB"}},
        {insert: 0, document: {_id: 1, skey: "MongoDB2"}},
        {insert: 0, document: {_id: 2, skey: "MongoDB3"}},
        {
            update: 0,
            filter: {$expr: {$eq: ["$skey", "$$targetKey"]}},
            updateMods: [{$set: {skey: "$$replacedKey"}}],
            constants: {replacedKey: "MongoDB4"}
        },
    ],
    nsInfo: [{ns: "test.coll"}],
    let : {targetKey: "MongoDB3", replacedKey: "MongoDB2"}
});

assert.commandWorked(res);
cursorSizeValidator(res, 4);
assert.eq(res.numErrors, 0, "bulkWrite command response: " + tojson(res));

cursorEntryValidator(res.cursor.firstBatch[0], {ok: 1, idx: 0, n: 1});
cursorEntryValidator(res.cursor.firstBatch[1], {ok: 1, idx: 1, n: 1});
cursorEntryValidator(res.cursor.firstBatch[2], {ok: 1, idx: 2, n: 1});
cursorEntryValidator(res.cursor.firstBatch[3], {ok: 1, idx: 3, n: 1, nModified: 1});

const updatedDoc = coll.findOne({_id: 2});
assert.eq(updatedDoc["skey"], "MongoDB4");

coll.drop();
