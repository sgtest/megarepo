/**
 * drop_database.js
 *
 * Repeatedly creates and drops a database.
 *
 * @tags: [
 * ]
 */
export const $config = (function() {
    var states = {
        init: function init(db, collName) {
            this.uniqueDBName = db.getName() + 'drop_database' + this.tid;
        },

        createAndDrop: function createAndDrop(db, collName) {
            // TODO: should we ever do something different?
            //       e.g. create multiple collections on the database and then drop?
            var myDB = db.getSiblingDB(this.uniqueDBName);
            assertAlways.commandWorked(myDB.createCollection(collName));

            assertAlways.commandWorked(myDB.dropDatabase());
        }
    };

    var transitions = {init: {createAndDrop: 1}, createAndDrop: {createAndDrop: 1}};

    return {threadCount: 10, iterations: 20, states: states, transitions: transitions};
})();
