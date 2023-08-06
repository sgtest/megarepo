/**
 * Test to verify that the 'comment' field set while running a command gets populated in $currentOp
 * and profiler.
 * The test runs commands that are not allowed with security token: setProfilingLevel.
 * @tags: [
 *   not_allowed_with_security_token,
 *   assumes_against_mongod_not_mongos,
 *   assumes_read_concern_unchanged,
 *   assumes_unsharded_collection,
 *   assumes_write_concern_unchanged,
 *   does_not_support_stepdowns,
 *   requires_profiling,
 *   uses_parallel_shell,
 *   no_selinux,
 *   # This test uses legacy read mode which does not use runCommand (required by the
 *   # inject_tenant_prefix.js override).
 *   tenant_migration_incompatible,
 * ]
 */

import {profilerHasSingleMatchingEntryOrThrow} from "jstests/libs/profiler.js";

// This test runs manual getMores using different connections, which will not inherit the
// implicit session of the cursor establishing command.
TestData.disableImplicitSessions = true;

const testDB = db.getSiblingDB(jsTestName());
const adminDB = db.getSiblingDB("admin");

const coll = testDB.coll;
coll.drop();

assert.commandWorked(coll.insert({x: 1, _id: 1}));
assert.commandWorked(coll.insert({x: 1, _id: 2}));

function setPostCommandFailpoint({mode, options}) {
    assert.commandWorked(db.adminCommand(
        {configureFailPoint: "waitAfterCommandFinishesExecution", mode: mode, data: options}));
}

function restartProfiler() {
    // Restart profiler.
    testDB.setProfilingLevel(0);
    testDB.system.profile.drop();
    testDB.setProfilingLevel(2);
}

function runCommentParamTest({coll, command, commentObj}) {
    const cmdName = Object.keys(command)[0];
    if (!commentObj) {
        commentObj = {testName: jsTestName(), commentField: "comment_" + cmdName};
        command["comment"] = commentObj;
    }
    restartProfiler();

    let parallelShell;
    try {
        setPostCommandFailpoint(
            {mode: "alwaysOn", options: {ns: coll.getFullName(), commands: [cmdName]}});

        const parallelFn = `
            const sourceDB = db.getSiblingDB(jsTestName());
            let cmdRes = sourceDB.runCommand(${tojson(command)});
            assert.commandWorked(cmdRes); `;

        // Run the 'command' in a parallel shell.
        parallelShell = startParallelShell(parallelFn);

        // Wait for the parallel shell to hit the failpoint and verify that the 'comment' field is
        // present in $currentOp.
        const filter = {[`command.${cmdName}`]: {$exists: true}, "command.comment": commentObj};

        assert.soon(
            () => adminDB.aggregate([{$currentOp: {}}, {$match: filter}]).toArray().length == 1,
            () => tojson(adminDB.aggregate([{$currentOp: {}}]).toArray()));

    } finally {
        // Ensure that we unset the failpoint, regardless of the outcome of the test.
        setPostCommandFailpoint({mode: "off", options: {}});
    }
    // Wait for the parallel shell to complete.
    parallelShell();

    // Verify that profile entry has 'comment' field.
    profilerHasSingleMatchingEntryOrThrow(
        {profileDB: testDB, filter: {"command.comment": commentObj}});
}

// Verify that the comment attached to a find command appears in both currentOp and the profiler.
runCommentParamTest({coll: coll, command: {find: coll.getName(), filter: {}}});

// Verify that the comment attached to an insert command appears in both currentOp and the profiler.
runCommentParamTest({
    coll: coll,
    command: {insert: coll.getName(), documents: [{x: 0.5}, {x: -0.5}], ordered: false}
});

// Verify that the comment attached to an aggregate command appears in both currentOp and the
// profiler.
runCommentParamTest({
    coll: coll,
    command: {aggregate: coll.getName(), pipeline: [], cursor: {batchSize: 1}},
});

// Verify the 'comment' field on the aggreage command is propagated to the subsequent getMore
// command.
const comment = [{name: "agg_comment"}];
const res = testDB.runCommand(
    {aggregate: coll.getName(), pipeline: [], comment: comment, cursor: {batchSize: 1}});
runCommentParamTest({
    coll: coll,
    command: {getMore: res.cursor.id, collection: coll.getName(), batchSize: 1},
    commentObj: comment
});

// Verify the 'comment' field on the getMore command takes precedence over the 'comment' field on
// the originating command.
runCommentParamTest(
    {coll: coll, command: {getMore: res.cursor.id, collection: coll.getName(), batchSize: 1}});

// Verify that comment field gets populated on the profiler for aggregate with explain:true.
runCommentParamTest({
    coll: coll,
    command: {aggregate: coll.getName(), pipeline: [], explain: true},
});

const innerComment = {
    name: "innerComment_aggregation"
};

// Verify that a comment field attached to the inner command of an explain command gets populated in
// profiler as top level 'comment'.
runCommentParamTest({
    coll: coll,
    command:
        {explain: {aggregate: coll.getName(), pipeline: [], cursor: {}, comment: innerComment}},
    commentObj: innerComment
});

// Verify that when a comment field is attached to the inner command of an explain command and there
// is another 'comment' field at the top level, top level comment takes precedence.
runCommentParamTest({
    coll: coll,
    command: {explain: {aggregate: coll.getName(), pipeline: [], cursor: {}, comment: innerComment}}
});