//
// Basic tests for enableSharding command.
//

var st = new ShardingTest({shards: 2});

jsTest.log('enableSharding can run only against the admin database');
{
    assert.commandFailedWithCode(st.s0.getDB('test').runCommand({enableSharding: 'db'}),
                                 ErrorCodes.Unauthorized);
}

jsTest.log('Cannot shard system databases except for the config db');
{
    assert.commandWorked(st.s0.adminCommand({enableSharding: 'config'}));
    assert.commandFailed(st.s0.adminCommand({enableSharding: 'local'}));
    assert.commandFailed(st.s0.adminCommand({enableSharding: 'admin'}));
}

jsTest.log('Cannot shard db with the name that just differ on case');
{
    assert.commandWorked(st.s0.adminCommand({enableSharding: 'db'}));
    assert.eq(1, st.config.databases.countDocuments({_id: 'db'}));
    assert.commandFailedWithCode(st.s0.adminCommand({enableSharding: 'DB'}),
                                 ErrorCodes.DatabaseDifferCase);
}

jsTest.log('Cannot shard invalid db name');
{
    assert.commandFailed(st.s0.adminCommand({enableSharding: 'a.b'}));
    assert.commandFailed(st.s0.adminCommand({enableSharding: ''}));
}

jsTest.log('Attempting to shard already sharded database returns success');
{
    assert.commandWorked(st.s0.adminCommand({enableSharding: 'db'}));
    assert.eq(1, st.config.databases.countDocuments({_id: 'db'}));
}

jsTest.log('Implicit db creation when writing to an unsharded collection');
{
    assert.commandWorked(st.s0.getDB('unsharded').foo.insert({aKey: "aValue"}));
    assert.eq(1, st.config.databases.countDocuments({_id: 'unsharded'}));
}

jsTest.log('Sharding a collection before enableSharding works');
{ assert.commandWorked(st.s.adminCommand({shardCollection: 'testdb.testcoll', key: {_id: 1}})); }

jsTest.log('Cannot enable sharding on a database using a wrong shard name');
{
    assert.commandFailed(st.s0.adminCommand(
        {enableSharding: 'db2', primaryShard: st.shard1.shardName + '_unenxisting_name_postfix'}));
}

jsTest.log('Enabling sharding on a database with a valid shard name must work');
{
    assert.commandWorked(
        st.s0.adminCommand({enableSharding: 'db_on_shard0', primaryShard: st.shard0.shardName}));
    assert.commandWorked(
        st.s0.adminCommand({enableSharding: 'db_on_shard1', primaryShard: st.shard1.shardName}));
    assert.eq(st.s0.getDB('config').databases.findOne({_id: 'db_on_shard0'}).primary,
              st.shard0.shardName);
    assert.eq(st.s0.getDB('config').databases.findOne({_id: 'db_on_shard1'}).primary,
              st.shard1.shardName);
}

jsTest.log(
    'Enable sharding on a database already created with the correct primary shard name must work');
{
    assert.commandWorked(
        st.s0.adminCommand({enableSharding: 'db_on_shard0', primaryShard: st.shard0.shardName}));
    assert.commandWorked(
        st.s0.adminCommand({enableSharding: 'db_on_shard1', primaryShard: st.shard1.shardName}));
}

jsTest.log(
    'Cannot enable sharding of a database already created with a different primary shard name');
{
    assert.commandFailedWithCode(
        st.s0.adminCommand({enableSharding: 'db_on_shard0', primaryShard: st.shard1.shardName}),
        ErrorCodes.NamespaceExists);
    assert.commandFailedWithCode(
        st.s0.adminCommand({enableSharding: 'db_on_shard1', primaryShard: st.shard0.shardName}),
        ErrorCodes.NamespaceExists);
}

st.stop();