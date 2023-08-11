/**
 * Tests bulk write to timeseries collections.
 *
 * The test runs commands that are not allowed with security token: bulkWrite.
 * @tags: [
 *   assumes_against_mongod_not_mongos,
 *   command_not_supported_in_serverless,
 *   requires_timeseries,
 *   # TODO SERVER-52419 Remove this tag.
 *   featureFlagBulkWriteCommand,
 * ]
 */
import {cursorEntryValidator} from "jstests/libs/bulk_write_utils.js";

const coll = db.getCollection("t");
const nonTSColl = db.getCollection("c");

const timeFieldName = 'time';

coll.drop();
nonTSColl.drop();
assert.commandWorked(db.createCollection(coll.getName(), {timeseries: {timeField: timeFieldName}}));
assert.commandWorked(db.createCollection(nonTSColl.getName()));

// Test basic ordered timeseries inserts.
let docs = [
    {_id: 0, [timeFieldName]: ISODate(), num: 0},
    {_id: 1, [timeFieldName]: ISODate(), num: 1},
    {_id: 2, [timeFieldName]: ISODate(), num: 2},
];
let res = db.adminCommand({
    bulkWrite: 1,
    ops: docs.map((doc) => ({insert: 0, document: doc})),
    nsInfo: [{ns: coll.getFullName()}],
    ordered: true,
});
assert.commandWorked(res);
assert.eq(res.numErrors, 0);
res.cursor.firstBatch.forEach((entry, idx) => cursorEntryValidator(entry, {ok: 1, idx: idx, n: 1}));
assert.docEq(docs, coll.find().sort({_id: 1}).toArray());

// Test ordered timeseries inserts with failed operations.
docs = [
    {_id: 3, [timeFieldName]: ISODate(), num: 3},
    {_id: 99, num: 99},  // Missing 'time' field.
    {_id: 4, [timeFieldName]: ISODate(), num: 4},
];
res = db.adminCommand({
    bulkWrite: 1,
    ops: docs.map((doc) => ({insert: 0, document: doc})),
    nsInfo: [{ns: coll.getFullName()}],
    ordered: true,
});
assert.eq(res.numErrors, 1);
cursorEntryValidator(res.cursor.firstBatch[0], {ok: 1, idx: 0, n: 1})
cursorEntryValidator(res.cursor.firstBatch[1], {ok: 0, idx: 1, code: 2, n: 0})
assert.eq(res.cursor.firstBatch.length, 2);
assert.eq(coll.countDocuments({}), 4);

// Test unordered timeseries inserts with failed operations.
docs = [
    {_id: 4, [timeFieldName]: ISODate(), num: 4},
    {_id: 99, num: 99},  // Missing 'time' field.
    {_id: 5, [timeFieldName]: ISODate(), num: 5},
];
res = db.adminCommand({
    bulkWrite: 1,
    ops: docs.map((doc) => ({insert: 0, document: doc})),
    nsInfo: [{ns: coll.getFullName()}],
    ordered: false,
});
assert.eq(res.numErrors, 1);
cursorEntryValidator(res.cursor.firstBatch[0], {ok: 1, idx: 0, n: 1})
cursorEntryValidator(res.cursor.firstBatch[1], {ok: 0, idx: 1, code: 2, n: 0})
cursorEntryValidator(res.cursor.firstBatch[2], {ok: 1, idx: 2, n: 1})
assert.eq(coll.countDocuments({}), 6);

// Test unordered inserts to 2 collections - 1 timeseries collection and 1 non-timeseries
// collections.
res = db.adminCommand({
    bulkWrite: 1,
    ops: [
        {insert: 0, document: {_id: 6, [timeFieldName]: ISODate(), num: 6}},
        {insert: 0, document: {_id: 7, [timeFieldName]: ISODate(), num: 7}},
        {insert: 1, document: {_id: 0, num: 0}},
        {insert: 1, document: {_id: 0, num: 1}},  // Duplicate key.
        {insert: 0, document: {_id: 8, [timeFieldName]: ISODate(), num: 8}},
        {insert: 0, document: {_id: 99, num: 99}},  // Missing 'time' field.
    ],
    nsInfo: [{ns: coll.getFullName()}, {ns: nonTSColl.getFullName()}],
    ordered: false,
});
assert.eq(res.numErrors, 2);
cursorEntryValidator(res.cursor.firstBatch[3], {ok: 0, idx: 3, code: 11000, n: 0})
cursorEntryValidator(res.cursor.firstBatch[5], {ok: 0, idx: 5, code: 2, n: 0})
assert.eq(coll.countDocuments({}), 9);
assert.eq(nonTSColl.countDocuments({}), 1);

// Test ordered inserts to 2 collections - 1 timeseries collection and 1 non-timeseries collections.
res = db.adminCommand({
    bulkWrite: 1,
    ops: [
        {insert: 1, document: {_id: 1, num: 1}},
        {insert: 0, document: {_id: 10, [timeFieldName]: ISODate(), num: 10}},
        {insert: 0, document: {_id: 99, num: 99}},  // Missing 'time' field.
        {insert: 1, document: {_id: 2, num: 2}},
    ],
    nsInfo: [{ns: coll.getFullName()}, {ns: nonTSColl.getFullName()}],
    ordered: true,
});
jsTestLog(tojson(res));
assert.eq(res.numErrors, 1);
cursorEntryValidator(res.cursor.firstBatch[2], {ok: 0, idx: 2, code: 2, n: 0})
assert.eq(res.cursor.firstBatch.length, 3);
assert.eq(coll.countDocuments({}), 10);
assert.eq(nonTSColl.countDocuments({}), 2);
