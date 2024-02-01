// Ensure that TLS version alerts are correctly propagated

import {determineSSLProvider, sslProviderSupportsTLS1_1} from "jstests/ssl/libs/ssl_helpers.js";

const clientOptions = [
    "--tls",
    "--tlsCertificateKeyFile",
    "jstests/libs/client.pem",
    "--tlsCAFile",
    "jstests/libs/ca.pem",
    "--eval",
    ";"
];

function runTest(serverDisabledProtos, clientDisabledProtos) {
    const implementation = determineSSLProvider();
    let expectedRegex;
    if (implementation === "openssl") {
        expectedRegex =
            /Error: couldn't connect to server .*:[0-9]*, connection attempt failed: SocketException: .*tlsv1 alert protocol version/;

        // OpenSSL does not send alerts and TLS 1.3 is too difficult to identify as incompatible
        // because it shows up in a TLS extension.
        if (!sslProviderSupportsTLS1_1()) {
            expectedRegex =
                /Error: couldn't connect to server .*:[0-9]*, connection attempt failed: SocketException: .*stream truncated/;
        }

    } else if (implementation === "windows") {
        expectedRegex =
            /Error: couldn't connect to server .*:[0-9]*, connection attempt failed: SocketException: .*The function requested is not supported/;
    } else if (implementation === "apple") {
        expectedRegex =
            /Error: couldn't connect to server .*:[0-9]*, connection attempt failed: SocketException: .*Secure.Transport: bad protocol version/;
    } else {
        throw Error("Unrecognized TLS implementation!");
    }

    var md = MongoRunner.runMongod({
        tlsMode: "requireTLS",
        tlsCAFile: "jstests/libs/ca.pem",
        tlsCertificateKeyFile: "jstests/libs/server.pem",
        tlsDisabledProtocols: serverDisabledProtos,
    });

    let mongoOutput;

    assert.soon(function() {
        clearRawMongoProgramOutput();
        runMongoProgram("mongo",
                        "--port",
                        md.port,
                        ...clientOptions,
                        "--tlsDisabledProtocols",
                        clientDisabledProtos);
        mongoOutput = rawMongoProgramOutput();
        return mongoOutput.match(expectedRegex);
    }, "Mongo shell output was as follows:\n" + mongoOutput + "\n************", 60 * 1000);

    MongoRunner.stopMongod(md);
}

// Client receives and reports a protocol version alert if it advertises a protocol older than
// the server's oldest supported protocol
if (!sslProviderSupportsTLS1_1()) {
    // On platforms that disable TLS 1.1, assume they have TLS 1.3 for this test.
    runTest("TLS1_2", "TLS1_3");
} else {
    runTest("TLS1_0", "TLS1_1,TLS1_2");
}
