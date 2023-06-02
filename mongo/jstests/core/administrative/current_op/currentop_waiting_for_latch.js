/**
 * Tests that a backtrace will appear in the $currentOp output if the backtrace option is
 * set to true and there is a latch timeout.
 *
 * The test runs commands that are not allowed with security token: whatsmyuri.
 * @tags: [
 *   not_allowed_with_security_token,
 *   assumes_read_concern_unchanged,
 *   assumes_read_preference_unchanged,
 *   no_selinux,
 *   multiversion_incompatible,
 *   requires_latch_analyzer
 * ]
 */
(function() {
"use strict";

const adminDB = db.getSiblingDB("admin");

// This test causes an extra thread with a self-contended lock to be created so that currentOp can
// observe its DiagnosticInfo. This mutex is test-only, and is at a lower level than the
// SessionCatalog's mutex, so we pass 'idleSessions: false' to avoid scanning idle sessions during
// currentOp and hitting a latch violation.
const getCurrentOp = function() {
    jsTestLog("Getting $currentOp");
    let result = adminDB
                     .aggregate(
                         [
                             {
                                 $currentOp: {
                                     allUsers: true,
                                     idleConnections: true,
                                     idleSessions: false,
                                     localOps: true,
                                     backtrace: true
                                 }
                             },
                         ],
                         {readConcern: {level: "local"}})
                     .toArray();
    assert(result);
    return result;
};

const blockedOpClients = {
    "DiagnosticCaptureTestLatch": {"seen": false},
    "DiagnosticCaptureTestInterruptible": {"seen": false},
};

const getClientName = function() {
    let myUri = adminDB.runCommand({whatsmyuri: 1}).you;
    return adminDB.aggregate([{$currentOp: {localOps: true}}, {$match: {client: myUri}}])
        .toArray()[0]
        .desc;
};

let clientName = getClientName();

try {
    assert.commandWorked(db.adminCommand({
        "configureFailPoint": 'currentOpSpawnsThreadWaitingForLatch',
        "mode": 'alwaysOn',
        "data": {
            'clientName': clientName,
        },
    }));

    const verifyResult = function(result) {
        jsTestLog("Verifying " + tojson(result));
        assert(result);
        assert(result.hasOwnProperty("waitingForLatch"));
        assert(result["waitingForLatch"].hasOwnProperty("timestamp"));
        assert(result["waitingForLatch"].hasOwnProperty("captureName"));

        /* Absent until we have efficient enough backtracing
        assert(result["waitingForLatch"].hasOwnProperty("backtrace"));
        result["waitingForLatch"]["backtrace"].forEach(function(frame) {
            assert(frame.hasOwnProperty("addr"));
            assert(typeof frame["addr"] === "string");
            assert(frame.hasOwnProperty("path"));
            assert(typeof frame["path"] === "string");
        });
        */
    };
    getCurrentOp().forEach(function(op) {
        const name = op["desc"];
        if (name in blockedOpClients) {
            jsTestLog("Verifying " + op["desc"]);
            verifyResult(op);
            blockedOpClients[name].seen = true;
        }
    });

    // Make sure we saw the ops we expected
    for (const name in blockedOpClients) {
        assert(blockedOpClients[name].seen);
    }
} finally {
    assert.commandWorked(db.adminCommand(
        {"configureFailPoint": 'currentOpSpawnsThreadWaitingForLatch', "mode": 'off'}));

    getCurrentOp().forEach(function(op) {
        const name = op["desc"];
        if (name in blockedOpClients) {
            assert(!op.hasOwnProperty("waitingForLatch"));
        }
    });
}
})();
