/**
 * Test cloneCollectionAsCapped
 *
 * @tags: [
 *  # The test runs commands that are not allowed with security token: cloneCollectionAsCapped,
 *  # convertToCapped.
 *  not_allowed_with_signed_security_token,
 *  requires_non_retryable_commands,
 *  requires_fastcount,
 *  requires_capped,
 *  # capped collections connot be sharded
 *  assumes_unsharded_collection,
 *  # cloneCollectionAsCapped command is not supported on mongos
 *  assumes_against_mongod_not_mongos,
 *  # cloneCollectionAsCapped (and capped collections) are not supported on serverless
 *  tenant_migration_incompatible,
 * ]
 */

let source = db.capped_convertToCapped1;
let dest = db.capped_convertToCapped1_clone;

source.drop();
dest.drop();

let N = 1000;

for (let i = 0; i < N; ++i) {
    source.save({i: i});
}
assert.eq(N, source.count());

// should all fit
let res = db.runCommand(
    {cloneCollectionAsCapped: source.getName(), toCollection: dest.getName(), size: 100000});
assert.commandWorked(res);
assert.eq(source.count(), dest.count());
assert.eq(N, source.count());  // didn't delete source

dest.drop();
// should NOT all fit
assert.commandWorked(db.runCommand(
    {cloneCollectionAsCapped: source.getName(), toCollection: dest.getName(), size: 1000}));

assert.eq(N, source.count());  // didn't delete source
assert.gt(source.count(), dest.count());
