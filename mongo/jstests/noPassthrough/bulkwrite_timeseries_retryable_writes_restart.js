/**
 * Tests time-series retryable writes oplog entries are correctly chained together so that a retry
 * after restarting the server doesn't perform a write that was already executed.
 *
 * This runs timeseries_retryable_writes_restart.js while overriding all CRUD commands to use
 * bulkWrite.
 *
 * @tags: [
 *   featureFlagBulkWriteCommand,
 *   requires_replication,
 *   requires_persistence,
 * ]
 */
await import('jstests/libs/override_methods/single_crud_op_as_bulk_write.js');
await import('jstests/noPassthrough/timeseries_retryable_writes_restart.js');