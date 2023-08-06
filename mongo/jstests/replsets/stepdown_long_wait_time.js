// Ensure writes do not prevent a node from Stepping down
// 1. Start up a 3 node set (1 arbiter).
// 2. Stop replication on the SECONDARY using a fail point.
// 3. Do one write and then spin up a second shell which asks the PRIMARY to StepDown.
// 4. Once StepDown has begun, try to do a write and ensure that it fails with NotWritablePrimary
// 5. Restart replication on the SECONDARY.
// 6. Wait for PRIMARY to StepDown.

import {restartServerReplication, stopServerReplication} from "jstests/libs/write_concern_util.js";

var name = "stepDownWithLongWait";
var replSet = new ReplSetTest({name: name, nodes: 3});
var nodes = replSet.nodeList();
replSet.startSet();
replSet.initiate({
    "_id": name,
    "members": [
        {"_id": 0, "host": nodes[0], "priority": 3},
        {"_id": 1, "host": nodes[1]},
        {"_id": 2, "host": nodes[2], "arbiterOnly": true}
    ]
});

replSet.waitForState(replSet.nodes[0], ReplSetTest.State.PRIMARY);
var primary = replSet.getPrimary();
// The default WC is majority and stopServerReplication will prevent satisfying any majority writes.
assert.commandWorked(primary.adminCommand(
    {setDefaultRWConcern: 1, defaultWriteConcern: {w: 1}, writeConcern: {w: "majority"}}));

var secondary = replSet.getSecondary();
jsTestLog('Disable replication on the SECONDARY ' + secondary.host);
stopServerReplication(secondary);

jsTestLog("do a write then ask the PRIMARY to stepdown");
var options = {writeConcern: {w: 1, wtimeout: ReplSetTest.kDefaultTimeoutMS}};
assert.commandWorked(primary.getDB(name).foo.insert({x: 1}, options));

var stepDownCmd = function() {
    assert.commandWorked(db.adminCommand({replSetStepDown: 60, secondaryCatchUpPeriodSecs: 60}));
};
var stepDowner = startParallelShell(stepDownCmd, primary.port);

assert.soon(function() {
    var res = primary.getDB('admin').currentOp(true);
    for (var entry in res.inprog) {
        if (res.inprog[entry]["command"] &&
            res.inprog[entry]["command"]["replSetStepDown"] === 60) {
            return true;
        }
    }
    printjson(res);
    return false;
}, "No pending stepdown command found");

jsTestLog("Ensure that writes start failing with NotWritablePrimary errors");
assert.soonNoExcept(function() {
    assert.commandFailedWithCode(primary.getDB(name).foo.insert({x: 2}),
                                 ErrorCodes.NotWritablePrimary);
    return true;
});

jsTestLog("Ensure that even though writes are failing with NotWritablePrimary, we still report " +
          "ourselves as PRIMARY");
assert.eq(ReplSetTest.State.PRIMARY, primary.adminCommand('replSetGetStatus').myState);

jsTestLog('Enable replication on the SECONDARY ' + secondary.host);
restartServerReplication(secondary);

jsTestLog("Wait for PRIMARY " + primary.host + " to completely step down.");
replSet.waitForState(primary, ReplSetTest.State.SECONDARY);
var exitCode = stepDowner();

jsTestLog("Wait for SECONDARY " + secondary.host + " to become PRIMARY");
replSet.waitForState(secondary, ReplSetTest.State.PRIMARY);
replSet.stopSet();