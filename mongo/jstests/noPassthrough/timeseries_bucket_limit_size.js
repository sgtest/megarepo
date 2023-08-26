/**
 * Tests maximum size of measurements held in each bucket in a time-series buckets collection.
 * @tags: [
 *   does_not_support_stepdowns,
 *   does_not_support_transactions,
 *   tenant_migration_incompatible,
 *   requires_fcv_61,
 * ]
 */
import {TimeseriesTest} from "jstests/core/timeseries/libs/timeseries.js";

const conn = MongoRunner.runMongod({setParameter: {timeseriesBucketMinCount: 1}});

const dbName = jsTestName();
const db = conn.getDB(dbName);

TimeseriesTest.run((insert) => {
    const collNamePrefix = 'timeseries_bucket_limit_size_';

    const timeFieldName = 'time';

    // Assumes each bucket has a limit of 125kB on the measurements stored in the 'data' field.
    const bucketMaxSizeKB = 125;
    const numDocs = 3;

    // The measurement data should not take up all of the 'bucketMaxSizeKB' limit because we need to
    // leave room for the control.min and control.max summaries (two measurements worth of data). We
    // need to fit two measurements within this limit to trigger compression if enabled.
    const largeValue = 'x'.repeat(((bucketMaxSizeKB - 1) / 4) * 1024);

    const runTest = function(numDocsPerInsert) {
        const coll = db.getCollection(collNamePrefix + numDocsPerInsert);
        const bucketsColl = db.getCollection('system.buckets.' + coll.getName());
        coll.drop();

        assert.commandWorked(
            db.createCollection(coll.getName(), {timeseries: {timeField: timeFieldName}}));
        assert.contains(bucketsColl.getName(), db.getCollectionNames());

        let docs = [];
        for (let i = 0; i < numDocs; i++) {
            docs.push({_id: i, [timeFieldName]: ISODate(), x: largeValue});
            if ((i + 1) % numDocsPerInsert === 0) {
                assert.commandWorked(insert(coll, docs), 'failed to insert docs: ' + tojson(docs));
                docs = [];
            }
        }

        // Check view.
        const viewDocs = coll.find({}, {x: 1}).sort({_id: 1}).toArray();
        assert.eq(numDocs, viewDocs.length, viewDocs);
        for (let i = 0; i < numDocs; i++) {
            const viewDoc = viewDocs[i];
            assert.eq(i, viewDoc._id, 'unexpected _id in doc: ' + i + ': ' + tojson(viewDoc));
            assert.eq(
                largeValue, viewDoc.x, 'unexpected field x in doc: ' + i + ': ' + tojson(viewDoc));
        }

        // Check bucket collection.
        const bucketDocs = bucketsColl.find().sort({'control.min._id': 1}).toArray();
        assert.eq(2, bucketDocs.length, bucketDocs);

        // Check both buckets.
        // First bucket should be full with two documents since we spill the third document over
        // into the second bucket due to size constraints on 'data'.
        assert.eq(0,
                  bucketDocs[0].control.min._id,
                  'invalid control.min for _id in first bucket: ' + tojson(bucketDocs[0].control));
        assert.eq(largeValue,
                  bucketDocs[0].control.min.x,
                  'invalid control.min for x in first bucket: ' + tojson(bucketDocs[0].control));
        assert.eq(1,
                  bucketDocs[0].control.max._id,
                  'invalid control.max for _id in first bucket: ' + tojson(bucketDocs[0].control));
        assert.eq(largeValue,
                  bucketDocs[0].control.max.x,
                  'invalid control.max for x in first bucket: ' + tojson(bucketDocs[0].control));
        // Version 2 indicates the bucket is compressed.
        assert.eq(2,
                  bucketDocs[0].control.version,
                  'unexpected control.version in first bucket: ' + tojson(bucketDocs));

        assert(!bucketDocs[0].control.hasOwnProperty("closed"),
               'unexpected control.closed in first bucket: ' + tojson(bucketDocs));

        // Second bucket should contain the remaining document.
        assert.eq(numDocs - 1,
                  bucketDocs[1].control.min._id,
                  'invalid control.min for _id in second bucket: ' + tojson(bucketDocs[1].control));
        assert.eq(largeValue,
                  bucketDocs[1].control.min.x,
                  'invalid control.min for x in second bucket: ' + tojson(bucketDocs[1].control));
        assert.eq(numDocs - 1,
                  bucketDocs[1].control.max._id,
                  'invalid control.max for _id in second bucket: ' + tojson(bucketDocs[1].control));
        assert.eq(largeValue,
                  bucketDocs[1].control.max.x,
                  'invalid control.max for x in second bucket: ' + tojson(bucketDocs[1].control));
        // Version 1 indicates the bucket is uncompressed, and version 2 indicates the bucket is
        // compressed.
        assert.eq(TimeseriesTest.timeseriesAlwaysUseCompressedBucketsEnabled(db) ? 2 : 1,
                  bucketDocs[1].control.version,
                  'unexpected control.version in second bucket: ' + tojson(bucketDocs));

        assert(!bucketDocs[1].control.hasOwnProperty("closed"),
               'unexpected control.closed in second bucket: ' + tojson(bucketDocs));
    };

    runTest(1);
    runTest(numDocs);
});

MongoRunner.stopMongod(conn);
