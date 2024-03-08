/**
 * Tests batch-deleting a large range of data using predicate on the 'a' field.
 * This test does not rely on getMores on purpose, as this is a requirement for running on
 * tenant migration passthroughs.
 *
 * @tags: [
 *   does_not_support_retryable_writes,
 *   # TODO (SERVER-55909): make WUOW 'groupOplogEntries' the only mode of operation.
 *   does_not_support_transactions,
 *   no_selinux,
 *   requires_non_retryable_writes,
 *   # TODO SERVER-87044: re-enable test in suites that perform random migrations
 *   assumes_balancer_off,
 * ]
 */

import {runBatchedMultiDeletesTest} from 'jstests/core/write/delete/libs/batched_multi_deletes.js';

runBatchedMultiDeletesTest(db[jsTestName()], {a: {$gte: 0}});
