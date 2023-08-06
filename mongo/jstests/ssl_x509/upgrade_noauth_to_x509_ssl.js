/**
 * This test checks the upgrade path from noauth/nossl to x509/requireSSL.
 *
 * NOTE: This test uses ssl communication and therefore cannot test modes that
 * do not allow ssl. The first step in the full upgrade process is to restart
 * the each node into the "transitionToX509AllowSSL" state and is tested in
 * the sslSpecial directory.
 *
 * This test requires users and data to persist across a restart.
 * @tags: [requires_persistence]
 */

import {allowSSL, requireSSL} from "jstests/ssl/libs/ssl_helpers.js";

TestData.disableImplicitSessions = true;

var dbName = 'upgradeToX509';

var transitionToX509AllowSSL =
    Object.merge(allowSSL, {transitionToAuth: '', clusterAuthMode: 'x509'});

// Undefine the flags we're replacing, otherwise upgradeSet will keep old values.
var x509RequireSSL =
    Object.merge(requireSSL, {transitionToAuth: undefined, clusterAuthMode: 'x509'});

var rst = new ReplSetTest({name: 'noauthSet', nodes: 3, nodeOptions: transitionToX509AllowSSL});
rst.startSet();
rst.initiate();

var rstConn1 = rst.getPrimary();
var testDB = rstConn1.getDB(dbName);

// Create a user to login when auth is enabled later
assert.commandWorked(rstConn1.adminCommand(
    {createUser: 'root', pwd: 'root', roles: ['root'], writeConcern: {w: 3}}));

assert.commandWorked(testDB.a.insert({a: 1, str: 'TESTTESTTEST'}));
assert.eq(1, testDB.a.count(), 'Error interacting with replSet');

print('=== UPGRADE transition to x509/allowSSL -> transition to x509/preferSSL ===');
rst.nodes.forEach(function(node) {
    assert.commandWorked(node.adminCommand({setParameter: 1, sslMode: "preferSSL"}));
});
rst.awaitSecondaryNodes();
testDB = rst.getPrimary().getDB(dbName);
assert.commandWorked(testDB.a.insert({a: 1, str: 'TESTTESTTEST'}));
assert.eq(2, testDB.a.count(), 'Error interacting with replSet');

print('=== UPGRADE transition to x509/preferSSL -> x509/requireSSL ===');

// Pre-logout so that upgradeSet() can authenticate for itself.
rst.nodes.forEach((node) => node.getDB('admin').logout());
rst.upgradeSet(x509RequireSSL, 'root', 'root');

// Reauth and commit one last write.
const finalPrimary = rst.getPrimary();
assert(finalPrimary.getDB('admin').auth('root', 'root'));
testDB = finalPrimary.getDB(dbName);
assert.commandWorked(testDB.a.insert({a: 1, str: 'TESTTESTTEST'}));
assert.eq(3, testDB.a.count(), 'Error interacting with replSet');

rst.stopSet();
