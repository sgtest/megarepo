'use strict';

/**
 * update_multifield_multiupdate.js
 *
 * Does updates that affect multiple fields on multiple documents.
 * The collection has an index for each field, and a multikey index for all fields.
 */
load('jstests/concurrency/fsm_libs/extend_workload.js');         // for extendWorkload
load('jstests/concurrency/fsm_workloads/update_multifield.js');  // for $config

// For isMongod
load('jstests/concurrency/fsm_workload_helpers/server_types.js');

var $config = extendWorkload($config,
                             function($config, $super) {
                                 $config.data.multi = true;

                                 $config.data.assertResult = function(res, db, collName, query) {
                                     assertAlways.eq(0, res.nUpserted, tojson(res));

                                     if (isMongod(db)) {
                                         // If a document's RecordId cannot change, then we should
                                         // not have updated any document more than once, since the
                                         // update stage internally de-duplicates based on RecordId.
                                         assertWhenOwnColl.lte(
                                             this.numDocs, res.nMatched, tojson(res));
                                     } else {  // mongos
                                         assertAlways.gte(res.nMatched, 0, tojson(res));
                                     }

                                     assertWhenOwnColl.eq(res.nMatched, res.nModified, tojson(res));

                                     if (TestData.runningWithBalancer !== true) {
                                         var docs = db[collName].find().toArray();
                                         docs
                                             .forEach(
                                                 function(doc) {
                                                     assertWhenOwnColl.eq(
                    'number',
                    typeof doc.z,
                    `The query is ${tojson(query)}, and doc is ${
                        tojson(doc)}, the number of all docs is ${
                        docs.length}. The response of update is ${tojson(res)}, and config multi is ${
                        $config.data.multi.toString()}`);
                                                     assertWhenOwnColl.gt(doc.z, 0);
                                                 });
                                     }
                                 };

                                 return $config;
                             });
