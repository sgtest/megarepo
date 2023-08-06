/*
 * This test checks that when mongod is started with UNIX sockets enabled or disabled,
 * that we are able to connect (or not connect) and run commands:
 * 1) There should be a default unix socket of /tmp/mongod-portnumber.sock
 * 2) If you specify a custom socket in the bind_ip param, that it shows up as
 *    /tmp/custom_socket.sock
 * 3) That bad socket paths, like paths longer than the maximum size of a sockaddr
 *    cause the server to exit with an error (socket names with whitespace are now supported)
 * 4) That the default unix socket doesn't get created if --nounixsocket is specified
 */
// @tags: [
//   requires_sharding,
// ]
// This test will only work on POSIX machines.
if (_isWindows()) {
    quit();
}

// Checking index consistency involves reconnecting to the mongos.
TestData.skipCheckingIndexesConsistentAcrossCluster = true;
TestData.skipCheckOrphans = true;
TestData.skipCheckRoutingTableConsistency = true;
TestData.skipCheckShardFilteringMetadata = true;

// Do not fail if this test leaves unterminated processes because testSockOptions
// is expected to throw before it calls stopMongod.
TestData.failIfUnterminatedProcesses = false;

// Do not check metadata consistency as it would require a connection to the mongos and this is
// bound to a specific socket for testing purposes.
TestData.skipCheckMetadataConsistency = true;

var doesLogMatchRegex = function(logArray, regex) {
    for (let i = (logArray.length - 1); i >= 0; i--) {
        var regexInLine = regex.exec(logArray[i]);
        if (regexInLine != null) {
            return true;
        }
    }
    return false;
};

var checkSocket = function(path) {
    assert.eq(fileExists(path), true);
    var conn = new Mongo(path);
    assert.commandWorked(conn.getDB("admin").runCommand("ping"),
                         `Expected ping command to succeed for ${path}`);
};

var testSockOptions = function(bindPath, expectSockPath, optDict, bindSep = ',', optMongos) {
    var optDict = optDict || {};
    if (bindPath) {
        optDict["bind_ip"] = `${MongoRunner.dataDir}/${bindPath}${bindSep}127.0.0.1`;
    }

    var conn, shards;
    if (optMongos) {
        shards = new ShardingTest({shards: 1, mongos: 1, other: {mongosOptions: optDict}});
        assert.neq(shards, null, "Expected cluster to start okay");
        conn = shards.s0;
    } else {
        conn = MongoRunner.runMongod(optDict);
    }

    assert.neq(conn, null, `Expected ${optMongos ? "mongos" : "mongod"} to start okay`);

    const defaultUNIXSocket = `/tmp/mongodb-${conn.port}.sock`;
    var checkPath = defaultUNIXSocket;
    if (expectSockPath) {
        checkPath = `${MongoRunner.dataDir}/${expectSockPath}`;
    }

    checkSocket(checkPath);

    // Test the naming of the unix socket
    var log = conn.adminCommand({getLog: 'global'});
    assert.commandWorked(log, "Expected getting the log to work");
    var ll = log.log;
    var re = new RegExp("anonymous unix socket");
    assert(doesLogMatchRegex(ll, re), "Log message did not contain 'anonymous unix socket'");

    if (optMongos) {
        shards.stop();
    } else {
        MongoRunner.stopMongod(conn);
    }

    assert.eq(fileExists(checkPath), false);
};

// Check that the default unix sockets work
testSockOptions();
testSockOptions(undefined, undefined, undefined, ',', true);

// Check that a custom unix socket path works
testSockOptions("testsock.socket", "testsock.socket");
testSockOptions("testsock.socket", "testsock.socket", undefined, ',', true);

// Check that a custom unix socket path works with spaces
testSockOptions("test sock.socket", "test sock.socket");
testSockOptions("test sock.socket", "test sock.socket", undefined, ',', true);

// Check that a custom unix socket path works with spaces before the comma and after
testSockOptions("testsock.socket ", "testsock.socket", undefined, ', ');
testSockOptions("testsock.socket ", "testsock.socket", undefined, ', ', true);

// Check that a bad UNIX path breaks
assert.throws(function() {
    var badname = "a".repeat(200) + ".socket";
    testSockOptions(badname, badname);
});

// Check that if UNIX sockets are disabled that we aren't able to connect over UNIX sockets
assert.throws(function() {
    testSockOptions(undefined, undefined, {nounixsocket: ""});
});

// Check the unixSocketPrefix option
var socketPrefix = `${MongoRunner.dataDir}/socketdir`;
mkdir(socketPrefix);
var port = allocatePort();
testSockOptions(
    undefined, `socketdir/mongodb-${port}.sock`, {unixSocketPrefix: socketPrefix, port: port});

port = allocatePort();
testSockOptions(undefined,
                `socketdir/mongodb-${port}.sock`,
                {unixSocketPrefix: socketPrefix, port: port},
                ',',
                true);