// Test creation of local users and roles for multitenancy.

function setup(conn) {
    const admin = conn.getDB('admin');
    assert.commandWorked(
        admin.runCommand({createUser: 'admin', pwd: 'admin', roles: ['__system']}));
    assert(admin.auth('admin', 'admin'));
}

function runTests(conn, tenant, multitenancySupport) {
    if (tenant == null && multitenancySupport) {
        // When multitenancySupport is enabled, requests are expected to contain a tenant, so do not
        // run these tests when tenant is null and multitenancySupport is enabled.
        return;
    }

    const expectSuccess = (tenant === null) || (multitenancySupport && TestData.enableTestCommands);
    jsTest.log("Runing test: " + tojson({tenant: tenant, multi: multitenancySupport}));

    function checkSuccess(result) {
        if (expectSuccess) {
            return assert.commandWorked(result);
        } else {
            assert.commandFailedWithCode(result, ErrorCodes.InvalidOptions);
            return false;
        }
    }

    const admin = conn.getDB('admin');
    const test = conn.getDB('test');
    const cmdSuffix = (tenant === null) ? {} : {"$tenant": tenant};
    function runCmd(cmd) {
        const cmdToRun = Object.assign({}, cmd, cmdSuffix);
        return test.runCommand(cmdToRun);
    }

    function runFindCmdWithTenant(db, collection, filter) {
        let result = [];
        let cmdRes = assert.commandWorked(
            db.runCommand({find: collection, filter: filter, batchSize: 0, "$tenant": tenant}));
        result = result.concat(cmdRes.cursor.firstBatch);

        let cursorId = cmdRes.cursor.id;
        while (cursorId != 0) {
            cmdRes = assert.commandWorked(db.runCommand(
                {getMore: cursorId, collection: collection, batchSize: 1, "$tenant": tenant}));
            result = result.concat(cmdRes.cursor.nextBatch);
            cursorId = cmdRes.cursor.id;
        }

        return result;
    }

    function findTenantRoles(filter) {
        return runFindCmdWithTenant(admin, "system.roles", filter);
    }

    function findTenantUsers(filter) {
        return runFindCmdWithTenant(admin, "system.users", filter);
    }

    function validateCounts(expectUsers, expectRoles) {
        const filter = {db: 'test'};
        const admin = conn.getDB('admin');

        if (!expectSuccess) {
            expectUsers = expectRoles = 0;
        }

        // usersInfo/rolesInfo commands return expected data.
        const usersInfo = assert.commandWorked(runCmd({usersInfo: 1})).users;
        const rolesInfo = assert.commandWorked(runCmd({rolesInfo: 1, showPrivileges: true})).roles;
        assert.eq(usersInfo.length, expectUsers, tojson(usersInfo));
        assert.eq(rolesInfo.length, expectRoles, tojson(rolesInfo));

        if (tenant) {
            // Look for users/roles in tenant specific collections directly.
            const tenantUsers = findTenantUsers(filter);
            const tenantRoles = findTenantRoles(filter);

            assert.eq(tenantUsers.length, expectUsers, tojson(tenantUsers));
            assert.eq(tenantRoles.length, expectRoles, tojson(tenantRoles));

            // Found users/roles in tenant, don't look for them in base collections.
            expectUsers = expectRoles = 0;
        }

        // Check base system collections, generally should be empty, unless we're in no-tenant mode.
        const systemUsers = admin.system.users.find(filter).toArray();
        const systemRoles = admin.system.roles.find(filter).toArray();
        assert.eq(systemUsers.length, expectUsers, tojson(systemUsers));
        assert.eq(systemRoles.length, expectRoles, tojson(systemRoles));
    }

    // createUser/createRole
    checkSuccess(runCmd({createUser: 'user1', 'pwd': 'pwd', roles: []}));
    checkSuccess(runCmd({createRole: 'role1', roles: [], privileges: []}));
    checkSuccess(runCmd({createRole: 'role2', roles: ['role1'], privileges: []}));
    checkSuccess(
        runCmd({createRole: 'role3', roles: [{db: 'test', role: 'role1'}], privileges: []}));
    checkSuccess(runCmd({createUser: 'user2', 'pwd': 'pwd', roles: ['role2', 'role3']}));

    const rwMyColl_privs = [{
        resource: {db: 'test', collection: 'myColl'},
        actions: ['find', 'insert', 'remove', 'update']
    }];
    const myCollUser_roles = [{role: 'rwMyColl', db: 'test'}];
    checkSuccess(runCmd({createRole: 'rwMyColl', roles: [], privileges: rwMyColl_privs}));
    checkSuccess(runCmd({createUser: 'myCollUser', pwd: 'pwd', roles: myCollUser_roles}));
    validateCounts(3, 4);

    if (tenant && expectSuccess) {
        const myCollUser = findTenantUsers({_id: 'test.myCollUser'})[0];
        assert.eq(tojson(myCollUser.roles), tojson(myCollUser_roles), tojson(myCollUser));
        const rwMyColl = findTenantRoles({_id: 'test.rwMyColl'})[0];
        assert.eq(tojson(rwMyColl.privileges), tojson(rwMyColl_privs), tojson(rwMyColl));
        const role2 = findTenantRoles({_id: 'test.role2'})[0];
        assert.eq(tojson(role2.roles), tojson([{role: 'role1', db: 'test'}]), tojson(role2));
        const role3 = findTenantRoles({_id: 'test.role3'})[0];
        assert.eq(tojson(role3.roles), tojson([{role: 'role1', db: 'test'}]), tojson(role3));
    }

    // grant/revoke privileges
    const rwMyColl_addPrivs =
        [{resource: {db: 'test', collection: 'otherColl'}, actions: ['find']}];
    checkSuccess(runCmd({grantPrivilegesToRole: 'rwMyColl', privileges: rwMyColl_addPrivs}));
    checkSuccess(runCmd({
        revokePrivilegesFromRole: 'rwMyColl',
        privileges: [{resource: {db: 'test', collection: 'myColl'}, actions: ['find']}]
    }));
    validateCounts(3, 4);

    if (tenant && expectSuccess) {
        const rwMyColl_expectPrivs = [
            {resource: {db: 'test', collection: 'myColl'}, actions: ['insert', 'remove', 'update']},
            {resource: {db: 'test', collection: 'otherColl'}, actions: ['find']}
        ];
        const rwMyColl = findTenantRoles({_id: 'test.rwMyColl'})[0];
        assert.eq(tojson(rwMyColl.privileges), tojson(rwMyColl_expectPrivs), tojson(rwMyColl));
    }

    // Grant/Revoke Roles to/fromfrom User/Role
    checkSuccess(runCmd({grantRolesToUser: 'user1', roles: ['role1']}));
    checkSuccess(runCmd({revokeRolesFromUser: 'user2', roles: ['role2']}));
    checkSuccess(runCmd({grantRolesToRole: 'role1', roles: ['rwMyColl']}));
    checkSuccess(runCmd({revokeRolesFromRole: 'role3', roles: ['role1']}));
    validateCounts(3, 4);

    if (tenant && expectSuccess) {
        const user1 = findTenantUsers({_id: 'test.user1'})[0];
        assert.eq(tojson(user1.roles), tojson([{role: 'role1', db: 'test'}]), tojson(user1));
        const user2 = findTenantUsers({_id: 'test.user2'})[0];
        assert.eq(tojson(user2.roles), tojson([{role: 'role3', db: 'test'}]), tojson(user2));

        const role1 = findTenantRoles({_id: 'test.role1'})[0];
        assert.eq(tojson(role1.roles), tojson([{role: 'rwMyColl', db: 'test'}]), tojson(role1));
        const role3 = findTenantRoles({_id: 'test.role3'})[0];
        assert.eq(tojson(role3.roles), tojson([]), tojson(role3));
    }

    // updateUser/updateRole
    checkSuccess(runCmd({updateUser: 'user1', roles: ['role2']}));
    checkSuccess(runCmd({updateRole: 'role2', roles: ['rwMyColl']}));
    validateCounts(3, 4);

    if (tenant && expectSuccess) {
        const user1 = findTenantUsers({_id: 'test.user1'})[0];
        assert.eq(tojson(user1.roles), tojson([{role: 'role2', db: 'test'}]), tojson(user1));
        const role2 = findTenantRoles({_id: 'test.role2'})[0];
        assert.eq(tojson(role2.roles), tojson([{role: 'rwMyColl', db: 'test'}]), tojson(role2));
    }

    // dropUser/dropRole
    checkSuccess(runCmd({dropRole: 'role2'}));
    checkSuccess(runCmd({dropUser: 'myCollUser'}));
    validateCounts(2, 3);

    if (tenant && expectSuccess) {
        // role2 should have been revoked from user1 during drop,.
        const user1 = findTenantUsers({_id: 'test.user1'})[0];
        assert.eq(tojson(user1.roles), tojson([]), tojson(user1));
        assert.eq(0, findTenantUsers({_id: 'test.myCollUser'}).length);
        assert.eq(0, findTenantRoles({_id: 'test.role2'}).length);
    }

    // Cleanup
    checkSuccess(runCmd({dropAllUsersFromDatabase: 1}));
    checkSuccess(runCmd({dropAllRolesFromDatabase: 1}));
    validateCounts(0, 0);
}

// This isn't relevant to this test, but requires enableTestCommands, which we want to frob.
TestData.roleGraphInvalidationIsFatal = false;

function spanOptions(cb) {
    [true].forEach(function(enableTestCommands) {
        TestData.enableTestCommands = enableTestCommands;
        [true].forEach(function(multitenancySupport) {
            jsTest.log(
                {enableTestCommands: enableTestCommands, multitenancySupport: multitenancySupport});
            cb({multitenancySupport: multitenancySupport});
        });
    });
}

spanOptions(function(setParams) {
    const standalone = MongoRunner.runMongod({auth: "", setParameter: setParams});
    jsTest.log('Standalone started');
    setup(standalone);
    runTests(standalone, null, setParams.multitenancySupport);
    runTests(standalone, ObjectId(), setParams.multitenancySupport);
    MongoRunner.stopMongod(standalone);
});