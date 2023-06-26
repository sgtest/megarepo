// Test that the collection catalog is restored correctly after a restart in a multitenant
// environment.

(function() {
"use strict";

load('jstests/aggregation/extras/utils.js');  // For arrayEq()
load("jstests/libs/feature_flag_util.js");    // for isEnabled

const rst =
    new ReplSetTest({nodes: 3, nodeOptions: {auth: '', setParameter: {multitenancySupport: true}}});
rst.startSet({keyFile: 'jstests/libs/key1'});
rst.initiate();

let primary = rst.getPrimary();
let adminDb = primary.getDB('admin');

// Must be authenticated as a user with ActionType::useTenant in order to use $tenant.
assert.commandWorked(adminDb.runCommand({createUser: 'admin', pwd: 'pwd', roles: ['root']}));
assert(adminDb.auth('admin', 'pwd'));

const featureFlagRequireTenantId = FeatureFlagUtil.isEnabled(adminDb, "RequireTenantID");

{
    const kTenant = ObjectId();
    let testDb = primary.getDB('myDb0');

    // Create a collection by inserting a document to it.
    assert.commandWorked(testDb.runCommand(
        {insert: 'myColl0', documents: [{_id: 0, a: 1, b: 1}], '$tenant': kTenant}));

    // Run findAndModify on the document.
    let fad = assert.commandWorked(testDb.runCommand(
        {findAndModify: "myColl0", query: {a: 1}, update: {$inc: {a: 10}}, '$tenant': kTenant}));
    assert.eq({_id: 0, a: 1, b: 1}, fad.value, tojson(fad));

    // Create a view on the collection.
    assert.commandWorked(testDb.runCommand(
        {"create": "view1", "viewOn": "myColl0", pipeline: [], '$tenant': kTenant}));

    // Stop the rs and restart it.
    rst.stopSet(null /* signal */, true /* forRestart */, {noCleanData: true});
    rst.startSet({restart: true});
    primary = rst.getPrimary();

    adminDb = primary.getDB('admin');
    assert(adminDb.auth('admin', 'pwd'));
    testDb = primary.getDB('myDb0');

    // Assert we see 3 collections in the tenant's db 'myDb0' - the original collection we
    // created, the view on it, and the system.views collection.
    const colls = assert.commandWorked(
        testDb.runCommand({listCollections: 1, nameOnly: true, '$tenant': kTenant}));
    assert.eq(3, colls.cursor.firstBatch.length, tojson(colls.cursor.firstBatch));
    const expectedColls = [
        {"name": "myColl0", "type": "collection"},
        {"name": "system.views", "type": "collection"},
        {"name": "view1", "type": "view"}
    ];
    assert(arrayEq(expectedColls, colls.cursor.firstBatch), tojson(colls.cursor.firstBatch));

    // Assert we can still run findAndModify on the doc.
    fad = assert.commandWorked(testDb.runCommand(
        {findAndModify: "myColl0", query: {a: 11}, update: {$inc: {a: 10}}, '$tenant': kTenant}));
    assert.eq({_id: 0, a: 11, b: 1}, fad.value, tojson(fad));

    const findAndModPrefixed =
        primary.getDB(kTenant + '_myDb0')
            .runCommand({findAndModify: "myColl0", query: {b: 1}, update: {$inc: {b: 10}}});
    // TOOD SERVER-74284: unwrap and keep only the (!featureFlagRequireTenantId) case
    if (!featureFlagRequireTenantId) {
        // Check that we do find the doc when the tenantId was passed as a prefix, only if the
        // feature flag is not enabled. In this case, the server still accepts prefixed names,
        // and will parse the tenant from the db name.
        assert.commandWorked(findAndModPrefixed);
        assert.eq({_id: 0, a: 21, b: 1}, findAndModPrefixed.value, tojson(findAndModPrefixed));
    } else {
        // assert.commandFailed(findAndModPrefixed);
        // TODO SERVER-73113 Uncomment out the check above, and remove the check below.
        assert.eq(null, findAndModPrefixed.value, tojson(findAndModPrefixed));
    }
}

rst.stopSet();
})();
