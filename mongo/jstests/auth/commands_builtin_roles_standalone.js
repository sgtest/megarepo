/**
 * Exhaustive test for authorization of commands with builtin roles.
 *
 * Runs tests on a standalone server.
 *
 * The test logic implemented here operates on the test cases defined
 * in jstests/auth/lib/commands_lib.js
 */

import {runAllCommandsBuiltinRoles} from "jstests/auth/lib/commands_builtin_roles.js";

const dbPath = MongoRunner.toRealDir("$dataDir/commands_built_in_roles_standalone/");
mkdir(dbPath);
const opts = {
    auth: "",
    setParameter: {
        trafficRecordingDirectory: dbPath,
        mongotHost: "localhost:27017",  // We have to set the mongotHost parameter for the
                                        // $search-relatead tests to pass configuration checks.
        syncdelay: 0  // Disable checkpoints as this can cause some commands to fail transiently.
    }
};

// run all tests standalone
const conn = MongoRunner.runMongod(opts);
runAllCommandsBuiltinRoles(conn);
MongoRunner.stopMongod(conn);
