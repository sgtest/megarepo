/**
 * Test use of computed fields in aggregations on time series collections.
 * @tags: [
 *   requires_timeseries,
 *   does_not_support_stepdowns,
 *   directly_against_shardsvrs_incompatible,
 *   # During fcv upgrade/downgrade the engine might not be what we expect.
 *   cannot_run_during_upgrade_downgrade,
 *   # "Explain of a resolved view must be executed by mongos"
 *   directly_against_shardsvrs_incompatible,
 *   # Some suites use mixed-binary cluster setup where some nodes might have the flag enabled while
 *   # others -- not. For this test we need control over whether the flag is set on the node that
 *   # ends up executing the query.
 *   assumes_standalone_mongod
 * ]
 */

import {TimeseriesTest} from "jstests/core/timeseries/libs/timeseries.js";

TimeseriesTest.run((insert) => {
    const datePrefix = 1680912440;

    let coll = db.timeseries_computed_field;
    const bucketsColl = db.getCollection('system.buckets.' + coll.getName());

    const timeFieldName = 'time';
    const metaFieldName = 'measurement';

    coll.drop();
    assert.commandWorked(db.createCollection(coll.getName(), {
        timeseries: {timeField: timeFieldName, metaField: metaFieldName},
    }));
    assert.contains(bucketsColl.getName(), db.getCollectionNames());

    insert(coll, {
        _id: 0,
        [timeFieldName]: new Date(datePrefix + 100),
        [metaFieldName]: "cpu",
        topLevelScalar: 123,
        topLevelArray: [1, 2, 3, 4],
        arrOfObj: [{x: 1}, {x: 2}, {x: 3}, {x: 4}],
        obj: {a: 123},
    });
    insert(coll, {
        _id: 1,
        [timeFieldName]: new Date(datePrefix + 200),
        [metaFieldName]: "cpu",
        topLevelScalar: 456,
        topLevelArray: [101, 102, 103, 104],
        arrOfObj: [{x: 101}, {x: 102}, {x: 103}, {x: 104}],
        obj: {a: 456},
    });
    // Insert a document that will be placed in a different bucket.
    insert(coll, {
        _id: 2,
        [timeFieldName]: new Date(datePrefix + 300),
        [metaFieldName]: "gpu",
    })

    // Computing a field on a dotted path which is an array, then grouping on it. Note that the
    // semantics for setting a computed field on a dotted array path are particularly strange, but
    // should be preserved for backwards compatibility.
    {
        const res = coll.aggregate([
                            {$addFields: {"arrOfObj.x": {$trim: {input: "test string"}}}},
                            {$match: {topLevelScalar: {$gte: 0}}},
                            {$group: {_id: null, max: {$max: "$arrOfObj.x"}}}
                        ])
                        .toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].max, ["test string", "test string", "test string", "test string"], res);
    }

    {
        const res = coll.aggregate([
                            {$addFields: {"arrOfObj.x": {$add: ["$topLevelScalar", 1]}}},
                            {$match: {topLevelScalar: {$gte: 0}}},
                            {$group: {_id: null, max: {$max: "$arrOfObj.x"}}}
                        ])
                        .toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].max, [457, 457, 457, 457], res);
    }

    // Computing a field and then filtering by it.
    {
        const res = coll.aggregate([
                            {$addFields: {"arrOfObj.x": {$trim: {input: "test string"}}}},
                            {$match: {"arrOfObj.x": "test string"}},
                            {$group: {_id: null, max: {$max: "$arrOfObj.x"}}}
                        ])
                        .toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].max, ["test string", "test string", "test string", "test string"], res);
    }

    {
        // Computing a field based on a dotted path which does not traverse arrays.
        const res = coll.aggregate([
                            {$addFields: {"computedA": {$add: ["$obj.a", 1]}}},
                            // Only one document should have a value where obj.a was 457
                            // (456 + 1).
                            {$match: {"computedA": 457}},
                            {$count: "count"}
                        ])
                        .toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].count, 1, res);
    }

    {
        // Include an unnecessary computed field before counting the number of documents.
        const res = coll.aggregate([{$addFields: {"a": 1}}, {$count: "count"}]).toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].count, 3, res);
    }

    // mathematical expressions
    {
        let pipeline = [
            {$addFields: {"computedA": {$add: ["$topLevelScalar", 1]}}},
            {$match: {"computedA": 457}},
            {$count: "count"}
        ];

        const res = coll.aggregate(pipeline).toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].count, 1, res);
    }
    {
        let pipeline = [
            {$addFields: {"computedA": {$subtract: ["$topLevelScalar", 1]}}},
            {$match: {"computedA": 455}},
            {$count: "count"}
        ];

        const res = coll.aggregate(pipeline).toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].count, 1, res);
    }
    {
        let pipeline = [
            {$addFields: {"computedA": {$multiply: ["$topLevelScalar", 10]}}},
            {$match: {"computedA": 4560}},
            {$count: "count"}
        ];

        const res = coll.aggregate(pipeline).toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].count, 1, res);
    }
    {
        let pipeline = [
            {$addFields: {"computedA": {$divide: ["$topLevelScalar", 2]}}},
            {$match: {"computedA": 228}},
            {$count: "count"}
        ];

        const res = coll.aggregate(pipeline).toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].count, 1, res);
    }
    {
        let pipeline = [
            {$addFields: {"computedA": {$add: [1, "$topLevelScalar"]}}},
            {$match: {"computedA": 457}},
            {$count: "count"}
        ];

        const res = coll.aggregate(pipeline).toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].count, 1, res);
    }
    {
        let pipeline = [
            {$addFields: {"computedA": {$subtract: [200, "$topLevelScalar"]}}},
            {$match: {"computedA": 77}},
            {$count: "count"}
        ];

        const res = coll.aggregate(pipeline).toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].count, 1, res);
    }
    {
        let pipeline = [
            {$addFields: {"computedA": {$multiply: [10, "$topLevelScalar"]}}},
            {$match: {"computedA": 4560}},
            {$count: "count"}
        ];

        const res = coll.aggregate(pipeline).toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].count, 1, res);
    }
    {
        let pipeline = [
            {$addFields: {"computedA": {$divide: [4560, "$topLevelScalar"]}}},
            {$match: {"computedA": 10}},
            {$count: "count"}
        ];

        const res = coll.aggregate(pipeline).toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].count, 1, res);
    }

    {
        let pipeline = [
            {$addFields: {"computedA": {$add: ["$topLevelScalar", "$topLevelScalar"]}}},
            {$match: {"computedA": 912}},
            {$count: "count"}
        ];

        const res = coll.aggregate(pipeline).toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].count, 1, res);
    }
    {
        let pipeline = [
            {$addFields: {"computedA": {$subtract: ["$topLevelScalar", "$topLevelScalar"]}}},
            {$match: {"computedA": 0}},
            {$count: "count"}
        ];

        const res = coll.aggregate(pipeline).toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].count, 2, res);
    }
    {
        let pipeline = [
            {$addFields: {"computedA": {$multiply: ["$topLevelScalar", "$topLevelScalar"]}}},
            {$match: {"computedA": 15129}},
            {$count: "count"}
        ];

        const res = coll.aggregate(pipeline).toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].count, 1, res);
    }
    {
        let pipeline = [
            {$addFields: {"computedA": {$divide: ["$topLevelScalar", "$topLevelScalar"]}}},
            {$match: {"computedA": 1}},
            {$count: "count"}
        ];

        const res = coll.aggregate(pipeline).toArray();
        assert.eq(res.length, 1, res);
        assert.eq(res[0].count, 2, res);
    }

    {
        // Try a project stage which adds and remove subfields.
        const res = coll.aggregate([
                            {$project: {"obj.newField": "$topLevelScalar"}},
                            {$project: {"_id": 0, "obj.a": 0}}
                        ])
                        .toArray();
        assert.eq(res.length, 3, res);
        assert.eq(res[0], {"obj": {"newField": 123}}, res);
        assert.eq(res[1], {"obj": {"newField": 456}}, res);
        assert.eq(res[2], {"obj": {}}, res);
    }
});
