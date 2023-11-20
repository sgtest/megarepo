/**
 * Tests that the row store expression is skipped when there is an appropriate group or projection
 * above a columnscan stage.
 *
 * @tags: [
 *   # Column store indexes are still under a feature flag.
 *   featureFlagColumnstoreIndexes,
 *   # explain is not supported in transactions
 *   does_not_support_transactions,
 *   requires_pipeline_optimization,
 *   # Runs explain on an aggregate command which is only compatible with readConcern local.
 *   assumes_read_concern_unchanged,
 *   # explain will be different in a sharded collection
 *   assumes_unsharded_collection,
 *   # Columnstore tests set server parameters to disable columnstore query planning heuristics -
 *   # 1) server parameters are stored in-memory only so are not transferred onto the recipient,
 *   # 2) server parameters may not be set in stepdown passthroughs because it is a command that may
 *   #      return different values after a failover
 *   tenant_migration_incompatible,
 *   does_not_support_stepdowns,
 *   not_allowed_with_security_token,
 * ]
 */
import {assertArrayEq} from "jstests/aggregation/extras/utils.js";
import {getSingleNodeExplain} from "jstests/libs/analyze_plan.js";
import {setUpServerForColumnStoreIndexTest} from "jstests/libs/columnstore_util.js";
import {checkSBEEnabled} from "jstests/libs/sbe_util.js";

const columnstoreEnabled =
    checkSBEEnabled(db, ["featureFlagColumnstoreIndexes"], true /* checkAllNodes */);
if (!columnstoreEnabled) {
    jsTestLog("Skipping columnstore index test since the feature flag is not enabled.");
    quit();
}

if (!setUpServerForColumnStoreIndexTest(db)) {
    quit();
}

const indexedColl = db.column_scan_skip_row_store_projection_indexed;
const unindexedColl = db.column_scan_skip_row_store_projection_unindexed;

function setupCollections() {
    indexedColl.drop();
    unindexedColl.drop();
    assert.commandWorked(indexedColl.createIndex({"$**": "columnstore"}));

    const docs = [
        {_id: "a_number", a: 4},
        {_id: "a_subobject_c_not_null", a: {c: "hi"}},
        {_id: "a_subobject_c_null", a: {c: null}},
        {_id: "a_subobject_c_undefined", a: {c: undefined}},
        {_id: "no_a", b: 1},
        {_id: "a_and_b_nested", a: 2, b: {d: 1}},
        {_id: "a_nested_and_b_nested", a: {c: 5}, b: {d: {f: 2}}, e: 1},
    ];
    assert.commandWorked(indexedColl.insertMany(docs));
    assert.commandWorked(unindexedColl.insertMany(docs));
}

function test({agg, requiresRowStoreExpr, requiredRowstoreReads}) {
    // Check that columnstore index is used, and we skip the row store expression appropriately.
    const explainPlan = getSingleNodeExplain(indexedColl.explain("queryPlanner").aggregate(agg));
    let sbeStages = ('queryPlanner' in explainPlan)
        // entirely SBE plan
        ? explainPlan.queryPlanner.winningPlan.slotBasedPlan.stages
        // SBE + classic plan
        : explainPlan.stages[0]["$cursor"].queryPlanner.winningPlan.slotBasedPlan.stages;
    assert(sbeStages.includes('columnscan'), `No columnscan in SBE stages: ${sbeStages}`);
    const nullRegex =
        /columnscan s.* ((s.*)|(none)) paths\[.*\] pathFilters\[.*\] rowStoreExpr\[\] @.* @.*/;
    const notNullRegex =
        /columnscan s.* ((s.*)|(none)) paths\[.*\] pathFilters\[.*\] rowStoreExpr\[((.*, \n)|(.+makeBsonObj.*))/;
    if (requiresRowStoreExpr) {
        assert(!nullRegex.test(sbeStages), `Don't expect null rowstoreExpr in ${sbeStages}`);
        assert(notNullRegex.test(sbeStages), `Expected non-null rowstoreExpr in ${sbeStages}`);
    } else {
        assert(nullRegex.test(sbeStages), `Expected null rowStoreExpr in ${sbeStages}`);
        assert(!notNullRegex.test(sbeStages), `Don't expect non-null rowStoreExpr in ${sbeStages}`);
    }

    // Check the expected number of row store reads. The reads are triggered by encountering a
    // record that cannot be reconstructed from the index and come in the form of a fetch followed
    // by a few records scanned from the row store. The number of scanned records fluctuates
    // depending on the settings and the data patterns so the only invariant we can assert is that
    // the number of combined reads from the row store is at least as the number of "bad" records.
    const explainExec = indexedColl.explain("executionStats").aggregate(agg);
    const actualRowstoreReads =
        parseInt(JSON.stringify(explainExec).split('"numRowStoreFetches":')[1].split(",")[0]) +
        parseInt(JSON.stringify(explainExec).split('"numRowStoreScans":')[1].split(",")[0]);
    assert.gte(
        actualRowstoreReads,
        requiredRowstoreReads,
        `Unexpected nubmer of row store fetches in ${JSON.stringify(explainExec, null, '\t')}`);

    // Check that results are identical with and without columnstore index.
    assertArrayEq({
        actual: indexedColl.aggregate(agg).toArray(),
        expected: unindexedColl.aggregate(agg).toArray()
    });
}

function runAllAggregations() {
    // $project only.  Requires row store expression regardless of nesting under the projected path.
    test({agg: [{$project: {_id: 0, a: 1}}], requiresRowStoreExpr: true, requiredRowstoreReads: 4});
    test({agg: [{$project: {_id: 0, b: 1}}], requiresRowStoreExpr: true, requiredRowstoreReads: 2});

    // $group only.
    // The 4 cases below provide the same coverage but illustrate when row store fetches are needed.
    test({
        agg: [{$group: {_id: null, a: {$push: "$a"}}}],
        requiresRowStoreExpr: false,
        requiredRowstoreReads: 4
    });
    test({
        agg: [{$group: {_id: null, b: {$push: "$b"}}}],
        requiresRowStoreExpr: false,
        requiredRowstoreReads: 2
    });
    test({
        agg: [{$group: {_id: null, e: {$push: "$e"}}}],
        requiresRowStoreExpr: false,
        requiredRowstoreReads: 0
    });
    test({
        agg: [{$group: {_id: "$_id", a: {$push: "$a"}, b: {$push: "$b"}}}],
        requiresRowStoreExpr: false,
        requiredRowstoreReads: 5
    });

    // $group and $project, including _id.
    test({
        agg: [{$project: {_id: 1, a: 1}}, {$group: {_id: "$_id", a: {$push: "$a"}}}],
        requiresRowStoreExpr: false,
        requiredRowstoreReads: 4
    });

    // The rowStoreExpr is needed to prevent the $group from seeing b.
    test({
        agg: [
            {$project: {_id: 1, a: 1}},
            {$group: {_id: "$_id", a: {$push: "$a"}, b: {$push: "$b"}}}
        ],
        requiresRowStoreExpr: true,
        requiredRowstoreReads: 4
    });

    // Same as above, but add another $group later that would be eligible for skipping the row store
    // expression.
    test({
        agg: [
            {$project: {_id: 1, a: 1}},
            {$group: {_id: "$_id", a: {$push: "$a"}, b: {$push: "$b"}}},
            {$project: {_id: 1, a: 1}},
            {$group: {_id: "$_id", a: {$push: "$a"}}}
        ],
        requiresRowStoreExpr: true,
        requiredRowstoreReads: 4
    });

    // $group and $project, excluding _id.
    // Because _id is projected out, the $group will aggregate all docs together.  The rowStoreExpr
    // must not be skipped or else $group will behave incorrectly.
    test({
        agg: [{$project: {_id: 0, a: 1}}, {$group: {_id: "$_id", a: {$push: "$a"}}}],
        requiresRowStoreExpr: true,
        requiredRowstoreReads: 4
    });

    // $match with a filter that can be pushed down.
    test({
        agg: [{$match: {a: 2}}, {$group: {_id: "$_id", b: {$push: "$b"}, a: {$push: "$a"}}}],
        requiresRowStoreExpr: false,
        requiredRowstoreReads: 1
    });

    // $match with no group, and non-output filter that can't be pushed down.
    test({
        agg: [{$match: {e: {$exists: false}}}, {$project: {_id: 1, b: 1}}],
        requiresRowStoreExpr: false,
        requiredRowstoreReads: 2
    });
    // $match with no group, and non-output filter that can be pushed down.
    test({
        agg: [{$match: {e: {$exists: true}}}, {$project: {_id: 1, b: 1}}],
        requiresRowStoreExpr: true,
        requiredRowstoreReads: 1
    });

    // $project inclusion followed by a $addFields which can be pushed into SBE should
    // require a row store expression.
    test({
        agg: [{$project: {a: 1}}, {$addFields: {newField: 999}}],
        requiresRowStoreExpr: true,
        requiredRowstoreReads: 1
    });

    // Nested paths.
    // The BrowserUsageByDistinctUserQuery that motivated this ticket is an example of this.
    test({
        agg: [{$match: {"a.c": 5}}, {$group: {_id: "$_id", b_d: {$push: "$b.d"}}}],
        requiresRowStoreExpr: false,
        requiredRowstoreReads: 1
    });

    // BrowserUsageByDistinctUserQuery from ColumnStoreIndex.yml in the genny repo.
    // $addFields is not implemented in SBE, so this will have an SBE plan + an agg pipeline.
    // This query does not match our documents, but the test checks for row store expression
    // elimination.
    test({
        agg: [
            {"$match": {"metadata.browser": {"$exists": true}}},
            {
                "$addFields":
                    {"browserName": {"$arrayElemAt": [{"$split": ["$metadata.browser", " "]}, 0]}}
            },
            {
                "$match": {
                    "browserName": {"$nin": [null, "", "null"]},
                    "created_at": {"$gte": ISODate("2020-03-10T01:17:41Z")}
                }
            },
            {
                "$group":
                    {"_id": {"__alias_0": "$browserName"}, "__alias_1": {"$addToSet": "$user_id"}}
            },
            {
                "$project":
                    {"_id": 0, "__alias_0": "$_id.__alias_0", "__alias_1": {"$size": "$__alias_1"}}
            },
            {"$project": {"label": "$__alias_0", "value": "$__alias_1", "_id": 0}},
            {"$limit": 5000}
        ],
        requiresRowStoreExpr: false,
        requiredRowstoreReads: 0
    });

    // Cases that may be improved by future work:

    // The limit below creates a Query Solution Node between the column scan and the group.
    // Our optimization is not clever enough to see that the limit QSN is irrelevant.
    test({
        agg: [{$limit: 100}, {$group: {_id: null, a: {$push: "$a"}}}],
        requiresRowStoreExpr: true,  // ideally this would be false
        requiredRowstoreReads: 4
    });

    // $match with a nested path filter than can be pushed down.
    // This fails to even use the column store index.  It should be able to in the future.
    assert.throws(() => {
        test({
            agg: [{$match: {"a.e": 1}}, {$group: {_id: "$_id", a: {$push: "$a"}}}],
            requiresRowStoreExpr: false,
            requiredRowstoreReads: 0
        });
    });
}

setupCollections();
runAllAggregations();
