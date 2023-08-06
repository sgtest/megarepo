// Test for auth counters in serverStatus.

let expectedSuccessLogs = 0;
let expectedFailureLogs = 0;

function authnSuccessIncrementsServerStatusTotalAuthTime(mongodRunner) {
    jsTest.log(
        "============================ authnSuccessIncrementsServerStatusTotalAuthTime ============================");
    const admin = mongodRunner.getDB("admin");

    admin.auth('admin', 'pwd');

    const expected = assert.commandWorked(admin.runCommand({serverStatus: 1}))
                         .security.authentication.totalAuthenticationTimeMicros;

    assert.gte(expected, 0);

    admin.logout();

    admin.auth('admin', 'pwd');

    const nextExpected = assert.commandWorked(admin.runCommand({serverStatus: 1}))
                             .security.authentication.totalAuthenticationTimeMicros;

    assert.gt(nextExpected, expected);

    admin.logout();
}

function authnFailureIncrementsServerStatusTotalAuthTime(mongodRunner) {
    jsTest.log(
        "============================ authnFailureIncrementsServerStatusTotalAuthTime ============================");
    const admin = mongodRunner.getDB("admin");

    admin.auth('admin', 'pwd');

    // Count the number of authentications performed during setup
    const expected = assert.commandWorked(admin.runCommand({serverStatus: 1}))
                         .security.authentication.totalAuthenticationTimeMicros;

    assert.gte(expected, 0);

    admin.auth('admin', 'wrong');

    const nextExpected = assert.commandWorked(admin.runCommand({serverStatus: 1}))
                             .security.authentication.totalAuthenticationTimeMicros;

    assert.gt(nextExpected, expected);

    admin.logout();
}

// Test that a successful authentication is logged correctly
function authnSuccessLogsMetricsReportWithSuccessStatus(mongodRunner, connectionHealthLoggingOn) {
    jsTest.log(
        "============================ authnSuccessLogsMetricsReportWithSuccessStatus ============================");

    const admin = mongodRunner.getDB("admin");

    // If the test is operating inside the same session as other tests, we can find duplicate log
    // entries so we increment the total # of expected log entries instead of using a fixed count
    expectedSuccessLogs += 1;

    admin.auth('admin', 'pwd');
    if (connectionHealthLoggingOn) {
        assert.soon(
            () => checkLog.checkContainsWithCountJson(
                mongodRunner,
                5286306,
                {
                    "result": 0,
                    "metrics": {"conversation_duration": {"summary": [{"step": 1}, {"step": 2}]}}
                },
                expectedSuccessLogs,
                null,
                true),
            "Did not find expected 1 successful metric log entries");
    } else {
        assert.eq(checkLog.checkContainsWithCountJson(mongodRunner, 5286306, {}),
                  false,
                  "Expected not to find metric log entries");
    }

    admin.logout();
}

// Test that authentication failure is logged correctly
function authnFailureLogsMetricsReportWithFailedStatus(mongodRunner, connectionHealthLoggingOn) {
    jsTest.log(
        "============================ authnFailureLogsMetricsReportWithFailedStatus ============================");

    const admin = mongodRunner.getDB("admin");
    admin.auth('admin', 'wrong');
    expectedFailureLogs += 1;

    if (connectionHealthLoggingOn) {
        assert.soon(
            () => checkLog.checkContainsWithCountJson(
                mongodRunner,
                5286307,
                {"result": 18, "metrics": {"conversation_duration": {"summary": [{"step": 1}]}}},
                expectedFailureLogs,
                null,
                true),
            "Did not find expected 1 failure metric log entries");
    } else {
        assert.eq(checkLog.checkContainsWithCountJson(mongodRunner, 5286307, {}),
                  false,
                  "Expected not to find failure metric log entries");
    }

    admin.logout();
}

// Test that multiple successful authentications across the same client produce the correct log
// output and do not e.g. combine summaries or results
function multipleAuthnSuccessLogsMultipleCorrectReports(mongodRunner, connectionHealthLoggingOn) {
    jsTest.log(
        "============================ multipleAuthnSuccessLogsMultipleCorrectReports ============================");

    const admin = mongodRunner.getDB("admin");

    admin.auth('admin', 'pwd');
    admin.logout();
    admin.auth('admin', 'pwd');

    expectedSuccessLogs += 2;

    if (connectionHealthLoggingOn) {
        assert.soon(
            () => checkLog.checkContainsWithCountJson(
                mongodRunner,
                5286306,
                {
                    "result": 0,
                    "metrics": {"conversation_duration": {"summary": [{"step": 1}, {"step": 2}]}}
                },
                expectedSuccessLogs,
                null,
                true),
            "Did not find expected 2 successful metric log entries");
    } else {
        assert.eq(checkLog.checkContainsWithCountJson(mongodRunner, 5286306, {}),
                  false,
                  "Expected not to find successful metric log entries");
    }
}

// Test that multiple authentication failure is logged correctly
function multiAuthnFailureLogsMetricsReportWithFailedStatus(mongodRunner,
                                                            connectionHealthLoggingOn) {
    jsTest.log(
        "============================ multiAuthnFailureLogsMetricsReportWithFailedStatus ============================");

    const admin = mongodRunner.getDB("admin");
    admin.auth('admin', 'wrong');
    admin.auth('admin', 'wrong_2');

    expectedFailureLogs += 2;

    if (connectionHealthLoggingOn) {
        assert.soon(
            () => checkLog.checkContainsWithCountJson(
                mongodRunner,
                5286307,
                {"result": 18, "metrics": {"conversation_duration": {"summary": [{"step": 1}]}}},
                expectedFailureLogs,
                null,
                true),
            "Did not find expected 2 failure metric log entries");
    } else {
        assert.eq(checkLog.checkContainsWithCountJson(mongodRunner, 5286307, {}),
                  false,
                  "Expected not to find failure metric log entries");
    }

    admin.logout();
}

// Test that multiple mixed authentications across the same client produce the correct log output
// and do not e.g. combine summaries or results
function multipleAuthnMixedLogsMultipleCorrectReports(mongodRunner, connectionHealthLoggingOn) {
    jsTest.log(
        "============================ multipleAuthnMixedLogsMultipleCorrectReports ============================");

    const admin = mongodRunner.getDB("admin");

    admin.auth('admin', 'pwd');
    admin.logout();

    expectedSuccessLogs += 1;

    if (connectionHealthLoggingOn) {
        assert.soon(
            () => checkLog.checkContainsWithCountJson(
                mongodRunner,
                5286306,
                {
                    "result": 0,
                    "metrics": {"conversation_duration": {"summary": [{"step": 1}, {"step": 2}]}}
                },
                expectedSuccessLogs,
                null,
                true),
            "Did not find expected 1 successful metric log entries");
    } else {
        assert.eq(checkLog.checkContainsWithCountJson(mongodRunner, 5286306, {}),
                  false,
                  "Expected not to find successful metric log entries");
    }

    expectedFailureLogs += 1;

    admin.auth('admin', 'wrong');

    if (connectionHealthLoggingOn) {
        assert.soon(
            () => checkLog.checkContainsWithCountJson(
                mongodRunner,
                5286307,
                {"result": 18, "metrics": {"conversation_duration": {"summary": [{"step": 1}]}}},
                expectedFailureLogs,
                null,
                true),
            "Did not find expected 1 failure metric log entries");
    } else {
        assert.eq(checkLog.checkContainsWithCountJson(mongodRunner, 5286307, {}),
                  false,
                  "Expected not to find failure metric log entries");
    }
}

let runTest = (connectionHealthLoggingOn) => {
    const mongod = MongoRunner.runMongod(
        {setParameter: {enableDetailedConnectionHealthMetricLogLines: connectionHealthLoggingOn}});

    try {
        mongod.getDB("admin").createUser(
            {user: 'admin', pwd: 'pwd', roles: ['root'], mechanisms: ['SCRAM-SHA-256']});

        authnSuccessLogsMetricsReportWithSuccessStatus(mongod, connectionHealthLoggingOn);
        authnFailureLogsMetricsReportWithFailedStatus(mongod, connectionHealthLoggingOn);
        multiAuthnFailureLogsMetricsReportWithFailedStatus(mongod, connectionHealthLoggingOn);
        multipleAuthnSuccessLogsMultipleCorrectReports(mongod, connectionHealthLoggingOn);
        multipleAuthnMixedLogsMultipleCorrectReports(mongod, connectionHealthLoggingOn);

        authnSuccessIncrementsServerStatusTotalAuthTime(mongod, connectionHealthLoggingOn);
        authnFailureIncrementsServerStatusTotalAuthTime(mongod, connectionHealthLoggingOn);

    } finally {
        MongoRunner.stopMongod(mongod);
    }
};

// Parameterized on enabling/disabling connection health logging.
runTest(true);
runTest(false);