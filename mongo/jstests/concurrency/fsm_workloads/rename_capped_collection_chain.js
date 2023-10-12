/**
 * rename_capped_collection_chain.js
 *
 * Creates a capped collection and then repeatedly executes the renameCollection
 * command against it. The previous "to" namespace is used as the next "from"
 * namespace.
 *
 * @tags: [requires_capped]
 */
export const $config = (function() {
    var data = {
        // Use the workload name as a prefix for the collection name,
        // since the workload name is assumed to be unique.
        prefix: 'rename_capped_collection_chain'
    };

    var states = (function() {
        function uniqueCollectionName(prefix, tid, num) {
            return prefix + tid + '_' + num;
        }

        function init(db, collName) {
            this.fromCollName = uniqueCollectionName(this.prefix, this.tid, 0);
            this.num = 1;

            var options = {capped: true, size: 4096};

            assert.commandWorked(db.createCollection(this.fromCollName, options));
            assert(db[this.fromCollName].isCapped());
        }

        function rename(db, collName) {
            var toCollName = uniqueCollectionName(this.prefix, this.tid, this.num++);
            var res = db[this.fromCollName].renameCollection(toCollName, false /* dropTarget */);

            // SERVER-57128: NamespaceNotFound is an acceptable error if the mongos retries
            // the rename after the coordinator has already fulfilled the original request
            assert.commandWorkedOrFailedWithCode(res, ErrorCodes.NamespaceNotFound);

            assert(db[toCollName].isCapped());
            this.fromCollName = toCollName;
        }

        return {init: init, rename: rename};
    })();

    var transitions = {init: {rename: 1}, rename: {rename: 1}};

    return {
        threadCount: 10,
        iterations: 20,
        data: data,
        states: states,
        transitions: transitions,
    };
})();
