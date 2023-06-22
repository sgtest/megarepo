/**
 * Tests that the internal parameter "$_resumeAfter" validates the type of the 'recordId' for
 * clustered and non clustered collections.
 * @tags: [
 *   # Queries on mongoS may not request or provide a resume token.
 *   assumes_against_mongod_not_mongos,
 * ]
 */

(function() {
"use strict";

load("jstests/libs/collection_drop_recreate.js");  // For assertDropCollection.
load("jstests/libs/sbe_util.js");                  // For checkSBEEnabled.
load("jstests/libs/feature_flag_util.js");

const clustered = db.clusteredColl;
const nonClustered = db.normalColl;
const clusteredName = clustered.getName();
const nonClusteredName = nonClustered.getName();

assertDropCollection(db, clusteredName);
assertDropCollection(db, nonClusteredName);

db.createCollection(clusteredName, {clusteredIndex: {key: {_id: 1}, unique: true}});
db.createCollection(nonClusteredName);

// TODO: SERVER-78103 remove check.
const isSBEEnabled = checkSBEEnabled(db);

// Insert some documents.
const docs = [{_id: 1, a: 1}, {_id: 2, a: 2}, {_id: 3, a: 3}];
assert.commandWorked(clustered.insertMany(docs));
assert.commandWorked(nonClustered.insertMany(docs));

function validateFailedResumeAfterInFind({collName, resumeAfterSpec, errorCode, explainFail}) {
    const spec = {
        find: collName,
        filter: {},
        $_requestResumeToken: true,
        $_resumeAfter: resumeAfterSpec,
        hint: {$natural: 1}
    };
    assert.commandFailedWithCode(db.runCommand(spec), errorCode);
    // Run the same query under an explain.
    if (explainFail) {
        assert.commandFailedWithCode(db.runCommand({explain: spec}), errorCode);
    } else {
        assert.commandWorked(db.runCommand({explain: spec}));
    }
}

function validateFailedResumeAfterInAggregate({collName, resumeAfterSpec, errorCode, explainFail}) {
    const spec = {
        aggregate: collName,
        pipeline: [],
        $_requestResumeToken: true,
        $_resumeAfter: resumeAfterSpec,
        hint: {$natural: 1},
        cursor: {}
    };
    assert.commandFailedWithCode(db.runCommand(spec), errorCode);
    // Run the same query under an explain.
    if (explainFail) {
        assert.commandFailedWithCode(db.runCommand({explain: spec}), errorCode);
    } else {
        assert.commandWorked(db.runCommand({explain: spec}));
    }
}

function testResumeAfter(validateFunction) {
    //  Confirm $_resumeAfter will fail for clustered collections if the recordId is Long.
    validateFunction({
        collName: clusteredName,
        resumeAfterSpec: {'$recordId': NumberLong(2)},
        errorCode: 7738600,
        explainFail: true
    });

    // Confirm $_resumeAfter will fail with 'KeyNotFound' if given a non existent recordId.
    validateFunction({
        collName: clusteredName,
        resumeAfterSpec: {'$recordId': BinData(5, '1234')},
        errorCode: ErrorCodes.KeyNotFound
    });

    // TODO SERVER-78103: Added test for $recordId:null

    // Confirm $_resumeAfter will fail for normal collections if it is of type BinData.
    validateFunction({
        collName: nonClusteredName,
        resumeAfterSpec: {'$recordId': BinData(5, '1234')},
        errorCode: 7738600,
        explainFail: true
    });

    // Confirm $_resumeAfter token will fail with 'KeyNotFound' if given a non existent recordId.
    validateFunction({
        collName: nonClusteredName,
        resumeAfterSpec: {'$recordId': NumberLong(8)},
        errorCode: ErrorCodes.KeyNotFound
    });

    if (isSBEEnabled) {
        validateFunction({
            collName: nonClusteredName,
            resumeAfterSpec: {'$recordId': null},
            errorCode: ErrorCodes.KeyNotFound
        });
    } else {
        assert.commandWorked(db.runCommand({
            find: nonClusteredName,
            filter: {},
            $_requestResumeToken: true,
            $_resumeAfter: {'$recordId': null},
            hint: {$natural: 1}
        }));
    }

    // Confirm $_resumeAfter will fail to parse if collection does not exist.
    validateFunction({
        collName: "random",
        resumeAfterSpec: {'$recordId': null, "anotherField": null},
        errorCode: ErrorCodes.BadValue,
        explainFail: true
    });
    validateFunction({
        collName: "random",
        resumeAfterSpec: "string",
        errorCode: ErrorCodes.TypeMismatch,
        explainFail: true
    });
}

testResumeAfter(validateFailedResumeAfterInFind);
if (FeatureFlagUtil.isEnabled(db, "ReshardingImprovements")) {
    testResumeAfter(validateFailedResumeAfterInAggregate);
}
}());
