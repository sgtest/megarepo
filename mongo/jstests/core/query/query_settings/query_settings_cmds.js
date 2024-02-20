// Tests query settings setQuerySettings and removeQuerySettings commands as well as $querySettings
// agg stage.
// @tags: [
//   directly_against_shardsvrs_incompatible,
//   featureFlagQuerySettings,
//   simulate_atlas_proxy_incompatible,
// ]
//

import {assertDropAndRecreateCollection} from "jstests/libs/collection_drop_recreate.js";
import {QuerySettingsUtils} from "jstests/libs/query_settings_utils.js";

// Creating the collection, because some sharding passthrough suites are failing when explain
// command is issued on the nonexistent database and collection.
const coll = assertDropAndRecreateCollection(db, jsTestName());
const qsutils = new QuerySettingsUtils(db, coll.getName());

/**
 * Tests query settings setQuerySettings and removeQuerySettings commands as well as $querySettings
 * agg stage.
 *
 *  params.queryA - query of some shape
 *  params.queryShapeA - debugQueryShape of params.queryA
 *  params.queryB - query of shape different from A
 *  params.queryBPrime - different query of the same shape as params.queryB
 *  params.querySettingsA - query setting for all queries of the shape as params.queryA
 *  params.querySettingsB - query setting for all queries of the shape as params.queryB
 */
let testQuerySettingsUsing =
    (params) => {
        // Ensure that query settings cluster parameter is empty.
        { qsutils.assertQueryShapeConfiguration([]); }

        // Ensure that 'querySettings' cluster parameter contains QueryShapeConfiguration after
        // invoking setQuerySettings command.
        {
            qsutils.assertExplainQuerySettings(params.queryA, undefined);
            assert.commandWorked(db.adminCommand(
                {setQuerySettings: params.queryA, settings: params.querySettingsA}));
            qsutils.assertQueryShapeConfiguration(
                [qsutils.makeQueryShapeConfiguration(params.querySettingsA, params.queryA)]);
        }

        // Ensure that 'querySettings' cluster parameter contains both QueryShapeConfigurations
        // after invoking setQuerySettings command.
        {
            qsutils.assertExplainQuerySettings(params.queryB, undefined);
            assert.commandWorked(db.adminCommand(
                {setQuerySettings: params.queryB, settings: params.querySettingsB}));
            qsutils.assertQueryShapeConfiguration([
                qsutils.makeQueryShapeConfiguration(params.querySettingsA, params.queryA),
                qsutils.makeQueryShapeConfiguration(params.querySettingsB, params.queryB)
            ]);
        }

        // Ensure that 'querySettings' cluster parameter gets updated on subsequent call of
        // setQuerySettings by passing a QueryShapeHash.
        {
            const queryShapeHashA = qsutils.getQueryHashFromQuerySettings(params.queryShapeA);
            assert.neq(queryShapeHashA, undefined);
            assert.commandWorked(db.adminCommand(
                {setQuerySettings: queryShapeHashA, settings: params.querySettingsB}));
            qsutils.assertQueryShapeConfiguration([
                qsutils.makeQueryShapeConfiguration(params.querySettingsB, params.queryA),
                qsutils.makeQueryShapeConfiguration(params.querySettingsB, params.queryB)
            ]);
        }

        // Ensure that 'querySettings' cluster parameter gets updated on subsequent call of
        // setQuerySettings by passing a different QueryInstance with the same QueryShape.
        {
            assert.commandWorked(db.adminCommand(
                {setQuerySettings: params.queryBPrime, settings: params.querySettingsA}));
            qsutils.assertQueryShapeConfiguration([
                qsutils.makeQueryShapeConfiguration(params.querySettingsB, params.queryA),
                qsutils.makeQueryShapeConfiguration(params.querySettingsA, params.queryB)
            ]);
        }

        // Ensure that removeQuerySettings command removes one query settings from the
        // 'settingsArray' of the 'querySettings' cluster parameter by providing a query instance.
        {
            // Some suites may transparently retry this request if it fails due to e.g., step down.
            // However, the settings may already be modified. The retry will then fail with:
            //  "A matching query settings entry does not exist"
            // despite the call actually succeeding.
            // Since we immediately check the correct set of settings exists, this test still
            // verifies the correct behaviour, even without an assert.commandWorked here.
            db.adminCommand({removeQuerySettings: params.queryBPrime});
            qsutils.assertQueryShapeConfiguration(
                [qsutils.makeQueryShapeConfiguration(params.querySettingsB, params.queryA)]);
            qsutils.assertExplainQuerySettings(params.queryB, undefined);
        }

        // Ensure that query settings cluster parameter is empty by issuing a removeQuerySettings
        // command providing a query shape hash.
        {
            qsutils.removeAllQuerySettings();
            qsutils.assertExplainQuerySettings(params.queryA, undefined);
        }
    }

let buildPipeline = (matchValue) => [{$match: {matchKey: matchValue}},
                                     {
                                         $group: {
                                             _id: "groupID",
                                             values: {$addToSet: "$value"},
                                         },
                                     },
];

let buildPipelineShape = matchValue => {
    return {
        command: "aggregate", pipeline: [
            {$match: {matchKey: matchValue}},
            {$group: {_id: "?string", values: {$addToSet: "$value"}}}
        ],
    }
};

let testQuerySettingsParameterized = ({find, distinct, aggregate}) => {
    // Testing find query settings.
    testQuerySettingsUsing({
        queryA: qsutils.makeFindQueryInstance({filter: {a: 15}}),
        queryShapeA: {command: "find", filter: {a: {$eq: "?number"}}},
        queryB: qsutils.makeFindQueryInstance({filter: {b: "string"}}),
        queryBPrime: qsutils.makeFindQueryInstance({filter: {b: "another string"}}),
        ...find
    });

    // Same for distinct query settings.
    testQuerySettingsUsing({
        queryA: qsutils.makeDistinctQueryInstance({key: "k", query: {a: 1}}),
        queryShapeA: {command: "distinct", key: "k", query: {a: {$eq: "?number"}}},
        queryB: qsutils.makeDistinctQueryInstance({key: "k", query: {b: "string"}}),
        queryBPrime: qsutils.makeDistinctQueryInstance({key: "k", query: {b: "another string"}}),
        ...distinct
    });

    // Same for aggregate query settings.
    testQuerySettingsUsing({
        queryA: qsutils.makeAggregateQueryInstance({
            pipeline: buildPipeline(15),
        }),
        queryShapeA: buildPipelineShape({$eq: "?number"}),
        queryB: qsutils.makeAggregateQueryInstance({
            pipeline: buildPipeline("string"),
        }),
        queryBPrime: qsutils.makeAggregateQueryInstance({
            pipeline: buildPipeline("another string"),
        }),
        ...aggregate
    });
};

// Test changing allowed indexes.
testQuerySettingsParameterized({
    find: {
        querySettingsA: {indexHints: {allowedIndexes: ["a_1", {$natural: 1}]}},
        querySettingsB: {indexHints: {allowedIndexes: ["b_1"]}}
    },
    distinct: {
        querySettingsA: {indexHints: {allowedIndexes: ["a_1", {$natural: 1}]}},
        querySettingsB: {indexHints: {allowedIndexes: ["b_1"]}}
    },
    aggregate: {
        querySettingsA: {indexHints: {allowedIndexes: ["groupID_1", {$natural: 1}]}},
        querySettingsB: {indexHints: {allowedIndexes: ["matchKey_1"]}}
    }
});

// Test changing reject. With no other settings present, there's only one valid value for
// reject - true. Tests attempting to change this value to false will fail, as they are
// required to issue a removeQuerySettings instead.
// However, for the sake of coverage, test what can be tested when reject is the only
// setting present.
testQuerySettingsParameterized({
    find: {querySettingsA: {reject: true}, querySettingsB: {reject: true}},
    distinct: {querySettingsA: {reject: true}, querySettingsB: {reject: true}},
    aggregate: {querySettingsA: {reject: true}, querySettingsB: {reject: true}}
});

// Test changing reject, with an unrelated setting present to allow it to be changed to false.
const unrelated = {
    indexHints: {allowedIndexes: ["a_1", {$natural: 1}]}
};
testQuerySettingsParameterized({
    find: {
        querySettingsA: {...unrelated, reject: true},
        querySettingsB: {...unrelated, reject: false}
    },
    distinct: {
        querySettingsA: {...unrelated, reject: true},
        querySettingsB: {...unrelated, reject: false}
    },
    aggregate: {
        querySettingsA: {...unrelated, reject: true},
        querySettingsB: {...unrelated, reject: false}
    }
});

// Test that making QuerySettings empty via setQuerySettings fails.
{
    // Test that setting reject=false as the _only_ setting fails as the newly constructed
    // QuerySettings would be empty.
    let query = qsutils.makeFindQueryInstance({filter: {a: 15}});
    assert.commandFailedWithCode(
        db.adminCommand({setQuerySettings: query, settings: {reject: false}}), 8587401);

    // Set reject=true, which should be permitted as it is non-default behaviour.
    assert.commandWorked(db.adminCommand({setQuerySettings: query, settings: {reject: true}}));

    // Setting reject=false would make the existing QuerySettings empty; verify that this fails.
    assert.commandFailedWithCode(
        db.adminCommand({setQuerySettings: query, settings: {reject: false}}), 8587402);

    // Confirm that the settings can indeed be removed (also cleans up after above test).
    db.adminCommand({removeQuerySettings: query});
    // Check that the given setting has indeed been removed.
    qsutils.assertQueryShapeConfiguration([]);
}
