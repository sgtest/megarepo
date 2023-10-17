// Test parsing expectPrefix out of an unsigned security token, and enforcing that once a connection
// is marked as being from atlas proxy it may never unset that property
// @tags: [featureFlagSecurityToken]

const tenantID = ObjectId();
const kVTSKey = 'secret';
const kDbName = 'myDb';
const kCollName = 'myColl';

const opts = {
    auth: '',
    setParameter: {
        multitenancySupport: true,
        testOnlyValidatedTenancyScopeKey: kVTSKey,
    },
};

const rst = new ReplSetTest({nodes: 2, nodeOptions: opts});
rst.startSet({keyFile: 'jstests/libs/key1'});
rst.initiate();

let conn = rst.getPrimary();
const admin = conn.getDB('admin');

// Must be authenticated as a user with read/write privileges on non-normal collections, since
// we are accessing system.users for another tenant.
assert.commandWorked(admin.runCommand({createUser: 'admin', pwd: 'pwd', roles: ['__system']}));
assert(admin.auth('admin', 'pwd'));
// Make a less-privileged base user.
assert.commandWorked(
    admin.runCommand({createUser: 'baseuser', pwd: 'pwd', roles: ['readWriteAnyDatabase']}));

const testDb = conn.getDB("myDb");
conn._setSecurityToken(_createTenantToken({tenant: tenantID, expectPrefix: true}));

assert.commandWorked(testDb.runCommand({insert: kCollName, documents: [{myDoc: 0}]}));

// The same count command with `$tenant` fails as we do not allow both security token and $tenant.
assert.commandFailedWithCode(
    testDb.runCommand({count: kCollName, query: {myDoc: 0}, '$tenant': tenantID}),
    6545800);  // "Cannot pass $tenant id if also passing securityToken"

// test this second token will throw an assert because we can't change the token once it's set.
conn._setSecurityToken(_createTenantToken({tenant: tenantID, expectPrefix: false}));
assert.commandFailedWithCode(testDb.runCommand({count: kCollName, query: {myDoc: 0}}),
                             8154400 /*conn protocol can only change once*/);

// test this third token will throw an assert because we can't change the token once it's set.
conn._setSecurityToken(_createTenantToken({tenant: tenantID}));
assert.commandFailedWithCode(testDb.runCommand({count: kCollName, query: {myDoc: 0}}),
                             8154400 /*conn protocol can only change once*/);

// Setting the same unsigned token works because same token is the same as using the original one.
// Nothing has changed.
conn._setSecurityToken(_createTenantToken({tenant: tenantID, expectPrefix: true}));
assert.commandWorked(testDb.runCommand({count: kCollName, query: {myDoc: 0}}));

// no tenant given so we shouldn't see the doc.
conn._setSecurityToken(undefined);
assert.commandFailedWithCode(testDb.runCommand({count: kCollName, query: {myDoc: 0}}),
                             8423388 /*TenantId must be set*/);

// $tenant still works.
assert.commandWorked(testDb.runCommand({count: kCollName, query: {myDoc: 0}, '$tenant': tenantID}));

rst.stopSet();
