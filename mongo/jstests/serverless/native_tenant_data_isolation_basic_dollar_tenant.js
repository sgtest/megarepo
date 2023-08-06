// Test basic db operations in multitenancy using $tenant.

import {arrayEq} from "jstests/aggregation/extras/utils.js";
import {FeatureFlagUtil} from "jstests/libs/feature_flag_util.js";

const rst = new ReplSetTest({
    nodes: 3,
    nodeOptions: {
        auth: '',
        setParameter: {
            multitenancySupport: true,
        }
    }
});
rst.startSet({keyFile: 'jstests/libs/key1'});
rst.initiate();

const primary = rst.getPrimary();
const adminDb = primary.getDB('admin');

// Prepare a user for testing pass tenant via $tenant.
// Must be authenticated as a user with ActionType::useTenant in order to use $tenant.
assert.commandWorked(adminDb.runCommand({createUser: 'admin', pwd: 'pwd', roles: ['root']}));
assert(adminDb.auth('admin', 'pwd'));

const featureFlagRequireTenantId = FeatureFlagUtil.isEnabled(adminDb, "RequireTenantID");

const kTenant = ObjectId();
const kOtherTenant = ObjectId();
const kDbName = 'myDb';
const kCollName = 'myColl';
const testDb = primary.getDB(kDbName);
const testColl = testDb.getCollection(kCollName);

// In this jstest, the collection (defined by kCollName) and the document "{_id: 0, a: 1, b: 1}"
// for the tenant (defined by kTenant) will be reused by all command tests. So, any test which
// changes the collection name or document should reset it.

// Test create and listCollections commands, plus $listCatalog aggregate, on collection.
{
    const viewName = "view1";
    const targetViews = 'system.views';

    // Create a collection for the tenant kTenant, and then create a view on the collection.
    assert.commandWorked(
        testColl.getDB().createCollection(testColl.getName(), {'$tenant': kTenant}));
    assert.commandWorked(testDb.runCommand(
        {"create": viewName, "viewOn": kCollName, pipeline: [], '$tenant': kTenant}));

    const colls = assert.commandWorked(
        testDb.runCommand({listCollections: 1, nameOnly: true, '$tenant': kTenant}));
    assert.eq(3, colls.cursor.firstBatch.length, tojson(colls.cursor.firstBatch));
    const expectedColls = [
        {"name": kCollName, "type": "collection"},
        {"name": targetViews, "type": "collection"},
        {"name": viewName, "type": "view"}
    ];
    assert(arrayEq(expectedColls, colls.cursor.firstBatch), tojson(colls.cursor.firstBatch));

    const prefixedDbName = kTenant + '_' + testDb.getName();
    const targetDb = featureFlagRequireTenantId ? testDb.getName() : prefixedDbName;

    // Get catalog without specifying target collection (collectionless).
    let result = adminDb.runCommand(
        {aggregate: 1, pipeline: [{$listCatalog: {}}], cursor: {}, '$tenant': kTenant});
    let resultArray = result.cursor.firstBatch;

    // Check that the resulting array of catalog entries contains our target databases and
    // namespaces.
    assert(resultArray.some((entry) => (entry.db === targetDb) && (entry.name === kCollName)),
           tojson(resultArray));

    // Also check that the resulting array contains views specific to our target database.
    assert(resultArray.some((entry) => (entry.db === targetDb) && (entry.name === targetViews)),
           tojson(resultArray));
    assert(resultArray.some((entry) => (entry.db === targetDb) && (entry.name === viewName)),
           tojson(resultArray));

    // Get catalog when specifying our target collection, which should only return one result.
    result = testDb.runCommand({
        aggregate: testColl.getName(),
        pipeline: [{$listCatalog: {}}],
        cursor: {},
        '$tenant': kTenant
    });
    resultArray = result.cursor.firstBatch;

    // Check that the resulting array of catalog entries contains our target database and
    // namespace.
    assert.eq(resultArray.length, 1, tojson(resultArray));
    assert(resultArray.some((entry) => (entry.db === targetDb) && (entry.name === kCollName)),
           tojson(resultArray));

    // These collections should not be accessed with a different tenant.
    const collsWithDiffTenant = assert.commandWorked(
        testDb.runCommand({listCollections: 1, nameOnly: true, '$tenant': kOtherTenant}));
    assert.eq(0,
              collsWithDiffTenant.cursor.firstBatch.length,
              tojson(collsWithDiffTenant.cursor.firstBatch));
}

// Test listDatabases command.
{
    // Create databases for kTenant. A new database is implicitly created when a collection is
    // created.
    const kOtherDbName = 'otherDb';
    assert.commandWorked(
        primary.getDB(kOtherDbName).createCollection(kCollName, {'$tenant': kTenant}));

    const dbs = assert.commandWorked(
        adminDb.runCommand({listDatabases: 1, nameOnly: true, '$tenant': kTenant}));
    assert.eq(2, dbs.databases.length, tojson(dbs));
    // The 'admin' database is not expected because we do not create a tenant user in this test.
    const expectedDbs = featureFlagRequireTenantId
        ? [kDbName, kOtherDbName]
        : [kTenant + "_" + kDbName, kTenant + "_" + kOtherDbName];
    assert(arrayEq(expectedDbs, dbs.databases.map(db => db.name)), tojson(dbs));

    // These databases should not be accessed with a different tenant.
    const dbsWithDiffTenant = assert.commandWorked(
        adminDb.runCommand({listDatabases: 1, nameOnly: true, '$tenant': kOtherTenant}));
    assert.eq(0, dbsWithDiffTenant.databases.length, tojson(dbsWithDiffTenant));

    const allDbs = assert.commandWorked(adminDb.runCommand({listDatabases: 1, nameOnly: true}));
    expectedDbs.push("admin");
    expectedDbs.push("config");
    expectedDbs.push("local");

    assert.eq(5, allDbs.databases.length, tojson(allDbs));
    assert(arrayEq(expectedDbs, allDbs.databases.map(db => db.name)), tojson(allDbs));
}

// Test insert, agg, find, getMore, and explain commands.
{
    const kTenantDocs = [{w: 0}, {x: 1}, {y: 2}, {z: 3}];
    const kOtherTenantDocs = [{i: 1}, {j: 2}, {k: 3}];

    assert.commandWorked(
        testDb.runCommand({insert: kCollName, documents: kTenantDocs, '$tenant': kTenant}));
    assert.commandWorked(testDb.runCommand(
        {insert: kCollName, documents: kOtherTenantDocs, '$tenant': kOtherTenant}));

    // Check that find only returns documents from the correct tenant
    const findRes = assert.commandWorked(
        testDb.runCommand({find: kCollName, projection: {_id: 0}, '$tenant': kTenant}));
    assert.eq(
        kTenantDocs.length, findRes.cursor.firstBatch.length, tojson(findRes.cursor.firstBatch));
    assert(arrayEq(kTenantDocs, findRes.cursor.firstBatch), tojson(findRes.cursor.firstBatch));

    const findRes2 = assert.commandWorked(
        testDb.runCommand({find: kCollName, projection: {_id: 0}, '$tenant': kOtherTenant}));
    assert.eq(kOtherTenantDocs.length,
              findRes2.cursor.firstBatch.length,
              tojson(findRes2.cursor.firstBatch));
    assert(arrayEq(kOtherTenantDocs, findRes2.cursor.firstBatch),
           tojson(findRes2.cursor.firstBatch));

    // Test that getMore only works on a tenant's own cursor
    const cmdRes = assert.commandWorked(testDb.runCommand(
        {find: kCollName, projection: {_id: 0}, batchSize: 1, '$tenant': kTenant}));
    assert.eq(cmdRes.cursor.firstBatch.length, 1, tojson(cmdRes.cursor.firstBatch));
    assert.commandWorked(
        testDb.runCommand({getMore: cmdRes.cursor.id, collection: kCollName, '$tenant': kTenant}));

    const cmdRes2 = assert.commandWorked(testDb.runCommand(
        {find: kCollName, projection: {_id: 0}, batchSize: 1, '$tenant': kTenant}));
    assert.commandFailedWithCode(
        testDb.runCommand(
            {getMore: cmdRes2.cursor.id, collection: kCollName, '$tenant': kOtherTenant}),
        ErrorCodes.Unauthorized);

    // Test that aggregate only finds a tenant's own document.
    const aggRes = assert.commandWorked(testDb.runCommand({
        aggregate: kCollName,
        pipeline: [{$match: {w: 0}}, {$project: {_id: 0}}],
        cursor: {},
        '$tenant': kTenant
    }));
    assert.eq(1, aggRes.cursor.firstBatch.length, tojson(aggRes.cursor.firstBatch));
    assert.eq(kTenantDocs[0], aggRes.cursor.firstBatch[0], tojson(aggRes.cursor.firstBatch));

    const aggRes2 = assert.commandWorked(testDb.runCommand({
        aggregate: kCollName,
        pipeline: [{$match: {i: 1}}, {$project: {_id: 0}}],
        cursor: {},
        '$tenant': kOtherTenant
    }));
    assert.eq(1, aggRes2.cursor.firstBatch.length, tojson(aggRes2.cursor.firstBatch));
    assert.eq(kOtherTenantDocs[0], aggRes2.cursor.firstBatch[0], tojson(aggRes2.cursor.firstBatch));

    // Test that explain works correctly.
    const kTenantExplainRes = assert.commandWorked(testDb.runCommand(
        {explain: {find: kCollName}, verbosity: 'executionStats', '$tenant': kTenant}));
    assert.eq(
        kTenantDocs.length, kTenantExplainRes.executionStats.nReturned, tojson(kTenantExplainRes));
    const kOtherTenantExplainRes = assert.commandWorked(testDb.runCommand(
        {explain: {find: kCollName}, verbosity: 'executionStats', '$tenant': kOtherTenant}));
    assert.eq(kOtherTenantDocs.length,
              kOtherTenantExplainRes.executionStats.nReturned,
              tojson(kOtherTenantExplainRes));
}

// Test insert and findAndModify command.
{
    assert.commandWorked(testDb.runCommand(
        {insert: kCollName, documents: [{_id: 0, a: 1, b: 1}], '$tenant': kTenant}));

    const fad1 = assert.commandWorked(testDb.runCommand(
        {findAndModify: kCollName, query: {a: 1}, update: {$inc: {a: 10}}, '$tenant': kTenant}));
    assert.eq({_id: 0, a: 1, b: 1}, fad1.value, tojson(fad1));
    const fad2 = assert.commandWorked(testDb.runCommand({
        findAndModify: kCollName,
        query: {a: 11},
        update: {$set: {a: 1, b: 1}},
        '$tenant': kTenant
    }));
    assert.eq({_id: 0, a: 11, b: 1}, fad2.value, tojson(fad2));
    // This document should not be accessed with a different tenant.
    const fadOtherUser = assert.commandWorked(testDb.runCommand({
        findAndModify: kCollName,
        query: {b: 1},
        update: {$inc: {b: 10}},
        '$tenant': kOtherTenant
    }));
    assert.eq(null, fadOtherUser.value, tojson(fadOtherUser));
}

// Test count and distinct command.
{
    assert.commandWorked(testDb.runCommand(
        {insert: kCollName, documents: [{c: 1, d: 1}, {c: 1, d: 2}], '$tenant': kTenant}));

    // Test count command.
    const resCount = assert.commandWorked(
        testDb.runCommand({count: kCollName, query: {c: 1}, '$tenant': kTenant}));
    assert.eq(2, resCount.n, tojson(resCount));
    const resCountOtherUser = assert.commandWorked(
        testDb.runCommand({count: kCollName, query: {c: 1}, '$tenant': kOtherTenant}));
    assert.eq(0, resCountOtherUser.n, tojson(resCountOtherUser));

    // Test Distict command.
    const resDistinct = assert.commandWorked(
        testDb.runCommand({distinct: kCollName, key: 'd', query: {}, '$tenant': kTenant}));
    assert.eq([1, 2], resDistinct.values.sort(), tojson(resDistinct));
    const resDistinctOtherUser = assert.commandWorked(
        testDb.runCommand({distinct: kCollName, key: 'd', query: {}, '$tenant': kOtherTenant}));
    assert.eq([], resDistinctOtherUser.values, tojson(resDistinctOtherUser));
}

// Test renameCollection command.
{
    const fromName = kDbName + "." + kCollName;
    const toName = fromName + "_renamed";
    assert.commandWorked(adminDb.runCommand(
        {renameCollection: fromName, to: toName, dropTarget: true, '$tenant': kTenant}));

    // Verify the the renamed collection by findAndModify existing documents.
    const fad1 = assert.commandWorked(testDb.runCommand({
        findAndModify: kCollName + "_renamed",
        query: {a: 1},
        update: {$inc: {a: 10}},
        '$tenant': kTenant
    }));
    assert.eq({_id: 0, a: 1, b: 1}, fad1.value, tojson(fad1));

    // This collection should not be accessed with a different tenant.
    assert.commandFailedWithCode(
        adminDb.runCommand(
            {renameCollection: toName, to: fromName, dropTarget: true, '$tenant': kOtherTenant}),
        ErrorCodes.NamespaceNotFound);

    // Reset the collection to be used below
    assert.commandWorked(adminDb.runCommand(
        {renameCollection: toName, to: fromName, dropTarget: true, '$tenant': kTenant}));
}

// Test the dropCollection and dropDatabase commands.
{
    // Another tenant shouldn't be able to drop the collection or database.
    assert.commandWorked(testDb.runCommand({drop: kCollName, '$tenant': kOtherTenant}));
    const collsAfterDropCollectionByOtherTenant = assert.commandWorked(testDb.runCommand(
        {listCollections: 1, nameOnly: true, filter: {name: kCollName}, '$tenant': kTenant}));
    assert.eq(1,
              collsAfterDropCollectionByOtherTenant.cursor.firstBatch.length,
              tojson(collsAfterDropCollectionByOtherTenant.cursor.firstBatch));

    assert.commandWorked(testDb.runCommand({dropDatabase: 1, '$tenant': kOtherTenant}));
    const collsAfterDropDbByOtherTenant = assert.commandWorked(testDb.runCommand(
        {listCollections: 1, nameOnly: true, filter: {name: kCollName}, '$tenant': kTenant}));
    assert.eq(1,
              collsAfterDropDbByOtherTenant.cursor.firstBatch.length,
              tojson(collsAfterDropDbByOtherTenant.cursor.firstBatch));

    // Now, drop the collection using the original tenantId.
    assert.commandWorked(testDb.runCommand({drop: kCollName, '$tenant': kTenant}));
    const collsAfterDropCollection = assert.commandWorked(testDb.runCommand(
        {listCollections: 1, nameOnly: true, filter: {name: kCollName}, '$tenant': kTenant}));
    assert.eq(0,
              collsAfterDropCollection.cursor.firstBatch.length,
              tojson(collsAfterDropCollection.cursor.firstBatch));

    // Now, drop the database using the original tenantId.
    assert.commandWorked(testDb.runCommand({dropDatabase: 1, '$tenant': kTenant}));
    const collsAfterDropDb = assert.commandWorked(testDb.runCommand(
        {listCollections: 1, nameOnly: true, filter: {name: kCollName}, '$tenant': kTenant}));
    assert.eq(
        0, collsAfterDropDb.cursor.firstBatch.length, tojson(collsAfterDropDb.cursor.firstBatch));

    // Reset the collection so other test cases can still access this collection with kCollName
    // after this test.
    assert.commandWorked(testDb.runCommand(
        {insert: kCollName, documents: [{_id: 0, a: 1, b: 1}], '$tenant': kTenant}));
}

// Test that transactions can be run successfully.
{
    const lsid = assert.commandWorked(testDb.runCommand({startSession: 1, $tenant: kTenant})).id;
    assert.commandWorked(testDb.runCommand({
        delete: kCollName,
        deletes: [{q: {_id: 0, a: 1, b: 1}, limit: 1}],
        startTransaction: true,
        lsid: lsid,
        txnNumber: NumberLong(0),
        autocommit: false,
        '$tenant': kTenant
    }));
    assert.commandWorked(testDb.adminCommand({
        commitTransaction: 1,
        lsid: lsid,
        txnNumber: NumberLong(0),
        autocommit: false,
        $tenant: kTenant
    }));

    const findRes = assert.commandWorked(testDb.runCommand({find: kCollName, '$tenant': kTenant}));
    assert.eq(0, findRes.cursor.firstBatch.length, tojson(findRes.cursor.firstBatch));

    // Reset the collection so other test cases can still access this collection with kCollName
    // after this test.
    assert.commandWorked(testDb.runCommand(
        {insert: kCollName, documents: [{_id: 0, a: 1, b: 1}], '$tenant': kTenant}));
}

// Test createIndexes, listIndexes and dropIndexes command.
{
    var sortIndexesByName = function(indexes) {
        return indexes.sort(function(a, b) {
            return a.name > b.name;
        });
    };

    var getIndexesKeyAndName = function(indexes) {
        return sortIndexesByName(indexes).map(function(index) {
            return {key: index.key, name: index.name};
        });
    };

    let res = assert.commandWorked(testDb.runCommand({
        createIndexes: kCollName,
        indexes: [{key: {a: 1}, name: "indexA"}, {key: {b: 1}, name: "indexB"}],
        '$tenant': kTenant
    }));
    assert.eq(3, res.numIndexesAfter, tojson(res));

    res = assert.commandWorked(testDb.runCommand({listIndexes: kCollName, '$tenant': kTenant}));
    assert.eq(3, res.cursor.firstBatch.length, tojson(res.cursor.firstBatch));
    assert(arrayEq(
               [
                   {key: {"_id": 1}, name: "_id_"},
                   {key: {a: 1}, name: "indexA"},
                   {key: {b: 1}, name: "indexB"}
               ],
               getIndexesKeyAndName(res.cursor.firstBatch)),
           tojson(res.cursor.firstBatch));

    // These indexes should not be accessed with a different tenant.
    assert.commandFailedWithCode(
        testDb.runCommand({listIndexes: kCollName, '$tenant': kOtherTenant}),
        ErrorCodes.NamespaceNotFound);
    assert.commandFailedWithCode(
        testDb.runCommand(
            {dropIndexes: kCollName, index: ["indexA", "indexB"], '$tenant': kOtherTenant}),
        ErrorCodes.NamespaceNotFound);

    // Drop those new created indexes.
    res = assert.commandWorked(testDb.runCommand(
        {dropIndexes: kCollName, index: ["indexA", "indexB"], '$tenant': kTenant}));

    res = assert.commandWorked(testDb.runCommand({listIndexes: kCollName, '$tenant': kTenant}));
    assert.eq(1, res.cursor.firstBatch.length, tojson(res.cursor.firstBatch));
    assert(arrayEq([{key: {"_id": 1}, name: "_id_"}], getIndexesKeyAndName(res.cursor.firstBatch)),
           tojson(res.cursor.firstBatch));
}

// Test collMod
{
    // Create the index used for collMod
    let res = assert.commandWorked(testDb.runCommand({
        createIndexes: kCollName,
        indexes: [{key: {c: 1}, name: "indexC", expireAfterSeconds: 50}],
        '$tenant': kTenant
    }));
    assert.eq(2, res.numIndexesAfter, tojson(res));

    // Modifying the index without the tenantId should not work.
    res = testDb.runCommand({
        "collMod": kCollName,
        "index": {"keyPattern": {c: 1}, expireAfterSeconds: 100},
    });
    if (featureFlagRequireTenantId) {
        // When the feature flag is enabled, the server will assert that all requests contain a
        // tenantId.
        assert.commandFailedWithCode(res, 6972100);
    } else {
        assert.commandFailedWithCode(res, ErrorCodes.NamespaceNotFound);
    }

    // Modify the index with the tenantId
    res = assert.commandWorked(testDb.runCommand({
        "collMod": kCollName,
        "index": {"keyPattern": {c: 1}, expireAfterSeconds: 100},
        '$tenant': kTenant
    }));
    assert.eq(50, res.expireAfterSeconds_old, tojson(res));
    assert.eq(100, res.expireAfterSeconds_new, tojson(res));

    // Drop the index created
    assert.commandWorked(
        testDb.runCommand({dropIndexes: kCollName, index: ["indexC"], '$tenant': kTenant}));
}

// Test the applyOps command
{
    if (featureFlagRequireTenantId) {
        assert.commandWorked(testDb.runCommand({
            applyOps:
                [{"op": "i", "ns": testColl.getFullName(), "tid": kTenant, "o": {_id: 5, x: 17}}],
            '$tenant': kTenant
        }));
    } else {
        const ns = kTenant + '_' + testColl.getFullName();
        assert.commandWorked(testDb.runCommand(
            {applyOps: [{"op": "i", "ns": ns, "o": {_id: 5, x: 17}}], '$tenant': kTenant}));
    }

    // Check applyOp inserted the document.
    const findRes = assert.commandWorked(
        testDb.runCommand({find: kCollName, filter: {_id: 5}, '$tenant': kTenant}));
    assert.eq(1, findRes.cursor.firstBatch.length, tojson(findRes.cursor.firstBatch));
    assert.eq(17, findRes.cursor.firstBatch[0].x, tojson(findRes.cursor.firstBatch));
}

// Test the validate command.
{
    const validateRes =
        assert.commandWorked(testDb.runCommand({validate: kCollName, '$tenant': kTenant}));
    assert(validateRes.valid, tojson(validateRes));
}

// Test dbCheck command.
{ assert.commandWorked(testDb.runCommand({dbCheck: kCollName, '$tenant': kTenant})); }

// fail server-side javascript commands/stages, all unsupported in serverless
{
    // Create a number of collections and entries used to test agg stages
    const kCollA = "collA";
    assert.commandWorked(testDb.createCollection(kCollA, {'$tenant': kTenant}));

    const collADocs = [
        {_id: 0, start: "a", end: "b"},
        {_id: 1, start: "b", end: "c"},
        {_id: 2, start: "c", end: "d"}
    ];

    assert.commandWorked(
        testDb.runCommand({insert: kCollA, documents: collADocs, '$tenant': kTenant}));

    // $where expression
    assert.commandFailedWithCode(testDb.runCommand({
        find: kCollA,
        filter: {
            $where: function() {
                return true;
            }
        },
        '$tenant': kTenant
    }),
                                 6108304);

    // $function aggregate stage
    assert.commandFailedWithCode(testDb.runCommand({
        aggregate: kCollA,
        pipeline: [{
            $match: {
                $expr: {
                    $function: {
                        body: function() {
                            return true;
                        },
                        args: [],
                        lang: "js"
                    }
                }
            }
        }],
        cursor: {},
        '$tenant': kTenant
    }),
                                 31264);

    // $accumulator operator
    assert.commandFailedWithCode(testDb.runCommand({
        aggregate: kCollA,
        pipeline: [{
            $group: {
                _id: 1,
                value: {
                    $accumulator: {
                        init: function() {},
                        accumulateArgs: {$const: []},
                        accumulate: function(state, value) {},
                        merge: function(s1, s2) {},
                        lang: 'js',
                    }
                }
            }
        }],
        cursor: {},
        '$tenant': kTenant
    }),
                                 31264);

    // mapReduce command
    function mapFunc() {
        emit(this.key, this.value);
    }
    function reduceFunc(key, values) {
        return values.join('');
    }
    assert.commandFailedWithCode(testDb.runCommand({
        mapReduce: kCollA,
        map: mapFunc,
        reduce: reduceFunc,
        out: {inline: 1},
        '$tenant': kTenant
    }),
                                 31264);
}

// Test the fail command failpoint with $tenant.
{
    // We should not pass $tenant in the data field. Here it is passed twice.
    assert.commandFailedWithCode(adminDb.runCommand({
        configureFailPoint: "failCommand",
        mode: {times: 1},
        '$tenant': kTenant,
        data: {
            failCommands: ["find"],
            namespace: testDb.getName() + "." + kCollName,
            '$tenant': kTenant,
        }
    }),
                                 7302300);

    // We should not pass $tenant in the data field.
    assert.commandFailedWithCode(adminDb.runCommand({
        configureFailPoint: "failCommand",
        mode: {times: 1},
        data: {
            failCommands: ["find"],
            namespace: testDb.getName() + "." + kCollName,
            '$tenant': kTenant,
        }
    }),
                                 7302300);

    // enable the failCommand failpoint for kTenant on myDb.myColl for the find command.
    assert.commandWorked(adminDb.runCommand({
        configureFailPoint: "failCommand",
        mode: "alwaysOn",
        '$tenant': kTenant,
        data: {
            errorCode: ErrorCodes.InternalError,
            failCommands: ["find"],
            namespace: testDb.getName() + "." + kCollName,
        }
    }));

    // same tenant and same namespace should fail.
    assert.commandFailedWithCode(testDb.runCommand({find: kCollName, '$tenant': kTenant}),
                                 ErrorCodes.InternalError);

    // same tenant different namespace.
    assert.commandWorked(testDb.runCommand({find: "foo", '$tenant': kTenant}));

    // different tenant passed and same namespace.
    assert.commandWorked(testDb.runCommand({find: kCollName, '$tenant': kOtherTenant}));

    // different tenant passed and different namespace.
    assert.commandWorked(testDb.runCommand({find: "foo", '$tenant': kOtherTenant}));

    // disable the failCommand failpoint.
    assert.commandWorked(adminDb.runCommand({configureFailPoint: "failCommand", mode: "off"}));
    assert.commandWorked(testDb.runCommand({find: kCollName, '$tenant': kTenant}));
}

// Test invalid db name length which is more than 38 chars.
{
    const longDb = primary.getDB("ThisIsADbExceedsTheMaxLengthOfTenantDB38");
    assert.commandFailedWithCode(longDb.createCollection("testColl", {'$tenant': kTenant}),
                                 ErrorCodes.InvalidNamespace);
}

rst.stopSet();
