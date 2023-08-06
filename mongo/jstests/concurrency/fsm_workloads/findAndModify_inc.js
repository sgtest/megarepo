/**
 * findAndModify_inc.js
 *
 * Inserts a single document into a collection. Each thread performs a
 * findAndModify command to select the document and increment a particular
 * field. Asserts that the field has the correct value based on the number
 * of increments performed.
 *
 * This workload was designed to reproduce SERVER-15892.
 */

import {assertAlways, assertWhenOwnColl} from "jstests/concurrency/fsm_libs/assert.js";
import {isMongod} from "jstests/concurrency/fsm_workload_helpers/server_types.js";

export const $config = (function() {
    let data = {
        getUpdateArgument: function(fieldName) {
            return {$inc: {[fieldName]: 1}};
        },
    };

    var states = {

        init: function init(db, collName) {
            this.fieldName = 't' + this.tid;
            this.count = 0;
        },

        update: function update(db, collName) {
            var updateDoc = this.getUpdateArgument(this.fieldName);

            var res = db.runCommand(
                {findAndModify: collName, query: {_id: 'findAndModify_inc'}, update: updateDoc});
            assertAlways.commandWorked(res);

            // If the document was invalidated during a yield, then we wouldn't have modified it.
            // The "findAndModify" command returns a null value in this case. See SERVER-22002 for
            // more details.
            if (isMongod(db)) {
                // If the document is modified by another thread during a yield, then the operation
                // is retried internally. We never expect to see a null value returned by the
                // "findAndModify" command when it is known that a matching document exists in the
                // collection.
                assertWhenOwnColl(res.value !== null, 'query spec should have matched a document');
            }

            if (res.value !== null) {
                ++this.count;
            }
        },

        find: function find(db, collName) {
            var docs = db[collName].find().toArray();
            assertWhenOwnColl.eq(1, docs.length);
            assertWhenOwnColl(() => {
                var doc = docs[0];
                if (doc.hasOwnProperty(this.fieldName)) {
                    assertWhenOwnColl.eq(this.count, doc[this.fieldName]);
                } else {
                    assertWhenOwnColl.eq(this.count, 0);
                }
            });
        }

    };

    var transitions = {init: {update: 1}, update: {find: 1}, find: {update: 1}};

    function setup(db, collName, cluster) {
        const doc = {_id: 'findAndModify_inc'};
        // Initialize the fields used to a count of 0.
        for (let i = 0; i < this.threadCount; ++i) {
            doc['t' + i] = 0;
        }
        db[collName].insert(doc);
    }

    return {
        threadCount: 20,
        iterations: 20,
        data: data,
        states: states,
        transitions: transitions,
        setup: setup
    };
})();
