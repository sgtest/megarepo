// @tags: [
//   assumes_write_concern_unchanged,
//   requires_non_retryable_writes,
//   requires_fastcount,
//   # TODO SERVER-89461 Investigate why test using huge batch size timeout in suites with balancer
//   assumes_balancer_off,
// ]

//
// Ensures that mongod respects the batch write protocols for delete
//

var coll = db.getCollection("batch_write_delete");
coll.drop();

var request;
var result;
var batch;

var maxWriteBatchSize = db.hello().maxWriteBatchSize;

function resultOK(result) {
    return result.ok && !('code' in result) && !('errmsg' in result) && !('errInfo' in result) &&
        !('writeErrors' in result);
}

function resultNOK(result) {
    return !result.ok && typeof (result.code) == 'number' && typeof (result.errmsg) == 'string';
}

function countEventually(collection, n) {
    assert.soon(
        function() {
            return collection.count() === n;
        },
        function() {
            return "unacknowledged write timed out";
        });
}

// EACH TEST BELOW SHOULD BE SELF-CONTAINED, FOR EASIER DEBUGGING

//
// NO DOCS, illegal command
coll.drop();
coll.insert({a: 1});
request = {
    delete: coll.getName()
};
result = coll.runCommand(request);
assert(resultNOK(result), tojson(result));
assert.eq(1, coll.count());

//
// Single document remove, default write concern specified
coll.drop();
coll.insert({a: 1});
request = {
    delete: coll.getName(),
    deletes: [{q: {a: 1}, limit: 1}]
};
result = coll.runCommand(request);
assert(resultOK(result), tojson(result));
assert.eq(1, result.n);
assert.eq(0, coll.count());

//
// Single document remove, w:1 write concern specified, ordered:true
coll.drop();
coll.insert([{a: 1}, {a: 1}]);
request = {
    delete: coll.getName(),
    deletes: [{q: {a: 1}, limit: 1}],
    writeConcern: {w: 1},
    ordered: false
};
result = coll.runCommand(request);
assert(resultOK(result), tojson(result));
assert.eq(1, result.n);
assert.eq(1, coll.count());

//
// Multiple document remove, w:1 write concern specified, ordered:true, default top
coll.drop();
coll.insert([{a: 1}, {a: 1}]);
request = {
    delete: coll.getName(),
    deletes: [{q: {a: 1}, limit: 0}],
    writeConcern: {w: 1},
    ordered: false
};
result = coll.runCommand(request);
assert(resultOK(result), tojson(result));
assert.eq(2, result.n);
assert.eq(0, coll.count());

//
// Multiple document remove, w:1 write concern specified, ordered:true, top:0
coll.drop();
coll.insert([{a: 1}, {a: 1}]);
request = {
    delete: coll.getName(),
    deletes: [{q: {a: 1}, limit: 0}],
    writeConcern: {w: 1},
    ordered: false
};
result = coll.runCommand(request);
assert(resultOK(result), tojson(result));
assert.eq(2, result.n);
assert.eq(0, coll.count());

//
// Large batch under the size threshold should delete successfully
coll.drop();
batch = [];
var insertBatch = coll.initializeUnorderedBulkOp();
for (var i = 0; i < maxWriteBatchSize; ++i) {
    insertBatch.insert({_id: i});
    batch.push({q: {_id: i}, limit: 0});
}
assert.commandWorked(insertBatch.execute());
request = {
    delete: coll.getName(),
    deletes: batch,
    writeConcern: {w: 1},
    ordered: false
};
result = coll.runCommand(request);
assert(resultOK(result), tojson(result));
assert.eq(batch.length, result.n);
assert.eq(0, coll.count());

//
// Large batch above the size threshold should fail to delete
coll.drop();
batch = [];
var insertBatch = coll.initializeUnorderedBulkOp();
for (var i = 0; i < maxWriteBatchSize + 1; ++i) {
    insertBatch.insert({_id: i});
    batch.push({q: {_id: i}, limit: 0});
}
assert.commandWorked(insertBatch.execute());
request = {
    delete: coll.getName(),
    deletes: batch,
    writeConcern: {w: 1},
    ordered: false
};
result = coll.runCommand(request);
assert(resultNOK(result), tojson(result));
assert.eq(batch.length, coll.count());

//
// Cause remove error using ordered:true
coll.drop();
coll.insert({a: 1});
request = {
    delete: coll.getName(),
    deletes: [{q: {a: 1}, limit: 0}, {q: {$set: {a: 1}}, limit: 0}, {q: {$set: {a: 1}}, limit: 0}],
    writeConcern: {w: 1},
    ordered: true
};
result = coll.runCommand(request);
assert.commandWorkedIgnoringWriteErrors(result);
assert.eq(1, result.n);
assert(result.writeErrors != null);
assert.eq(1, result.writeErrors.length);

assert.eq(1, result.writeErrors[0].index);
assert.eq('number', typeof result.writeErrors[0].code);
assert.eq('string', typeof result.writeErrors[0].errmsg);
assert.eq(0, coll.count());

//
// Cause remove error using ordered:false
coll.drop();
coll.insert({a: 1});
request = {
    delete: coll.getName(),
    deletes: [{q: {$set: {a: 1}}, limit: 0}, {q: {$set: {a: 1}}, limit: 0}, {q: {a: 1}, limit: 0}],
    writeConcern: {w: 1},
    ordered: false
};
result = coll.runCommand(request);
assert.commandWorkedIgnoringWriteErrors(result);
assert.eq(1, result.n);
assert.eq(2, result.writeErrors.length);

assert.eq(0, result.writeErrors[0].index);
assert.eq('number', typeof result.writeErrors[0].code);
assert.eq('string', typeof result.writeErrors[0].errmsg);

assert.eq(1, result.writeErrors[1].index);
assert.eq('number', typeof result.writeErrors[1].code);
assert.eq('string', typeof result.writeErrors[1].errmsg);
assert.eq(0, coll.count());
