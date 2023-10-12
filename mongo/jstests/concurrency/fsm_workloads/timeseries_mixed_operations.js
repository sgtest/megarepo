/**
 * Runs operations against a time-series view and bucket collection simultaneously.
 *
 * @tags: [
 *   assumes_no_implicit_collection_creation_after_drop,
 *   does_not_support_stepdowns,
 *   requires_timeseries,
 * ]
 */

export const $config = (function() {
    const initData = {
        getCollectionName: function(collName) {
            return "timeseries_mixed_operations_" + collName;
        },

        getCollection: function(db, collName) {
            return db.getCollection(this.getCollectionName(collName));
        },

        getBucketCollection: function(db, collName) {
            return db.getCollection("system.buckets." + this.getCollectionName(collName));
        },
    };

    const timeFieldName = "time";
    const metaFieldName = "meta";
    const batchSize = 10;

    const states = {
        init: function init(db, collName) {
            const coll = this.getCollection(db, collName);
            const res = coll.insert({
                [metaFieldName]: 1,
                [timeFieldName]: new Date(),
                first: true,
            });
            assert.commandWorked(res);
            assert.eq(1, res.nInserted, tojson(res));
        },

        insertManyOrdered: function insertManyOrdered(db, collName) {
            const coll = this.getCollection(db, collName);
            const docs = [];
            for (let i = 0; i < batchSize; i++) {
                docs.push({
                    [metaFieldName]: Random.randInt(this.threadCount),
                    [timeFieldName]: new Date(),
                    data: Random.rand(),
                });
            }
            const res = coll.insertMany(docs, {ordered: true});
            assert.commandWorked(res);
            assert.eq(res.insertedIds.length, batchSize);
        },

        insertManyUnordered: function insertManyUnordered(db, collName) {
            const coll = this.getCollection(db, collName);
            const docs = [];
            for (let i = 0; i < batchSize; i++) {
                docs.push({
                    [metaFieldName]: Random.randInt(this.threadCount),
                    [timeFieldName]: new Date(),
                    data: Random.rand(),
                });
            }
            const res = coll.insertMany(docs, {ordered: false});
            assert.commandWorked(res);
            assert.eq(res.insertedIds.length, batchSize);
        },

        deleteAllBuckets: function deleteAllBuckets(db, collName) {
            assert.commandWorked(this.getBucketCollection(db, collName).remove({}));
        },
    };

    function setup(db, collName, cluster) {
        collName = this.getCollectionName(collName);
        assert.commandWorked(db.createCollection(collName, {
            timeseries: {
                timeField: timeFieldName,
                metaField: metaFieldName,
            }
        }));
    }

    const standardTransition = {
        insertManyOrdered: 0.4,
        insertManyUnordered: 0.4,
        deleteAllBuckets: 0.2,
    };

    const transitions = {
        init: standardTransition,
        insertManyOrdered: standardTransition,
        insertManyUnordered: standardTransition,
        deleteAllBuckets: standardTransition,
    };

    return {
        threadCount: 10,
        iterations: 500,
        startState: 'init',
        states: states,
        data: initData,
        transitions: transitions,
        setup: setup,
    };
})();