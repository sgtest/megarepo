/*
 * Test SERVER-18622 listCollections should special case filtering by name.
 *
 * The test runs commands that are not allowed with security token: applyOps.
 * @tags: [
 *   not_allowed_with_security_token,
 *   requires_replication,
 *   # applyOps is not supported on mongos
 *   assumes_against_mongod_not_mongos,
 *   # Tenant migrations don't support applyOps.
 *   tenant_migration_incompatible,
 * ]
 */

(function() {
"use strict";
var mydb = db.getSiblingDB("list_collections_filter");
assert.commandWorked(mydb.dropDatabase());

// Make some collections.
assert.commandWorked(mydb.createCollection("lists"));
assert.commandWorked(mydb.createCollection("ordered_sets"));
assert.commandWorked(mydb.createCollection("unordered_sets"));
assert.commandWorked(mydb.runCommand(
    {applyOps: [{op: "c", ns: mydb.getName() + ".$cmd", o: {create: "arrays_temp", temp: true}}]}));

/**
 * Asserts that the names of the collections returned from running the listCollections
 * command with the given filter match the expected names.
 */
function testListCollections(filter, expectedNames) {
    if (filter === undefined) {
        filter = {};
    }

    var cursor = new DBCommandCursor(mydb, mydb.runCommand("listCollections", {filter: filter}));
    function stripToName(result) {
        return result.name;
    }
    var cursorResultNames = cursor.toArray().map(stripToName);

    assert.eq(cursorResultNames.sort(), expectedNames.sort());

    // Assert the shell helper returns the same list, but in sorted order.
    var shellResultNames = mydb.getCollectionInfos(filter).map(stripToName);
    assert.eq(shellResultNames, expectedNames.sort());
}

// No filter.
testListCollections({}, ["lists", "ordered_sets", "unordered_sets", "arrays_temp"]);

// Filter without name.
testListCollections({options: {}}, ["lists", "ordered_sets", "unordered_sets"]);

// Filter with exact match on name.
testListCollections({name: "lists"}, ["lists"]);
testListCollections({name: "non-existent"}, []);
testListCollections({name: ""}, []);
testListCollections({name: 1234}, []);

// Filter with $in.
testListCollections({name: {$in: ["lists"]}}, ["lists"]);
testListCollections({name: {$in: []}}, []);
testListCollections({name: {$in: ["lists", "ordered_sets", "non-existent", "", 1234]}},
                    ["lists", "ordered_sets"]);
// With a regex.
testListCollections({name: {$in: ["lists", /.*_sets$/, "non-existent", "", 1234]}},
                    ["lists", "ordered_sets", "unordered_sets"]);

// Filter with $and.
testListCollections({name: "lists", options: {}}, ["lists"]);
testListCollections({name: "lists", options: {temp: true}}, []);
testListCollections({$and: [{name: "lists"}, {options: {temp: true}}]}, []);
testListCollections({name: "arrays_temp", options: {temp: true}}, ["arrays_temp"]);

// Filter with $and and $in.
testListCollections({name: {$in: ["lists", /.*_sets$/]}, options: {}},
                    ["lists", "ordered_sets", "unordered_sets"]);
testListCollections({
    $and: [
        {name: {$in: ["lists", /.*_sets$/]}},
        {name: "lists"},
        {options: {}},
    ]
},
                    ["lists"]);
testListCollections({
    $and: [
        {name: {$in: ["lists", /.*_sets$/]}},
        {name: "non-existent"},
        {options: {}},
    ]
},
                    []);

// Filter with $expr.
testListCollections({$expr: {$eq: ["$name", "lists"]}}, ["lists"]);

// Filter with $expr with an unbound variable.
assert.throws(function() {
    mydb.getCollectionInfos({$expr: {$eq: ["$name", "$$unbound"]}});
});

// Filter with $expr with a runtime error.
assert.throws(function() {
    mydb.getCollectionInfos({$expr: {$abs: "$name"}});
});

// No extensions are allowed in filters.
assert.throws(function() {
    mydb.getCollectionInfos({$text: {$search: "str"}});
});
assert.throws(function() {
    mydb.getCollectionInfos({
        $where: function() {
            return true;
        }
    });
});
assert.throws(function() {
    mydb.getCollectionInfos({a: {$nearSphere: {$geometry: {type: "Point", coordinates: [0, 0]}}}});
});
}());
