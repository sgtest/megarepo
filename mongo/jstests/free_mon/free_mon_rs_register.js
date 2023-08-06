// Validate registration works in a replica set
//
import {
    FreeMonWebServer,
    ValidateFreeMonReplicaSet,
    WaitForFreeMonServerStatusState,
    WaitForRegistration,
    WaitForUnRegistration,
} from "jstests/free_mon/libs/free_mon.js";

let mock_web = new FreeMonWebServer();

mock_web.start();

let options = {
    setParameter: "cloudFreeMonitoringEndpointURL=" + mock_web.getURL(),
    verbose: 1,
};

const rst = new ReplSetTest({nodes: 2, nodeOptions: options});

rst.startSet();
rst.initiate();
rst.awaitReplication();

sleep(10 * 1000);
assert.eq(0, mock_web.queryStats().registers, "mongod registered without enabling free_mod");

assert.commandWorked(rst.getPrimary().adminCommand({setFreeMonitoring: 1, action: "enable"}));
WaitForRegistration(rst.getPrimary());

mock_web.waitRegisters(2);

WaitForRegistration(rst.getPrimary());
WaitForRegistration(rst.getSecondary());
ValidateFreeMonReplicaSet(rst);

const last_register = mock_web.query("last_register");
print(tojson(last_register));

assert.eq(last_register.version, 2);
assert.eq(last_register.payload.buildInfo.bits, 64);
assert.eq(last_register.payload.buildInfo.ok, 1);
assert.eq(last_register.payload.storageEngine.readOnly, false);
assert.eq(last_register.payload.isMaster.ok, 1);
assert.gte(last_register.payload.replSetGetConfig.config.version, 2);

function isUUID(val) {
    // Mock webserver gives us back unpacked BinData/UUID in the form:
    //"$binary" : {"base64" : "2gzkSY3bTlu/k3bXfpPUKg==", "subType" : "04"}
    if ((typeof val) !== 'object') {
        return false;
    }
    const binary = val['$binary'];
    const subType = binary['subType'];
    const base64 = binary['base64'];

    // This number is the indentifier for a UUID.
    // https://www.mongodb.com/docs/manual/reference/bson-types/#binary-data
    if (subType !== '04') {
        return false;
    }

    // Validate base64
    return base64.match('^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$') !==
        null;
}
assert.eq(isUUID(last_register.payload.uuid['local.oplog.rs']), true);

// Restart the secondary
var s1 = rst.getSecondary();
var s1Id = rst.getNodeId(s1);

rst.stop(s1Id);
rst.waitForState(s1, ReplSetTest.State.DOWN);

rst.restart(s1Id);

mock_web.waitRegisters(3);

// Now disable it
assert.commandWorked(rst.getPrimary().adminCommand({setFreeMonitoring: 1, action: "disable"}));

WaitForUnRegistration(rst.getPrimary());
WaitForUnRegistration(rst.getSecondary());

WaitForFreeMonServerStatusState(rst.getPrimary(), 'disabled');
WaitForFreeMonServerStatusState(rst.getSecondary(), 'disabled');

// Restart the secondary with it disabled
var s1 = rst.getSecondary();
var s1Id = rst.getNodeId(s1);

rst.stop(s1Id);
rst.waitForState(s1, ReplSetTest.State.DOWN);

rst.restart(s1Id);

// Make sure it is disabled
WaitForFreeMonServerStatusState(rst.getPrimary(), 'disabled');
WaitForFreeMonServerStatusState(rst.getSecondary(), 'disabled');

rst.stopSet();

mock_web.stop();
