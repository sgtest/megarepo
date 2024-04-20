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
 *   # TODO SERVER-85382 re-enable upgrade/downgrade now that the feature flag is no longer checked
 *   cannot_run_during_upgrade_downgrade,
 *
 * ]
 */
import {FeatureFlagUtil} from "jstests/libs/feature_flag_util.js";

const isTrackingUnsplittableCollections = FeatureFlagUtil.isPresentAndEnabled(
    db.getSiblingDB('admin'), "TrackUnshardedCollectionsUponCreation");

// TODO SERVER-85382 re-enable this test with tracked collection once create collection coordinator
// support all timeseries/bucket namespace cases
if (!isTrackingUnsplittableCollections) {
    const tsOptions = {timeField: "timestamp", metaField: "metadata"};
    const tsOptions2 = {timeField: "timestamp", metaField: "metadata2"};
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

        jsTest.log("Case collection: standard / collection: bucket.");
        runTest(() => {
            let coll = createWorked(kColl);
            let bucket = createWorked(kBucket);

            bucket.insert({x: 1});
            var docsSystemBuckets = bucket.find().toArray();
            assert.eq(1, docsSystemBuckets.length);

            coll.insert({x: 1})
            var docs = coll.find().toArray();
            assert.eq(1, docs.length);
        });

        jsTest.log("Case collection: standard / collection: bucket timeseries.");
        runTest(() => {
            createWorked(kColl);
            // TODO SERVER-85855 creating bucket timeseries for a collection already created should
            // fail.
            createWorked(kBucket, tsOptions);
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

        jsTest.log("Case collection: timeseries / collection: bucket.");
        runTest(() => {
            createWorked(kColl, tsOptions);
            createFailed(kBucket, {}, ErrorCodes.NamespaceExists);
        });

        jsTest.log("Case collection: timeseries / collection: bucket timeseries.");
        runTest(
            () => {
                createWorked(kColl, tsOptions);
                createWorked(kBucket, tsOptions);
            },
            // TODO BACKPORT-19356 creation of the bucket used to not be idempotent. Re-enable this
            // test case in previous versions once the backport is completed.
            "8.0"  // minRequiredVersion
        );

        jsTest.log(
            "Case collection: timeseries / collection: bucket timeseries with different options.");
        runTest(() => {
            createWorked(kColl, tsOptions);
            createFailed(kBucket, tsOptions2, ErrorCodes.NamespaceExists);
        });
    }

    // Case prexisting bucket collection.
    jsTest.log("Case collection: bucket / collection: standard.");
    {
        runTest(() => {
            let bucket = createWorked(kBucket);
            let coll = createWorked(kColl);

            bucket.insert({x: 1});
            var docsSystemBuckets = bucket.find().toArray();
            assert.eq(1, docsSystemBuckets.length);

            coll.insert({x: 1})
            var docs = coll.find().toArray();
            assert.eq(1, docs.length);
        });

        jsTest.log("Case collection: bucket / collection: timeseries.");
        runTest(() => {
            createWorked(kBucket);
            createFailed(kColl, tsOptions, ErrorCodes.NamespaceExists);
        });

        jsTest.log("Case collection: bucket / collection: bucket.");
        runTest(() => {
            createWorked(kBucket);
            createWorked(kBucket);
        });

        jsTest.log("Case collection: bucket / collection: bucket timeseries.");
        runTest(() => {
            createWorked(kBucket);
            createFailed(kBucket, tsOptions, ErrorCodes.NamespaceExists);
        });
    }

    // Case prexisting bucket collection timeseries.
    {
        jsTest.log("Case collection: bucket timeseries / collection: standard.");
        runTest(() => {
            createWorked(kBucket, tsOptions)
            // TODO SERVER-85855 creating a normal collection with an already created bucket
            // timeseries should fail.
            createWorked(kColl);
        });

        jsTest.log("Case collection: bucket timeseries / collection: timeseries.");
        runTest(
            () => {
                createWorked(kBucket, tsOptions)
                createWorked(kColl, tsOptions);
            },
            // TODO BACKPORT-19356 creation of the bucket used to not be idempotent. Re-enable this
            // test case in previous versions once the backport is completed.
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

        jsTest.log("Case collection: bucket timeseries / collection: bucket.");
        runTest(() => {
            createWorked(kBucket, tsOptions);
            createFailed(kBucket, {}, ErrorCodes.NamespaceExists);
        });

        jsTest.log("Case collection: bucket timeseries / collection: bucket timeseries.");
        runTest(
            () => {
                createWorked(kBucket, tsOptions);
                createWorked(kBucket, tsOptions);
            },
            // TODO BACKPORT-19356 creation of the bucket used to not be idempotent. Re-enable this
            // test case in previous versions once the backport is completed.
            "8.0"  // minRequiredVersion
        );
    }
}
