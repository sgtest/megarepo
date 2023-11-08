/**
 * Tests for the ingress handshake metrics.
 *
 * @tags: [requires_fcv_70]
 */
import {getFailPointName} from "jstests/libs/fail_point_util.js";
import {ingressHandshakeMetricsTest} from "jstests/libs/ingress_handshake_metrics_helpers.js";

let runTest = (connectionHealthLoggingOn) => {
    let rootCreds = {user: 'root', pwd: 'root'};
    let conn = MongoRunner.runMongod({auth: ''});

    jsTestLog("Setting up users and test data.");
    let runMetricsTest = ingressHandshakeMetricsTest(conn, {
        connectionHealthLoggingOn: connectionHealthLoggingOn,
        preAuthDelayMillis: 50,
        postAuthDelayMillis: 100,
        helloProcessingDelayMillis: 50,
        helloResponseDelayMillis: 100,
        rootCredentials: rootCreds,
        helloFailPointName: getFailPointName("shardWaitInHello", conn.getMaxWireVersion()),
    });

    jsTestLog("Connecting to mongod and running the test.");
    runMetricsTest();

    MongoRunner.stopMongod(conn, null, rootCreds);
};

// Parameterized on enabling/disabling connection health logging.
runTest(true);
runTest(false);
