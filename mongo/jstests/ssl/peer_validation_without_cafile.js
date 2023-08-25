(function() {
"use strict";

if (getBuildInfo().buildEnvironment.target_os != "linux") {
    // these tests are specifically for linux
    return;
}

function testRS(opts, succeed) {
    const origSkipCheck = TestData.skipCheckDBHashes;
    const rsOpts = {
        // Use localhost so that SAN matches.
        useHostName: false,
        nodes: {node0: opts, node1: opts},
    };
    const rs = new ReplSetTest(rsOpts);
    rs.startSet({
        env: {
            SSL_CERT_FILE: 'jstests/libs/ca.pem',
        },
    });
    if (succeed) {
        rs.initiate();
        assert.commandWorked(rs.getPrimary().getDB('admin').runCommand({hello: 1}));
    } else {
        // By default, rs.initiate takes a very long time to timeout. We should shorten this
        // period, because we expect it to fail. ReplSetTest has both a static and local copy
        // of kDefaultTimeOutMS, so we must override both.
        const oldTimeout = ReplSetTest.kDefaultTimeoutMS;
        const shortTimeout = 2 * 60 * 1000;
        ReplSetTest.kDefaultTimeoutMS = shortTimeout;
        rs.kDefaultTimeoutMS = shortTimeout;
        // The rs.initiate will fail in an assert.soon, which would ordinarily trigger the hang
        // analyzer.  We don't want that to happen, so we disable it here.
        MongoRunner.runHangAnalyzer.disable();
        try {
            assert.throws(function() {
                rs.initiate();
            });
        } finally {
            ReplSetTest.kDefaultTimeoutMS = oldTimeout;
            MongoRunner.runHangAnalyzer.enable();
        }
        TestData.skipCheckDBHashes = true;
    }
    rs.stopSet();
    TestData.skipCheckDBHashes = origSkipCheck;
}

// ca.pem signed client.pem and server.pem
// trusted-ca.pem signed trusted-client.pem and trusted-server.pem
const options_explicit_systemca = {
    tlsMode: 'requireTLS',
    tlsCAFile: 'jstests/libs/ca.pem',
    tlsCertificateKeyFile: 'jstests/libs/server.pem',

};

testRS(options_explicit_systemca, true);

const options_implicit_systemca = {
    tlsMode: 'requireTLS',
    tlsCertificateKeyFile: 'jstests/libs/server.pem',
};

testRS(options_implicit_systemca, true);

const options_explicit_systemca_nomatch = {
    tlsMode: 'requireTLS',
    tlsCAFile: 'jstests/libs/ca.pem',
    tlsCertificateKeyFile: 'jstests/libs/trusted-server.pem',
};

testRS(options_explicit_systemca_nomatch, false);

const options_implicit_systemca_nomatch = {
    tlsMode: 'requireTLS',
    tlsCertificateKeyFile: 'jstests/libs/trusted-server.pem',
};

testRS(options_implicit_systemca_nomatch, false);
}());
