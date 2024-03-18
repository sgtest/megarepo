/**
 * Tests dropping the system.js collection.
 *
 * @tags: [
 *   assumes_read_preference_unchanged,
 *   assumes_unsharded_collection,
 *   requires_fcv_62,
 *   requires_non_retryable_writes,
 *   # Uses $where operator.
 *   requires_scripting,
 *   requires_system_dot_js_stored_functions,
 * ]
 */
const testDB = db.getSiblingDB(jsTestName());
assert.commandWorked(testDB.dropDatabase());

const coll = testDB.getCollection('coll');
const systemJs = testDB.getCollection('system.js');

assert.commandWorked(coll.insert([{name: 'Alice', age: 20}, {name: 'Bob', age: 18}]));

assert.commandWorked(systemJs.insert({
    _id: "isTeenager",
    value: function(age) {
        return age >= 13 && age <= 19;
    },
}));

assert.commandWorked(
    testDB.runCommand({find: coll.getName(), filter: {$where: "isTeenager(this.age)"}}));

assert(systemJs.drop());

assert.commandFailedWithCode(
    testDB.runCommand({find: coll.getName(), filter: {$where: "isTeenager(this.age)"}}),
    ErrorCodes.JSInterpreterFailure);
