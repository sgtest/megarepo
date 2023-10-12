/**
 * indexed_insert_base.js
 *
 * Inserts multiple documents into an indexed collection. Asserts that all
 * documents appear in both a collection scan and an index scan. The indexed
 * value is the thread's id.
 */
export const $config = (function() {
    function makeSortSpecFromIndexSpec(ixSpec) {
        var sort = {};

        for (var field in ixSpec) {
            if (!ixSpec.hasOwnProperty(field)) {
                continue;
            }

            var order = ixSpec[field];
            if (order !== 1 && order !== -1) {  // e.g. '2d' or '2dsphere'
                order = 1;
            }

            sort[field] = order;
        }

        return sort;
    }

    var states = {
        init: function init(db, collName) {
            this.nInserted = 0;
            this.indexedValue = this.tid;
        },

        insert: function insert(db, collName) {
            var res = db[collName].insert(this.getDoc());
            assert.commandWorked(res);
            assert.eq(1, res.nInserted, tojson(res));
            this.nInserted += this.docsPerInsert;
        },

        find: function find(db, collName) {
            // collection scan
            var count = db[collName].find(this.getQuery()).sort({$natural: 1}).itcount();
            if (!this.skipAssertions) {
                assert.eq(count, this.nInserted);
            }

            // Use hint() to force an index scan, but only when an appropriate index exists.
            if (this.indexExists) {
                count = db[collName].find(this.getQuery()).hint(this.getIndexSpec()).itcount();
                if (!this.skipAssertions) {
                    assert.eq(count, this.nInserted);
                }
            }

            // Otherwise, impose a sort ordering over the collection scan
            else {
                // For single and compound-key indexes, the index specification is a
                // valid sort spec; however, for geospatial and text indexes it is not
                var sort = makeSortSpecFromIndexSpec(this.getIndexSpec());
                count = db[collName].find(this.getQuery()).sort(sort).itcount();
                if (!this.skipAssertions) {
                    assert.eq(count, this.nInserted);
                }
            }
        }
    };

    var transitions = {init: {insert: 1}, insert: {find: 1}, find: {insert: 1}};

    function setup(db, collName, cluster) {
        const spec = {name: this.getIndexName(), key: this.getIndexSpec()};
        assert.commandWorked(db.runCommand({
            createIndexes: collName,
            indexes: [spec],
            writeConcern: {w: 'majority'},
        }));
        this.indexExists = true;
    }

    return {
        threadCount: 20,
        iterations: 50,
        states: states,
        transitions: transitions,
        data: {
            getIndexName: function getIndexName() {
                return this.indexedField + '_1';
            },
            getIndexSpec: function getIndexSpec() {
                var ixSpec = {};
                ixSpec[this.indexedField] = 1;
                return ixSpec;
            },
            getDoc: function getDoc() {
                var doc = {};
                doc[this.indexedField] = this.indexedValue;
                return doc;
            },
            getQuery: function getQuery() {
                return this.getDoc();
            },
            indexedField: 'x',
            shardKey: {x: 1},
            docsPerInsert: 1,
            skipAssertions: false,
        },
        setup: setup
    };
})();
