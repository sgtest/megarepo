// @tags: [
//   requires_non_retryable_writes,
// ]

// Basic examples for $bit
var res;
var coll = db.update_bit;
coll.drop();

// $bit and
coll.remove({});
coll.save({_id: 1, a: NumberInt(2)});
res = coll.update({}, {$bit: {a: {and: NumberInt(4)}}});
assert.commandWorked(res);
assert.eq(coll.findOne().a, 0);

// $bit or
coll.remove({});
coll.save({_id: 1, a: NumberInt(2)});
res = coll.update({}, {$bit: {a: {or: NumberInt(4)}}});
assert.commandWorked(res);
assert.eq(coll.findOne().a, 6);

// $bit xor
coll.remove({});
coll.save({_id: 1, a: NumberInt(0)});
res = coll.update({}, {$bit: {a: {xor: NumberInt(4)}}});
assert.commandWorked(res);
assert.eq(coll.findOne().a, 4);

// SERVER-19706 Empty bit operation.
res = coll.update({}, {$bit: {a: {}}});
assert.writeError(res);

// Make sure $bit on index arrays 9 and 10 when padding is needed works.
assert.commandWorked(coll.insert({_id: 2, a: [0]}));
assert.commandWorked(
    coll.update({_id: 2}, {$bit: {"a.9": {or: NumberInt(0)}, "a.10": {or: NumberInt(0)}}}));
res = coll.find({_id: 2}).toArray();
assert.eq(res[0]["a"], [0, null, null, null, null, null, null, null, null, 0, 0]);
