/**
 * rename_collection_chain.js
 *
 * Creates a collection and then repeatedly executes the renameCollection
 * command against it. The previous "to" namespace is used as the next "from"
 * namespace.
 *
 * @tags: [
 *   assumes_unsharded_collection,
 * ]
 */
import {assertAlways, assertWhenOwnDB} from "jstests/concurrency/fsm_libs/assert.js";

export const $config = (function() {
    var data = {
        // Use the workload name as a prefix for the collection name,
        // since the workload name is assumed to be unique.
        prefix: 'rename_collection_chain'
    };

    var states = (function() {
        function uniqueCollectionName(prefix, tid, num) {
            return prefix + tid + '_' + num;
        }

        function init(db, collName) {
            this.fromCollName = uniqueCollectionName(this.prefix, this.tid, 0);
            this.num = 1;
            assertAlways.commandWorked(db.createCollection(this.fromCollName));
        }

        function rename(db, collName) {
            var toCollName = uniqueCollectionName(this.prefix, this.tid, this.num++);
            var res = db[this.fromCollName].renameCollection(toCollName, false /* dropTarget */);

            // SERVER-57128: NamespaceNotFound is an acceptable error if the mongos retries
            // the rename after the coordinator has already fulfilled the original request
            assertWhenOwnDB.commandWorkedOrFailedWithCode(res, ErrorCodes.NamespaceNotFound);

            this.fromCollName = toCollName;
        }

        function listCollections(db, collName) {
            const collectionInfos = db.getCollectionInfos();
            if (!this.allCollectionsInitialized) {
                if (collectionInfos.length === this.threadCount) {
                    this.allCollectionsInitialized = true;
                    jsTestLog(`All collections visible to thread ${this.tid}: ${
                        tojsononeline(collectionInfos)}`);
                }
            } else {
                const numColls =
                    collectionInfos.filter((collInfo) => collInfo.name.startsWith(this.prefix))
                        .length;
                assertAlways.eq(numColls, this.threadCount, () => tojson(collectionInfos));
            }
        }

        return {init: init, rename: rename, listCollections: listCollections};
    })();

    var transitions = {
        init: {rename: 1},
        rename: {rename: 0.9, listCollections: 0.1},
        listCollections: {rename: 1},
    };

    return {
        threadCount: 10,
        iterations: 20,
        data: data,
        states: states,
        transitions: transitions,
    };
})();
