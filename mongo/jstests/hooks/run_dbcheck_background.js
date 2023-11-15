/**
 * Runs dbCheck in background.
 */
import {DiscoverTopology, Topology} from "jstests/libs/discover_topology.js";
import {Thread} from "jstests/libs/parallelTester.js";
import {
    assertForDbCheckErrorsForAllNodes,
    runDbCheckForDatabase
} from "jstests/replsets/libs/dbcheck_utils.js";

if (typeof db === 'undefined') {
    throw new Error(
        "Expected mongo shell to be connected a server, but global 'db' object isn't defined");
}

TestData = TestData || {};

// Disable implicit sessions so FSM workloads that kill random sessions won't interrupt the
// operations in this test that aren't resilient to interruptions.
TestData.disableImplicitSessions = true;

const conn = db.getMongo();
const topology = DiscoverTopology.findConnectedNodes(conn);

const exceptionFilteredBackgroundDbCheck = function(hosts) {
    // Set a higher rate to let 'maxDocsPerBatch' be the only limiting factor.
    assert.commandWorkedOrFailedWithCode(
        db.adminCommand({setParameter: 1, maxDbCheckMBperSec: 1024}), ErrorCodes.InvalidOptions);
    const runBackgroundDbCheck = function(hosts) {
        const quietly = (func) => {
            const printOriginal = print;
            try {
                print = Function.prototype;
                func();
            } finally {
                print = printOriginal;
            }
        };

        let rst;
        // We construct the ReplSetTest instance with the print() function overridden to be a no-op
        // in order to suppress the log messages about the replica set configuration. The
        // run_dbcheck_background.js hook is executed frequently by resmoke.py and would
        // otherwise lead to generating an overwhelming amount of log messages.
        quietly(() => {
            rst = new ReplSetTest(hosts[0]);
        });

        const dbNames = new Set();
        const primary = rst.getPrimary();

        const version = assert
                            .commandWorked(primary.adminCommand(
                                {getParameter: 1, featureCompatibilityVersion: 1}))
                            .featureCompatibilityVersion.version;
        if (version != latestFCV) {
            print("Not running dbCheck in FCV " + version);
            return {ok: 1};
        }

        print("Running dbCheck for: " + rst.getURL());

        const adminDb = primary.getDB('admin');
        let res = assert.commandWorked(adminDb.runCommand({listDatabases: 1, nameOnly: true}));
        for (let dbInfo of res.databases) {
            dbNames.add(dbInfo.name);
        }

        // Transactions cannot be run on the following databases so we don't attempt to read at a
        // clusterTime on them either. (The "local" database is also not replicated.)
        // The config.transactions collection is different between primaries and secondaries.
        dbNames.delete('config');
        dbNames.delete('local');

        dbNames.forEach((dbName) => {
            jsTestLog("dbCheck is starting on database " + dbName + " for RS: " + rst.getURL());
            runDbCheckForDatabase(rst, primary.getDB(dbName), true /*awaitCompletion*/);
            jsTestLog("dbCheck is done on database " + dbName + " for RS: " + rst.getURL());
        });

        assertForDbCheckErrorsForAllNodes(
            rst, true /*assertForErrors*/, false /*assertForWarnings*/);

        return {ok: 1};
    };

    const onDrop = function(e) {
        jsTestLog("Skipping dbCheck due to transient error: " + tojson(e));
        return {ok: 1};
    };

    return assert.dropExceptionsWithCode(() => {
        return runBackgroundDbCheck(hosts);
    }, [ErrorCodes.NamespaceNotFound, ErrorCodes.LockTimeout, ErrorCodes.Interrupted], onDrop);
};

if (topology.type === Topology.kReplicaSet) {
    let res = exceptionFilteredBackgroundDbCheck(topology.nodes);
    assert.commandWorked(res, () => 'dbCheck replication consistency check failed: ' + tojson(res));
} else if (topology.type === Topology.kShardedCluster) {
    const threads = [];
    try {
        if (topology.configsvr.type === Topology.kReplicaSet) {
            const thread = new Thread(exceptionFilteredBackgroundDbCheck, topology.configsvr.nodes);
            threads.push(thread);
            thread.start();
        }

        for (let shardName of Object.keys(topology.shards)) {
            const shard = topology.shards[shardName];
            if (shard.type === Topology.kReplicaSet) {
                const thread = new Thread(exceptionFilteredBackgroundDbCheck, shard.nodes);
                threads.push(thread);
                thread.start();
            } else {
                throw new Error('Unrecognized topology format: ' + tojson(topology));
            }
        }
    } finally {
        // Wait for each thread to finish. Throw an error if any thread fails.
        let exception;
        const returnData = threads.map(thread => {
            try {
                thread.join();
                return thread.returnData();
            } catch (e) {
                if (!exception) {
                    exception = e;
                }
            }
        });
        if (exception) {
            // eslint-disable-next-line
            throw exception;
        }

        returnData.forEach(res => {
            assert.commandWorked(
                res, () => 'dbCheck replication consistency check failed: ' + tojson(res));
        });
    }
} else {
    throw new Error('Unsupported topology configuration: ' + tojson(topology));
}
