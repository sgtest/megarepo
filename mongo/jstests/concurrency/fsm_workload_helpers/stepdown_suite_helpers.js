// Commands that are not fully retryable will be refused by
// jstests/libs/override_methods/network_error_and_txn_override.js unless opting out of automatic
// retries. Use this helper to temporarily disable automatic retries for your command and handle the
// consequences of failure some other way.
export function withSkipRetryOnNetworkError(fn) {
    const previousSkipRetryOnNetworkError = TestData.skipRetryOnNetworkError;
    TestData.skipRetryOnNetworkError = true;

    let res = undefined;
    try {
        res = fn();
    } finally {
        TestData.skipRetryOnNetworkError = previousSkipRetryOnNetworkError;
    }

    return res;
}

export function runWithManualRetriesIfInStepdownSuite(fn) {
    if (TestData.runningWithShardStepdowns) {
        var result = undefined;
        assert.soonNoExcept(() => {
            result = withSkipRetryOnNetworkError(fn);
            return true;
        });
        return result;
    } else {
        return fn();
    }
}

// The getMore command is not retryable, so it is not allowed to be run in suites with
// stepdown/kill/terminate. This will find only one batch to avoid calling getMore; ensure that
// the batchSize is large enough for the number of documents expected to be returned.
export function findFirstBatch(db, collName, filter, batchSize) {
    return assert.commandWorked(db.runCommand({find: collName, filter, batchSize}))
        .cursor.firstBatch;
}
