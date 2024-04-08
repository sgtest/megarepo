// Cannot implicitly shard accessed collections because of following errmsg: A single
let t = db.find_and_modify_server7660;
t.drop();

let a = t.findAndModify(
    {query: {foo: 'bar'}, update: {$set: {bob: 'john'}}, sort: {foo: 1}, upsert: true, new: true});

let b = t.findOne();
assert.eq(a, b);
assert.eq("bar", a.foo);
assert.eq("john", a.bob);
