/*
 * Test applyops upsert flag SERVER-7452
 *
 * @tags: [
 *   # The test runs commands that are not allowed with security token: applyOps.
 *   not_allowed_with_signed_security_token,
 *   requires_non_retryable_commands,
 *   requires_fastcount,
 *   # 6.2 removes support for atomic applyOps
 *   requires_fcv_62,
 *   # applyOps is not supported on mongos
 *   assumes_against_mongod_not_mongos,
 *   # applyOps uses the oplog that require replication support
 *   requires_replication,
 *   # Tenant migrations don't support applyOps.
 *   tenant_migration_incompatible,
 * ]
 */

var t = db.apply_ops2;
t.drop();

assert.eq(0, t.find().count(), "test collection not empty");

t.insert({_id: 1, x: "init"});

// alwaysUpsert = true
print("Testing applyOps with alwaysUpsert = true");

var res = db.runCommand({
    applyOps: [
        {
            op: "u",
            ns: t.getFullName(),
            o2: {_id: 1},
            o: {$v: 2, diff: {u: {x: "upsert=true existing"}}}
        },
        {
            op: "u",
            ns: t.getFullName(),
            o2: {_id: 2},
            o: {$v: 2, diff: {u: {x: "upsert=true non-existing"}}}
        }
    ],
    alwaysUpsert: true
});

assert.eq(true, res.results[0], "upsert = true, existing doc update failed");
assert.eq(true, res.results[1], "upsert = true, nonexisting doc not upserted");
assert.eq(2, t.find().count(), "2 docs expected after upsert");

// alwaysUpsert = false
print("Testing applyOps with alwaysUpsert = false");

res = db.runCommand({
    applyOps: [
        {
            op: "u",
            ns: t.getFullName(),
            o2: {_id: 1},
            o: {$v: 2, diff: {u: {x: "upsert=false existing"}}}
        },

        {
            op: "u",
            ns: t.getFullName(),
            o2: {_id: 3},
            o: {$v: 2, diff: {u: {x: "upsert=false non-existing"}}}
        }

    ],
    alwaysUpsert: false
});

assert.eq(true, res.results[0], "upsert = true, existing doc expected to succeed, but it failed");
assert.eq(false, res.results[1], "upsert = false, nonexisting doc upserted");
assert.eq(2, t.find().count(), "2 docs expected after upsert failure");

// alwaysUpsert not specified, should default to true
print("Testing applyOps with default alwaysUpsert");

res = db.runCommand({
    applyOps: [
        {
            op: "u",
            ns: t.getFullName(),
            o2: {_id: 1},
            o: {$v: 2, diff: {u: {x: "upsert=default existing"}}}
        },

        {
            op: "u",
            ns: t.getFullName(),
            o2: {_id: 4},

            o: {$v: 2, diff: {u: {x: "upsert=default non-existing"}}}
        },
    ]
});

assert.eq(true, res.results[0], "default upsert, existing doc update failed");
assert.eq(true, res.results[1], "default upsert, nonexisting doc not upserted");
assert.eq(3, t.find().count(), "2 docs expected after upsert failure");
