/**
 * Test that rolling back an index build, but not collection creation, behaves correctly even when
 * the index build is aborted.
 */
import {RollbackIndexBuildsTest} from "jstests/replsets/libs/rollback_index_builds_test.js";

const rollbackIndexTest = new RollbackIndexBuildsTest(
    [ErrorCodes.InterruptedDueToReplStateChange, ErrorCodes.Interrupted]);

const schedule = [
    // Create the collection
    "createColl",
    // Hold the stable timestamp, if applicable.
    "holdStableTimestamp",
    // Everything after this will be rolled-back.
    "transitionToRollback",
    // The index build will be rolled-back.
    "start",
    // Abort the index build
    "abort",
];

rollbackIndexTest.runSchedules([schedule]);
rollbackIndexTest.stop();
