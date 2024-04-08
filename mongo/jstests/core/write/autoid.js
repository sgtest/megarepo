let f = db.jstests_autoid;
f.drop();

f.save({z: 1});
let a = f.findOne({z: 1});
f.update({z: 1}, {z: 2});
let b = f.findOne({z: 2});
assert.eq(a._id.str, b._id.str);
let c = f.update({z: 2}, {z: "abcdefgabcdefgabcdefg"});
c = f.findOne({});
assert.eq(a._id.str, c._id.str);
