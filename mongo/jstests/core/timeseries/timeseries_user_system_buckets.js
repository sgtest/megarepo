/**
 * A user creating directly a bucket namespace is required not to fail. This test attempts to test
 * all possible combination of creating a collection/timeseries (with a non-bucket/bucket nss) with
 * an already created collection/timeseries (with a non-bucket/bucket nss). This test also attempts
 * to be a reference on what behaviour expect when creating a bucket namespaces and performing an
 * insert on it.
 *
 *  @tags: [
 *   # We need a timeseries collection.
 *   requires_timeseries,
 *   # "Overriding safe failed response for :: create"
 *   does_not_support_stepdowns,
 *   # Running shardCollection instead of createCollection returns different error types which are
 *   # not expected by the test
 *   assumes_unsharded_collection,
 *   # TODO SERVER-89827 re-enable upgrade/downgrade
 *   cannot_run_during_upgrade_downgrade,
 *
 * ]
 */
import {FeatureFlagUtil} from "jstests/libs/feature_flag_util.js";
import {FixtureHelpers} from "jstests/libs/fixture_helpers.js";

const tsOptions = {
    timeField: "timestamp",
    metaField: "metadata"
};
const tsOptions2 = {
    timeField: "timestamp",
    metaField: "metadata2"
};
const kColl = "coll"
const kBucket = "system.buckets.coll"

function createWorked(collName, tsOptions = {}) {
    if (Object.keys(tsOptions).length === 0) {
        assert.commandWorked(db.createCollection(collName));
    } else {
        assert.commandWorked(db.createCollection(collName, {timeseries: tsOptions}));
    }
    return db.getCollection(collName);
}

function createFailed(collName, tsOptions, errorCode) {
    if (Object.keys(tsOptions).length === 0) {
        assert.commandFailedWithCode(db.createCollection(collName), errorCode);
    } else {
        let res = db.createCollection(collName, {timeseries: tsOptions});
        assert.commandFailedWithCode(res, errorCode);
    }
}

// TODO SERVER-77915 remove this helper. Now a non-tracked bucket timeseries creation should behave
// the same as the tracked one executed by the coordinator.
function isCollectionTracked(collName) {
    let coll = db.getCollection(collName);
    return FixtureHelpers.isTracked(coll) &&
        FeatureFlagUtil.isPresentAndEnabled(db, "TrackUnshardedCollectionsUponCreation");
}

function runTest(testCase, minRequiredVersion = null) {
    if (minRequiredVersion != null) {
        const res = db.getSiblingDB("admin")
                        .system.version.find({_id: "featureCompatibilityVersion"})
                        .toArray();
        if (res.length == 0) {
            // For specific suites like multitenancy we don't have the privileges to access to
            // specific databases. If we cannot establish the version, let's skip the test case.
            return;
        }
        if (MongoRunner.compareBinVersions(res[0].version, minRequiredVersion) < 0) {
            return;
        }
    }
    testCase()
    db.dropDatabase();
}

// Reset any previous run state.
db.dropDatabase()

// Case prexisting collection: standard.
{
    jsTest.log("Case collection: standard / collection: standard.");
    runTest(() => {
        createWorked(kColl);
        createWorked(kColl, {});
    });

    jsTest.log("Case collection: standard / collection: timeseries.");
    runTest(() => {
        createWorked(kColl);
        createFailed(kColl, tsOptions, ErrorCodes.NamespaceExists);
    });

    jsTest.log("Case collection: standard / collection: bucket timeseries.");
    runTest(() => {
        createWorked(kColl);
        if (isCollectionTracked(kColl)) {
            createFailed(kBucket, tsOptions, ErrorCodes.NamespaceExists);
        } else {
            // TODO SERVER-85855 creating bucket timeseries for a collection already created should
            // fail.
            createWorked(kBucket, tsOptions);
        }
    });
}

// Case prexisting collection: timeseries.
{
    jsTest.log("Case collection: timeseries / collection: standard.");
    runTest(() => {
        createWorked(kColl, tsOptions);
        createFailed(kColl, {}, ErrorCodes.NamespaceExists);
    });

    jsTest.log("Case collection: timeseries / collection: timeseries.");
    runTest(
        () => {
            createWorked(kColl, tsOptions);
            createWorked(kColl, tsOptions);
        },
        // On 7.0 this test case wrongly fails with NamespaceExists and should not be tested.
        "7.1"  // minRequiredVersion
    );

    jsTest.log("Case collection: timeseries / collection: timeseries with different options.");
    runTest(() => {
        createWorked(kColl, tsOptions);
        createFailed(kColl, tsOptions2, ErrorCodes.NamespaceExists);
    });

    jsTest.log("Case collection: timeseries / collection: bucket timeseries.");
    runTest(
        () => {
            createWorked(kColl, tsOptions);
            createWorked(kBucket, tsOptions);
        },
        // Creation of bucket namespace is not idempotent before 8.0 (SERVER-89827)
        "8.0"  // minRequiredVersion
    );

    jsTest.log(
        "Case collection: timeseries / collection: bucket timeseries with different options.");
    runTest(() => {
        createWorked(kColl, tsOptions);
        createFailed(kBucket, tsOptions2, ErrorCodes.NamespaceExists);
    });
}

// Case prexisting bucket collection timeseries.
{
    jsTest.log("Case collection: bucket timeseries / collection: standard.");
    runTest(() => {
        createWorked(kBucket, tsOptions)
        if (isCollectionTracked(kBucket)) {
            createFailed(kColl, {}, ErrorCodes.NamespaceExists);
        }
        else {
            // TODO SERVER-85855 creating a normal collection with an already created bucket
            // timeseries should fail.
            createWorked(kColl);
        }
    });

    jsTest.log("Case collection: bucket timeseries / collection: timeseries.");
    runTest(
        () => {
            createWorked(kBucket, tsOptions)
            createWorked(kColl, tsOptions);
        },
        // Creation of bucket namespace is not idempotent before 8.0 (SERVER-89827)
        "8.0"  // minRequiredVersion
    );

    jsTest.log(
        "Case collection: bucket timeseries / collection: timeseries with different options.");
    runTest(() => {
        createWorked(kBucket, tsOptions)
        createFailed(kColl, tsOptions2, ErrorCodes.NamespaceExists);
    });

    jsTest.log(
        "Case collection: bucket timeseries / collection: bucket timeseries with different options.");
    runTest(() => {
        createWorked(kBucket, tsOptions)
        createFailed(kBucket, tsOptions2, ErrorCodes.NamespaceExists);
    });

    jsTest.log("Case collection: bucket timeseries / collection: bucket timeseries.");
    runTest(
        () => {
            createWorked(kBucket, tsOptions);
            createWorked(kBucket, tsOptions);
        },
        // Creation of bucket namespace is not idempotent before 8.0 (SERVER-89827)
        "8.0"  // minRequiredVersion
    );
}
