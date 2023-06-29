/**
 * map_reduce_drop.js
 *
 * This workload generates random data and inserts it into a collection.
 * It then runs simultaneous mapReduce commands while dropping the source
 * collection or source database.  It repopulates the data before each
 * mapReduce in an attempt to ensure that the mapReduce commands are
 * actually doing work.
 *
 * This workload serves as a regression test for SERVER-6757, SERVER-15087,
 * and SERVER-15842.
 * @tags: [
 *   # mapReduce does not support afterClusterTime.
 *   does_not_support_causal_consistency,
 * ]
 */
export const $config = (function() {
    var data = {
        mapper: function mapper() {
            emit(this.key, 1);
        },
        reducer: function reducer() {
            // This dummy reducer is present to enable the database and collection
            // drops to occur during different phases of the mapReduce.
            return 1;
        },
        numDocs: 250
    };

    var states = (function() {
        function dropColl(db, collName) {
            var mapReduceDb = db.getSiblingDB(this.mapReduceDBName);

            // We don't check the return value of drop() because the collection
            // might not exist due to a drop() in another thread.
            mapReduceDb[collName].drop();
        }

        function dropDB(db, collName) {
            var mapReduceDb = db.getSiblingDB(this.mapReduceDBName);

            // Concurrent dropDatabase calls can result in transient errors.
            mapReduceDb.dropDatabase();
        }

        function mapReduce(db, collName) {
            var mapReduceDb = db.getSiblingDB(this.mapReduceDBName);

            // Try to ensure that some documents have been inserted before running
            // the mapReduce command.  Although it's possible for the documents to
            // be dropped by another thread, some mapReduce commands should end up
            // running on non-empty collections by virtue of the number of
            // iterations and threads in this workload.
            try {
                var bulk = mapReduceDb[collName].initializeUnorderedBulkOp();
                for (var i = 0; i < this.numDocs; ++i) {
                    bulk.insert({key: Random.randInt(10000)});
                }
                var res = bulk.execute();
                assertAlways.commandWorked(res);
            } catch (ex) {
                assert.eq(true, ex instanceof BulkWriteError, tojson(ex));
                assert.writeErrorWithCode(ex, ErrorCodes.DatabaseDropPending);
            }

            var options = {
                finalize: function finalize(key, reducedValue) {
                    return reducedValue;
                },
                out: collName + '_out'
            };

            try {
                mapReduceDb[collName].mapReduce(this.mapper, this.reducer, options);
            } catch (e) {
                // Ignore all mapReduce exceptions.  This workload is only concerned
                // with verifying server availability.
            }
        }

        return {dropColl: dropColl, dropDB: dropDB, mapReduce: mapReduce};
    })();

    var transitions = {
        dropColl: {mapReduce: 1},
        dropDB: {mapReduce: 1},
        mapReduce: {mapReduce: 0.7, dropDB: 0.05, dropColl: 0.25}
    };

    function setup(db, collName, cluster) {
        this.mapReduceDBName = db.getName() + 'map_reduce_drop';
    }

    return {
        threadCount: 5,
        iterations: 10,
        data: data,
        setup: setup,
        states: states,
        startState: 'mapReduce',
        transitions: transitions,
    };
})();
