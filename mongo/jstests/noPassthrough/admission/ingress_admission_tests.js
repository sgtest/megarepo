/**
 * Test that the ingress admission control works correctly.
 * @tags: [featureFlagIngressAdmissionControl]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {findMatchingLogLine} from "jstests/libs/log.js";
import {funWithArgs} from "jstests/libs/parallel_shell_helpers.js";

/**
 * Find one specific command in current ops, by its comment
 */
function findCurOpByComment(db, comment) {
    const res = assert.commandWorked(db.adminCommand({currentOp: 1}));
    const ops = res["inprog"].filter((op) => op["command"]["comment"] == comment);
    if (ops.length == 0) {
        return null;
    }
    return ops[0];
}

/**
 * Wait until ingress admission control ticket pool is resized to 0
 */
function waitUntilIngressAdmissionIsBlocked(db) {
    assert.soon(() => {
        const res = assert.commandWorked(
            db.adminCommand({getParameter: 1, ingressAdmissionControllerTicketPoolSize: 1}));
        return res.ingressAdmissionControllerTicketPoolSize == 0;
    });
}

/**
 * Test that the operations waiting for ingress admission can be distinguished in currentOp.
 */
function testCurrentOp(conn, db, collName) {
    const kComment = "ingress admission queue curop test";

    // run a parallel shell that can wait for admission control
    let parallelShell = startParallelShell(
        funWithArgs((dbName, collName, comment) => {
            let testDB = db.getSiblingDB(dbName);

            // make the next controlled command wait for ingress admission
            assert.commandWorked(testDB.adminCommand(
                {setParameter: 1, ingressAdmissionControllerTicketPoolSize: 0}));

            // wait until the command has been admitted
            assert.commandWorked(testDB.runCommand({count: collName, comment: comment}));
        }, db.getName(), collName, kComment), conn.port, false);

    // wait until the parallel shell has blocked the ingress admission
    waitUntilIngressAdmissionIsBlocked(db);

    // confirm that our operation is waiting for ingress admission
    assert.soon(() => {
        const op = findCurOpByComment(db, kComment);
        if (op == null) {
            return false;
        }
        return op.waitingForIngressAdmission;
    });

    // make sure the operation hangs after it was admitted
    const fp = configureFailPoint(conn, "waitAfterCommandFinishesExecution", {commands: ["count"]});

    // unblock ingress admission
    assert.commandWorked(
        db.adminCommand({setParameter: 1, ingressAdmissionControllerTicketPoolSize: 1}));

    // confirm that our operation is no longer waiting for ingress admission
    assert.soon(() => {
        const op = findCurOpByComment(conn, kComment);
        if (op == null) {
            return false;
        }
        return !op.waitingForIngressAdmission;
    });

    fp.off();
    parallelShell();
}

/**
 * Test that the time spent waiting for ingress admission is reported in slow query log.
 */
function testSlowQueryLog(conn, db, collName) {
    const kDelayMillis = 50;
    const kDelayMicros = kDelayMillis * 1000;

    // run a parallel shell that can wait for admission control
    let parallelShell =
        startParallelShell(funWithArgs((dbName, collName) => {
                               let testDB = db.getSiblingDB(dbName);

                               // report every command as slow
                               testDB.setProfilingLevel(0, -1);

                               // make the next controlled command wait for ingress admission
                               assert.commandWorked(testDB.adminCommand(
                                   {setParameter: 1, ingressAdmissionControllerTicketPoolSize: 0}));

                               // wait until the command has been admitted
                               assert.commandWorked(testDB.runCommand({count: collName}));
                           }, db.getName(), collName), conn.port, false);

    // wait until the parallel shell has blocked the ingress admission
    waitUntilIngressAdmissionIsBlocked(db);

    // make sure the reported ingress admission wait time will be at least kDelayMillis
    sleep(kDelayMillis);

    // unblock ingress admission
    assert.commandWorked(
        db.adminCommand({setParameter: 1, ingressAdmissionControllerTicketPoolSize: 1}));

    // wait until the parallel shell is finished to stop reporting all commands as slow
    parallelShell();
    db.setProfilingLevel(0, 100);

    // verify that the reported ingress admission wait time is at least kDelayMicros
    let log = assert.commandWorked(db.adminCommand({getLog: "global"})).log;
    let line = findMatchingLogLine(log, {id: 51803, command: "count"});
    assert.neq(line, null);
    let entry = JSON.parse(line);
    assert.gt(entry.attr.ingressAdmissionDurationMicros, kDelayMicros);
}

/**
 * Test that the operations waiting for ingress admission will respect maxTimeMS.
 */
function testMaxTimeMS(db, collName) {
    // ensure controlled operations work with the default pool size
    assert.commandWorked(db.runCommand({count: collName}));

    // block all controlled operations indefinitely
    assert.commandWorked(
        db.adminCommand({setParameter: 1, ingressAdmissionControllerTicketPoolSize: 0}));

    // ensure controlled operations time out while queued
    const cmdRes = db.runCommand({count: collName, maxTimeMS: 500});
    assert.commandFailedWithCode(cmdRes, ErrorCodes.MaxTimeMSExpired);

    // stop blocking controlled operations so we can stop the runner
    assert.commandWorked(
        db.adminCommand({setParameter: 1, ingressAdmissionControllerTicketPoolSize: 1}));
}

function runTests() {
    const mongodOptions = {setParameter: {ingressAdmissionControllerTicketPoolSize: 1}};
    const conn = MongoRunner.runMongod(mongodOptions);

    const db = conn.getDB(`${jsTest.name()}_db`);
    const collName = `${jsTest.name()}_coll`;
    db.getCollection(collName).drop();

    testCurrentOp(conn, db, collName);
    testSlowQueryLog(conn, db, collName);
    testMaxTimeMS(db, collName);

    MongoRunner.stopMongod(conn);
}

runTests();
