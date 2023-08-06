/**
 * Tests that mongos reacts properly to a client disconnecting while the logical time is being
 * signed as a part of appending fields to a command response.
 *
 * @tags: [
 *   requires_sharding,
 * ]
 */
const st = new ShardingTest({mongos: 1, shards: 0, keyFile: "jstests/libs/key1"});

assert.commandFailedWithCode(st.s.adminCommand({
    configureFailPoint: "throwClientDisconnectInSignLogicalTimeForExternalClients",
    mode: {times: 1}
}),
                             ErrorCodes.ClientDisconnect);

st.stop();