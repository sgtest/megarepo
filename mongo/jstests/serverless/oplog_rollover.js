/**
 * Test that oplog (on both primary and secondary) rolls over when its size exceeds the configured
 * maximum. This test runs on wiredTiger storage engine for the serverless environment.
 */
import {oplogRolloverTest} from "jstests/replsets/libs/oplog_rollover_test.js";

oplogRolloverTest("wiredTiger", false /* initialSyncMethod */, true /* serverless */);