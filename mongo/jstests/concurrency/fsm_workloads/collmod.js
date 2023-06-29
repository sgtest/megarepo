/**
 * collmod.js
 *
 * Base workload for collMod command.
 * Generates some random data and inserts it into a collection with a
 * TTL index. Runs a collMod command to change the value of the
 * expireAfterSeconds setting to a random integer.
 *
 * All threads update the same TTL index on the same collection.
 */
export const $config = (function() {
    var data = {
        numDocs: 1000,
        maxTTL: 5000  // max time to live
    };

    var states = (function() {
        function collMod(db, collName) {
            var newTTL = Random.randInt(this.maxTTL);
            var res = db.runCommand({
                collMod: this.threadCollName,
                index: {keyPattern: {createdAt: 1}, expireAfterSeconds: newTTL}
            });
            assertAlways.commandWorkedOrFailedWithCode(res,
                                                       [ErrorCodes.ConflictingOperationInProgress]);
            // only assert if new expireAfterSeconds differs from old one
            if (res.ok === 1 && res.hasOwnProperty('expireAfterSeconds_new')) {
                assertWhenOwnDB.eq(res.expireAfterSeconds_new, newTTL);
            }

            // Attempt an invalid collMod which should always fail regardless of whether a WCE
            // occurred. This is meant to reproduce SERVER-56772.
            const encryptSchema = {$jsonSchema: {properties: {_id: {encrypt: {}}}}};
            assertAlways.commandFailedWithCode(
                db.runCommand({
                    collMod: this.threadCollName,
                    validator: encryptSchema,
                    validationAction: "warn"
                }),
                [ErrorCodes.ConflictingOperationInProgress, ErrorCodes.QueryFeatureNotAllowed]);
        }

        return {collMod: collMod};
    })();

    var transitions = {collMod: {collMod: 1}};

    function setup(db, collName, cluster) {
        // other workloads that extend this one might have set 'this.threadCollName'
        this.threadCollName = this.threadCollName || collName;
        var bulk = db[this.threadCollName].initializeUnorderedBulkOp();
        for (var i = 0; i < this.numDocs; ++i) {
            bulk.insert({createdAt: new Date()});
        }

        var res = bulk.execute();
        assertAlways.commandWorked(res);
        assertAlways.eq(this.numDocs, res.nInserted);

        // create TTL index
        res = db[this.threadCollName].createIndex({createdAt: 1}, {expireAfterSeconds: 3600});
        assertAlways.commandWorked(res);
    }

    return {
        threadCount: 10,
        iterations: 20,
        data: data,
        startState: 'collMod',
        states: states,
        transitions: transitions,
        setup: setup
    };
})();
