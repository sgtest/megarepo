// Contains helpers for checking, based on the explain output, properties of a
// plan. For instance, there are helpers for checking whether a plan is a collection
// scan or whether the plan is covered (index only).

import {FixtureHelpers} from "jstests/libs/fixture_helpers.js";
import {usedBonsaiOptimizer} from "jstests/libs/optimizer_utils.js";

/**
 * Returns query planner part of explain for every node in the explain report.
 */
export function getQueryPlanners(explain) {
    return getAllNodeExplains(explain).map(explain => {
        const queryPlanner = getNestedProperty(explain, "queryPlanner");
        return queryPlanner ? queryPlanner : explain;
    });
}

/**
 * Utility to return the 'queryPlanner' section of 'explain'. The input is the root of the explain
 * output.
 *
 * This helper function can be used for any optimizer.
 */
export function getQueryPlanner(explain) {
    explain = getSingleNodeExplain(explain);
    if ("queryPlanner" in explain) {
        const qp = explain.queryPlanner;
        // Sharded case.
        if ("winningPlan" in qp && "shards" in qp.winningPlan) {
            return qp.winningPlan.shards[0];
        }
        return qp;
    }
    assert(explain.hasOwnProperty("stages"), explain);
    const stage = explain.stages[0];
    assert(stage.hasOwnProperty("$cursor"), explain);
    const cursorStage = stage.$cursor
    assert(cursorStage.hasOwnProperty("queryPlanner"), explain);
    return cursorStage.queryPlanner;
}

/**
 * Extracts and returns an array of explain outputs for every shard in a sharded cluster; returns
 * the original explain output in case of a single replica set.
 */
export function getAllNodeExplains(explain) {
    const shards = getNestedProperty(explain, "shards");
    if (shards) {
        const shardNames = Object.keys(shards);
        return shardNames.map(shardName => shards[shardName]);
    }
    return [explain];
}

/**
 * Returns the output from a single shard if 'explain' was obtained from an unsharded collection;
 * returns 'explain' as is otherwise.
 *
 * This helper function can be used for any optimizer.
 */
export function getSingleNodeExplain(explain) {
    if ("shards" in explain) {
        const shards = explain.shards;
        const shardNames = Object.keys(shards);
        // There should only be one shard given that this function assumes that 'explain' was
        // obtained from an unsharded collection.
        assert.eq(shardNames.length, 1, explain);
        return shards[shardNames[0]];
    }
    return explain;
}

/**
 * Returns a sub-element of the 'queryPlanner' explain output which represents a winning plan.
 * For sharded collections, this may return the top-level "winningPlan" which contains the shards.
 * To ensure getting the winning plan for a specific shard, provide as input the specific explain
 * for that shard i.e, queryPlanner.winningPlan.shards[shardNames[0]].
 *
 * This helper function can be used for any optimizer.
 */
export function getWinningPlan(queryPlanner) {
    // The 'queryPlan' format is used when the SBE engine is turned on. If this field is present,
    // it will hold a serialized winning plan, otherwise it will be stored in the 'winningPlan'
    // field itself.
    return queryPlanner.winningPlan.hasOwnProperty("queryPlan") ? queryPlanner.winningPlan.queryPlan
                                                                : queryPlanner.winningPlan;
}

export function getWinningSBEPlan(queryPlanner) {
    assert(queryPlanner.winningPlan.hasOwnProperty("slotBasedPlan"), queryPlanner)
    return queryPlanner.winningPlan.slotBasedPlan;
}

/**
 * Returns the winning plan from the corresponding sub-node of classic/SBE explain output. Takes
 * into account that the plan may or may not have agg stages.
 *
 * This helper function can be used for any optimizer.
 */
export function getWinningPlanFromExplain(explain, isSBEPlan = false) {
    if ("shards" in explain) {
        for (const shardName in explain.shards) {
            let queryPlanner = getQueryPlanner(explain.shards[shardName]);
            return isSBEPlan ? getWinningSBEPlan(queryPlanner) : getWinningPlan(queryPlanner);
        }
    }

    if (explain.hasOwnProperty("pipeline")) {
        const pipeline = explain.pipeline;
        // Pipeline stages' explain output come in two shapes:
        // 1. When in single node, as a single object array
        // 2. When in sharded, as an object.
        if (pipeline.constructor === Array) {
            return getWinningPlanFromExplain(pipeline[0].$cursor, isSBEPlan);
        } else {
            return getWinningPlanFromExplain(pipeline, isSBEPlan);
        }
    }

    let queryPlanner = getQueryPlanner(explain);
    return isSBEPlan ? getWinningSBEPlan(queryPlanner) : getWinningPlan(queryPlanner);
}

/**
 * Returns the winning SBE plan from the corresponding sub-node of classic/SBE explain output. Takes
 * into account that the plan may or may not have agg stages.
 *
 * This helper function can be used for any optimizer.
 */
export function getWinningSBEPlanFromExplain(explain) {
    if ("shards" in explain) {
        for (const shardName in explain.shards) {
            let queryPlanner = getQueryPlanner(explain.shards[shardName]);
            return getWinningSBEPlan(queryPlanner);
        }
    }

    if (explain.hasOwnProperty("pipeline")) {
        const pipeline = explain.pipeline;
        // Pipeline stages' explain output come in two shapes:
        // 1. When in single node, as a single object array
        // 2. When in sharded, as an object.
        if (pipeline.constructor === Array) {
            return getWinningSBEPlanFromExplain(pipeline[0].$cursor);
        } else {
            return getWinningSBEPlanFromExplain(pipeline);
        }
    }

    let queryPlanner = getQueryPlanner(explain);
    return getWinningSBEPlan(queryPlanner);
}

/**
 * Returns an element of explain output which represents a rejected candidate plan.
 *
 * This helper function can be used for any optimizer. However, currently for the CQF optimizer,
 * rejected plans are not included in the explain output
 */
export function getRejectedPlan(rejectedPlan) {
    // The 'queryPlan' format is used when the SBE engine is turned on. If this field is present,
    // it will hold a serialized winning plan, otherwise it will be stored in the 'rejectedPlan'
    // element itself.
    return rejectedPlan.hasOwnProperty("queryPlan") ? rejectedPlan.queryPlan : rejectedPlan;
}

/**
 * Returns a sub-element of the 'cachedPlan' explain output which represents a query plan.
 *
 * This helper function can be used only with "classic" optimizer. TODO SERVER-83768: extend the
 * functionality of this helper for CQF plans.
 */
export function getCachedPlan(cachedPlan) {
    // The 'queryPlan' format is used when the SBE engine is turned on. If this field is present, it
    // will hold a serialized cached plan, otherwise it will be stored in the 'cachedPlan' field
    // itself.
    return cachedPlan.hasOwnProperty("queryPlan") ? cachedPlan.queryPlan : cachedPlan;
}

/**
 * Given the root stage of explain's JSON representation of a query plan ('root'), returns all
 * subdocuments whose stage is 'stage'. Returns an empty array if the plan does not have the
 * requested stage. if 'stage' is 'null' returns all the stages in 'root'.
 *
 * This helper function can be used for any optimizer.
 */
export function getPlanStages(root, stage) {
    var results = [];

    if (root.stage === stage || stage === undefined || root.nodeType === stage) {
        results.push(root);
    }

    if ("inputStage" in root) {
        results = results.concat(getPlanStages(root.inputStage, stage));
    }

    if ("inputStages" in root) {
        for (var i = 0; i < root.inputStages.length; i++) {
            results = results.concat(getPlanStages(root.inputStages[i], stage));
        }
    }

    if ("queryPlanner" in root) {
        results = results.concat(getPlanStages(getWinningPlan(root.queryPlanner), stage));
    }

    if ("thenStage" in root) {
        results = results.concat(getPlanStages(root.thenStage, stage));
    }

    if ("elseStage" in root) {
        results = results.concat(getPlanStages(root.elseStage, stage));
    }

    if ("outerStage" in root) {
        results = results.concat(getPlanStages(root.outerStage, stage));
    }

    if ("innerStage" in root) {
        results = results.concat(getPlanStages(root.innerStage, stage));
    }

    if ("queryPlan" in root) {
        results = results.concat(getPlanStages(root.queryPlan, stage));
    }

    if ("child" in root) {
        results = results.concat(getPlanStages(root.child, stage));
    }

    if ("leftChild" in root) {
        results = results.concat(getPlanStages(root.leftChild, stage));
    }

    if ("rightChild" in root) {
        results = results.concat(getPlanStages(root.rightChild, stage));
    }

    if ("shards" in root) {
        if (Array.isArray(root.shards)) {
            results =
                root.shards.reduce((res, shard) => res.concat(getPlanStages(
                                       shard.hasOwnProperty("winningPlan") ? getWinningPlan(shard)
                                                                           : shard.executionStages,
                                       stage)),
                                   results);
        } else {
            const shards = Object.keys(root.shards);
            results = shards.reduce(
                (res, shard) => res.concat(getPlanStages(root.shards[shard], stage)), results);
        }
    }

    return results;
}

/**
 * Given the root stage of explain's JSON representation of a query plan ('root'), returns a list of
 * all the stages in 'root'.
 *
 * This helper function can be used for any optimizer.
 */
export function getAllPlanStages(root) {
    return getPlanStages(root);
}

/**
 * Given the root stage of explain's JSON representation of a query plan ('root'), returns the
 * subdocument with its stage as 'stage'. Returns null if the plan does not have such a stage.
 * Asserts that no more than one stage is a match.
 *
 * This helper function can be used for any optimizer.
 */
export function getPlanStage(root, stage) {
    assert(stage, "Stage was not defined in getPlanStage.")
    var planStageList = getPlanStages(root, stage);

    if (planStageList.length === 0) {
        return null;
    } else {
        assert(planStageList.length === 1,
               "getPlanStage expects to find 0 or 1 matching stages. planStageList: " +
                   tojson(planStageList));
        return planStageList[0];
    }
}

/**
 * Returns the set of rejected plans from the given replset or sharded explain output.
 *
 * This helper function can be used for any optimizer.
 */
export function getRejectedPlans(root) {
    if (root.queryPlanner.winningPlan.hasOwnProperty("shards")) {
        const rejectedPlans = [];
        for (let shard of root.queryPlanner.winningPlan.shards) {
            for (let rejectedPlan of shard.rejectedPlans) {
                rejectedPlans.push(Object.assign({shardName: shard.shardName}, rejectedPlan));
            }
        }
        return rejectedPlans;
    }
    return root.queryPlanner.rejectedPlans;
}

/**
 * Given the root stage of explain's JSON representation of a query plan ('root'), returns true if
 * the query planner reports at least one rejected alternative plan, and false otherwise.
 *
 * This helper function can be used for any optimizer. Currently for CQF optimizer, this function
 * returns always true (TODO SERVER-77719: address this behavior).
 */
export function hasRejectedPlans(root) {
    function sectionHasRejectedPlans(explainSection, optimizer = "classic") {
        if (optimizer == "CQF") {
            // TODO SERVER-77719: The existence of alternative/rejected plans will be re-evaluated
            // in the future.
            return true;
        }

        assert(explainSection.hasOwnProperty("rejectedPlans"), tojson(explainSection));
        return explainSection.rejectedPlans.length !== 0;
    }

    function cursorStageHasRejectedPlans(cursorStage) {
        assert(cursorStage.hasOwnProperty("$cursor"), tojson(cursorStage));
        assert(cursorStage.$cursor.hasOwnProperty("queryPlanner"), tojson(cursorStage));
        return sectionHasRejectedPlans(cursorStage.$cursor.queryPlanner);
    }

    if (root.hasOwnProperty("shards")) {
        // This is a sharded agg explain. Recursively check whether any of the shards has rejected
        // plans.
        const shardExplains = [];
        for (const shard in root.shards) {
            shardExplains.push(root.shards[shard]);
        }
        return shardExplains.some(hasRejectedPlans);
    } else if (root.hasOwnProperty("stages")) {
        // This is an agg explain.
        const cursorStages = getAggPlanStages(root, "$cursor");
        return cursorStages.find((cursorStage) => cursorStageHasRejectedPlans(cursorStage)) !==
            undefined;
    } else {
        let optimizer = getOptimizer(root);

        // This is some sort of query explain.
        assert(root.hasOwnProperty("queryPlanner"), tojson(root));
        assert(root.queryPlanner.hasOwnProperty("winningPlan"), tojson(root));
        if (!root.queryPlanner.winningPlan.hasOwnProperty("shards")) {
            // SERVER-77719: Update regarding the expected behavior of the CQF optimizer. Currently
            // CQF explains are empty, when the optimizer returns alternative plans, we should
            // address this.

            // This is an unsharded explain.
            return sectionHasRejectedPlans(root.queryPlanner, optimizer);
        }

        if ("SINGLE_SHARD" == root.queryPlanner.winningPlan.stage) {
            var shards = root.queryPlanner.winningPlan.shards;
            shards.forEach(function assertShardHasRejectedPlans(shard) {
                sectionHasRejectedPlans(shard, optimizer);
            });
        }

        // This is a sharded explain. Each entry in the shards array contains a 'winningPlan' and
        // 'rejectedPlans'.
        return root.queryPlanner.winningPlan.shards.find(
                   (shard) => sectionHasRejectedPlans(shard, optimizer)) !== undefined;
    }
}

/**
 * Returns an array of execution stages from the given replset or sharded explain output.
 *
 * This helper function can be used for any optimizer.
 */
export function getExecutionStages(root) {
    if (root.hasOwnProperty("executionStats") &&
        root.executionStats.executionStages.hasOwnProperty("shards")) {
        const executionStages = [];
        for (let shard of root.executionStats.executionStages.shards) {
            executionStages.push(Object.assign(
                {shardName: shard.shardName, executionSuccess: shard.executionSuccess},
                shard.executionStages));
        }
        return executionStages;
    }
    if (root.hasOwnProperty("shards")) {
        const executionStages = [];
        for (const shard in root.shards) {
            executionStages.push(root.shards[shard].executionStats.executionStages);
        }
        return executionStages;
    }
    return [root.executionStats.executionStages];
}

/**
 * Returns an array of "executionStats" from the given replset or sharded explain output.
 *
 * This helper function can be used for any optimizer.
 */
export function getExecutionStats(root) {
    const allExecutionStats = [];
    if (root.hasOwnProperty("shards")) {
        // This test assumes that there is only one shard in the cluster.
        for (let shardExplain of getAllNodeExplains(root)) {
            allExecutionStats.push(shardExplain.executionStats);
        }
        return allExecutionStats;
    }
    assert(root.hasOwnProperty("executionStats"), root);
    if (root.executionStats.hasOwnProperty("executionStages") &&
        root.executionStats.executionStages.hasOwnProperty("shards")) {
        for (let shardExecutionStats of root.executionStats.executionStages.shards) {
            allExecutionStats.push(shardExecutionStats);
        }
        return allExecutionStats;
    }
    return [root.executionStats];
}

/**
 * Returns the winningPlan.queryPlan of each shard in the explain in a list.
 *
 * This helper function can be used for any optimizer.
 */
export function getShardQueryPlans(root) {
    let result = [];

    if (root.hasOwnProperty("shards")) {
        for (let shardName of Object.keys(root.shards)) {
            let shard = root.shards[shardName];
            result.push(shard.queryPlanner.winningPlan.queryPlan);
        }
    } else {
        for (let shard of root.queryPlanner.winningPlan.shards) {
            result.push(shard.winningPlan.queryPlan);
        }
    }

    return result;
}

/**
 * Performs the given fn on each shard's explain output in root or the top-level explain, if root
 * comes from a standalone explain. fn should accept a single node's top-level explain as input.
 *
 * This helper function currently only works for CQF queries. It can be extended to work for
 * aggregation-like explains.
 */
export function runOnAllTopLevelExplains(root, fn) {
    if (root.hasOwnProperty("shards")) {
        // Sharded agg explain, where the aggregations get pushed down to find on the shards.
        for (let shardName of Object.keys(root.shards)) {
            let shard = root.shards[shardName];
            fn(shard);
        }
    } else if (root.queryPlanner.winningPlan.hasOwnProperty("shards")) {
        // Sharded find explain.
        for (let shard of root.queryPlanner.winningPlan.shards) {
            fn(shard);
        }
    } else {
        // Standalone find explain.
        fn(root);
    }
}

/**
 * Returns an array of strings representing the "planSummary" values found in the input explain.
 * Assumes the given input is the root of an explain.
 *
 * The helper supports sharded and unsharded explain. It can be used with any optimizer. It returns
 * an empty list for non-CQF plans, since only CQF will attach planSummary to explain output.
 */
export function getPlanSummaries(root) {
    let res = [];

    // Queries that use the find system have top-level queryPlanner and winningPlan fields. If it's
    // a sharded query, the shards have their own winningPlan fields to look at.
    if ("queryPlanner" in root && "winningPlan" in root.queryPlanner) {
        const wp = root.queryPlanner.winningPlan;

        if ("shards" in wp) {
            for (let shard of wp.shards) {
                res.push(shard.winningPlan.planSummary);
            }
        } else {
            res.push(wp.planSummary);
        }
    }

    // Queries that use the agg system either have a top-level stages field or a top-level shards
    // field. getQueryPlanner pulls the queryPlanner out of the stages/cursor subfields.
    if ("stages" in root) {
        res.push(getQueryPlanner(root).winningPlan.planSummary);
    }

    if ("shards" in root) {
        for (let shardName of Object.keys(root.shards)) {
            let shard = root.shards[shardName];
            res.push(getQueryPlanner(shard).winningPlan.planSummary);
        }
    }

    return res.filter(elem => elem !== undefined);
}

/**
 * Given the root stage of agg explain's JSON representation of a query plan ('root'), returns all
 * subdocuments whose stage is 'stage'. This can either be an agg stage name like "$cursor" or
 * "$sort", or a query stage name like "IXSCAN" or "SORT".
 *
 * If 'useQueryPlannerSection' is set to 'true', the 'queryPlanner' section of the explain output
 * will be used to lookup the given 'stage', even if 'executionStats' section is available.
 *
 * Returns an empty array if the plan does not have the requested stage. Asserts that agg explain
 * structure matches expected format.
 *
 * This helper function can be used for any optimizer.
 */
export function getAggPlanStages(root, stage, useQueryPlannerSection = false) {
    assert(stage, "Stage was not defined in getAggPlanStages.");
    let results = [];

    function getDocumentSources(docSourceArray) {
        let results = [];
        for (let i = 0; i < docSourceArray.length; i++) {
            let properties = Object.getOwnPropertyNames(docSourceArray[i]);
            if (properties[0] === stage) {
                results.push(docSourceArray[i]);
            }
        }
        return results;
    }

    function getStagesFromQueryLayerOutput(queryLayerOutput) {
        let results = [];

        assert(queryLayerOutput.hasOwnProperty("queryPlanner"));
        assert(queryLayerOutput.queryPlanner.hasOwnProperty("winningPlan"));

        // If execution stats are available, then use the execution stats tree. Otherwise use the
        // plan info from the "queryPlanner" section.
        if (queryLayerOutput.hasOwnProperty("executionStats") && !useQueryPlannerSection) {
            assert(queryLayerOutput.executionStats.hasOwnProperty("executionStages"));
            results = results.concat(
                getPlanStages(queryLayerOutput.executionStats.executionStages, stage));
        } else {
            results =
                results.concat(getPlanStages(getWinningPlan(queryLayerOutput.queryPlanner), stage));
        }

        return results;
    }

    if (root.hasOwnProperty("stages")) {
        assert(root.stages.constructor === Array);

        results = results.concat(getDocumentSources(root.stages));

        if (root.stages[0].hasOwnProperty("$cursor")) {
            results = results.concat(getStagesFromQueryLayerOutput(root.stages[0].$cursor));
        } else if (root.stages[0].hasOwnProperty("$geoNearCursor")) {
            results = results.concat(getStagesFromQueryLayerOutput(root.stages[0].$geoNearCursor));
        }
    }

    if (root.hasOwnProperty("shards")) {
        for (let elem in root.shards) {
            if (root.shards[elem].hasOwnProperty("queryPlanner")) {
                // The shard was able to optimize away the pipeline, which means that the format of
                // the explain output doesn't have the "stages" array.
                assert.eq(true, root.shards[elem].queryPlanner.optimizedPipeline);
                results = results.concat(getStagesFromQueryLayerOutput(root.shards[elem]));

                // Move onto the next shard.
                continue;
            }

            if (!root.shards[elem].hasOwnProperty("stages")) {
                continue;
            }

            assert(root.shards[elem].stages.constructor === Array);

            results = results.concat(getDocumentSources(root.shards[elem].stages));

            const firstStage = root.shards[elem].stages[0];
            if (firstStage.hasOwnProperty("$cursor")) {
                results = results.concat(getStagesFromQueryLayerOutput(firstStage.$cursor));
            } else if (firstStage.hasOwnProperty("$geoNearCursor")) {
                results = results.concat(getStagesFromQueryLayerOutput(firstStage.$geoNearCursor));
            }
        }
    }

    // If the agg pipeline was completely optimized away, then the agg explain output will be
    // formatted like the explain output for a find command.
    if (root.hasOwnProperty("queryPlanner")) {
        assert.eq(true, root.queryPlanner.optimizedPipeline);
        results = results.concat(getStagesFromQueryLayerOutput(root));
    }

    return results;
}

/**
 * Given the root stage of agg explain's JSON representation of a query plan ('root'), returns the
 * subdocument with its stage as 'stage'. Returns null if the plan does not have such a stage.
 * Asserts that no more than one stage is a match.
 *
 * If 'useQueryPlannerSection' is set to 'true', the 'queryPlanner' section of the explain output
 * will be used to lookup the given 'stage', even if 'executionStats' section is available.
 *
 * This helper function can be used for any optimizer.
 */
export function getAggPlanStage(root, stage, useQueryPlannerSection = false) {
    assert(stage, "Stage was not defined in getAggPlanStage.")
    let planStageList = getAggPlanStages(root, stage, useQueryPlannerSection);

    if (planStageList.length === 0) {
        return null;
    } else {
        assert.eq(1,
                  planStageList.length,
                  "getAggPlanStage expects to find 0 or 1 matching stages. planStageList: " +
                      tojson(planStageList));
        return planStageList[0];
    }
}

/**
 * Given the root stage of agg explain's JSON representation of a query plan ('root'), returns
 * whether the plan has a stage called 'stage'. It could have more than one to allow for sharded
 * explain plans, and it can search for a query planner stage like "FETCH" or an agg stage like
 * "$group."
 *
 * This helper function can be used for any optimizer.
 */
export function aggPlanHasStage(root, stage) {
    return getAggPlanStages(root, stage).length > 0;
}

/**
 * Given the root stage of explain's BSON representation of a query plan ('root'),
 * returns true if the plan has a stage called 'stage'.
 *
 * Expects that the stage appears once or zero times per node. If the stage appears more than once
 * on one node's query plan, an error will be thrown.
 */
export function planHasStage(db, root, stage) {
    assert(stage, "Stage was not defined in planHasStage.")
    const matchingStages = getPlanStages(root, stage);

    // If we are executing against a mongos, we may get more than one occurrence of the stage.
    if (FixtureHelpers.isMongos(db) || TestData.testingReplicaSetEndpoint) {
        return matchingStages.length >= 1;
    } else {
        assert.lt(matchingStages.length,
                  2,
                  `Expected to find 0 or 1 matching stages: ${tojson(matchingStages)}`);
        return matchingStages.length === 1;
    }
}

/**
 * A query is covered iff it does *not* have a FETCH stage or a COLLSCAN.
 *
 * Given the root stage of explain's BSON representation of a query plan ('root'),
 * returns true if the plan is index only. Otherwise returns false.
 *
 * This helper function can be used for any optimizer.
 */
export function isIndexOnly(db, root) {
    // SERVER-77719: Ensure that the decision for using the scan lines up with CQF optimizer.
    return !planHasStage(db, root, "FETCH") && !planHasStage(db, root, "COLLSCAN") &&
        !planHasStage(db, root, "PhysicalScan") && !planHasStage(db, root, "CoScan") &&
        !planHasStage(db, root, "Seek");
}

/**
 * Returns true if the BSON representation of a plan rooted at 'root' is using
 * an index scan, and false otherwise.
 *
 * This helper function can be used for any optimizer.
 */
export function isIxscan(db, root) {
    // SERVER-77719: Ensure that the decision for using the scan lines up with CQF optimizer.
    return planHasStage(db, root, "IXSCAN") || planHasStage(db, root, "IndexScan");
}

/**
 * Returns true if the plan is formed of a single EOF stage. False otherwise.
 *
 * This helper function can be used for any optimizer.
 */
export function isEofPlan(db, root) {
    return planHasStage(db, root, "EOF");
}

/**
 * Returns true if the BSON representation of a plan rooted at 'root' is using
 * the idhack fast path, and false otherwise. These can be represented either as
 * explicit 'IDHACK' stages, or as 'CLUSTERED_IXSCAN' stages with equal min & max record bounds
 * in the case of clustered collections.
 *
 * This helper function can be used only with classic optimizer (TODO SERVER-77719: address this
 * behavior).
 */
export function isIdhack(db, root) {
    // SERVER-77719: Ensure that the decision for using the scan lines up with CQF optimizer.
    if (planHasStage(db, root, "IDHACK")) {
        return true;
    }
    if (!isClusteredIxscan(db, root)) {
        return false;
    }
    const stage = getPlanStages(root, "CLUSTERED_IXSCAN")[0];
    return stage.minRecord === stage.maxRecord;
}

/**
 * Returns true if the BSON representation of a plan indicates that this plan was generated by the
 * fastpath logic of the Bonsai optimiser.
 */
export function isBonsaiFastPathPlan(db, explain) {
    return planHasStage(db, explain, "FASTPATH");
}

/**
 * Returns true if the BSON representation of a plan rooted at 'root' is using
 * a collection scan, and false otherwise.
 *
 * This helper function can be used for any optimizer. This assumes that the PhysicalScan operator
 * of CQF is equivalent to COLLSCAN.
 */
export function isCollscan(db, root) {
    return planHasStage(db, root, "COLLSCAN") || planHasStage(db, root, "PhysicalScan");
}

/**
 * Returns true if the BSON representation of a plan rooted at 'root' is using
 * a clustered Ix scan, and false otherwise.
 *
 * This helper function can be used only for the "classic" optimizer. Note that it can be applied to
 * CQF plans, but it will always return false because there is not yet a clustered IXSCAN
 * representation in Bonsai.
 */
export function isClusteredIxscan(db, root) {
    // SERVER-77719: Ensure that the decision for using the scan lines up with CQF optimizer.
    return planHasStage(db, root, "CLUSTERED_IXSCAN");
}

/**
 * Returns true if the BSON representation of a plan rooted at 'root' is using the aggregation
 * framework, and false otherwise.
 *
 * This helper function can be used for any optimizer.
 */
export function isAggregationPlan(root) {
    if (root.hasOwnProperty("shards")) {
        const shards = Object.keys(root.shards);
        return shards.reduce(
                   (res, shard) => res + root.shards[shard].hasOwnProperty("stages") ? 1 : 0, 0) >
            0;
    }
    return root.hasOwnProperty("stages");
}

/**
 * Returns true if the BSON representation of a plan rooted at 'root' is using just the query layer,
 * and false otherwise.
 *
 * This helper function can be used for any optimizer.
 */
export function isQueryPlan(root) {
    if (root.hasOwnProperty("shards")) {
        const shards = Object.keys(root.shards);
        return shards.reduce(
                   (res, shard) => res + root.shards[shard].hasOwnProperty("queryPlanner") ? 1 : 0,
                   0) > 0;
    }
    return root.hasOwnProperty("queryPlanner");
}

/**
 * Get the "chunk skips" for a single shard. Here, "chunk skips" refer to documents excluded by the
 * shard filter.
 *
 * This helper function can be used only with the "classic" optimizer. TODO SERVER-77719: extend the
 * functionality of this helper for CQF operators
 */
export function getChunkSkipsFromShard(shardPlan, shardExecutionStages) {
    const shardFilterPlanStage = getPlanStage(getWinningPlan(shardPlan), "SHARDING_FILTER");
    if (!shardFilterPlanStage) {
        return 0;
    }

    if (shardFilterPlanStage.hasOwnProperty("planNodeId")) {
        const shardFilterNodeId = shardFilterPlanStage.planNodeId;

        // If the query plan's shard filter has a 'planNodeId' value, we search for the
        // corresponding SBE filter stage and use its stats to determine how many documents were
        // excluded.
        const filters = getPlanStages(shardExecutionStages.executionStages, "filter")
                            .filter(stage => (stage.planNodeId === shardFilterNodeId));
        return filters.reduce((numSkips, stage) => (numSkips + (stage.numTested - stage.nReturned)),
                              0);
    } else {
        // Otherwise, we assume that execution used a "classic" SHARDING_FILTER stage, which
        // explicitly reports a "chunkSkips" value.
        const filters = getPlanStages(shardExecutionStages.executionStages, "SHARDING_FILTER");
        return filters.reduce((numSkips, stage) => (numSkips + stage.chunkSkips), 0);
    }
}

/**
 * Get the sum of "chunk skips" from all shards. Here, "chunk skips" refer to documents excluded by
 * the shard filter.
 *
 * This helper function can be used only with the "classic" optimizer. TODO SERVER-77719: extend the
 * functionality of this helper for CQF operators
 */
export function getChunkSkipsFromAllShards(explainResult) {
    const shardPlanArray = explainResult.queryPlanner.winningPlan.shards;
    const shardExecutionStagesArray = explainResult.executionStats.executionStages.shards;
    assert.eq(shardPlanArray.length, shardExecutionStagesArray.length, explainResult);

    let totalChunkSkips = 0;
    for (let i = 0; i < shardPlanArray.length; i++) {
        totalChunkSkips += getChunkSkipsFromShard(shardPlanArray[i], shardExecutionStagesArray[i]);
    }
    return totalChunkSkips;
}

/**
 * Given explain output at executionStats level verbosity, for a count query, confirms that the root
 * stage is COUNT or RECORD_STORE_FAST_COUNT and that the result of the count is equal to
 * 'expectedCount'.
 *
 * This helper function can be used for any optimizer.
 */
export function assertExplainCount({explainResults, expectedCount}) {
    const execStages = explainResults.executionStats.executionStages;

    // If passed through mongos, then the root stage should be the mongos SINGLE_SHARD stage or
    // SHARD_MERGE stages, with COUNT as the root stage on each shard. If explaining directly on the
    // shard, then COUNT is the root stage.
    if ("SINGLE_SHARD" == execStages.stage || "SHARD_MERGE" == execStages.stage) {
        let totalCounted = 0;
        for (let shardExplain of execStages.shards) {
            const countStage = shardExplain.executionStages;
            assert(countStage.stage === "COUNT" || countStage.stage === "RECORD_STORE_FAST_COUNT",
                   `Root stage on shard is not COUNT or RECORD_STORE_FAST_COUNT. ` +
                       `The actual plan is: ${tojson(explainResults)}`);
            totalCounted += countStage.nCounted;
        }
        assert.eq(totalCounted,
                  expectedCount,
                  assert.eq(totalCounted, expectedCount, "wrong count result"));
    } else {
        assert(execStages.stage === "COUNT" || execStages.stage === "RECORD_STORE_FAST_COUNT",
               `Root stage on shard is not COUNT or RECORD_STORE_FAST_COUNT. ` +
                   `The actual plan is: ${tojson(explainResults)}`);
        assert.eq(
            execStages.nCounted,
            expectedCount,
            "Wrong count result. Actual: " + execStages.nCounted + "expected: " + expectedCount);
    }
}

/**
 * Verifies that a given query uses an index and is covered when used in a count command.
 *
 * This helper function can be used for any optimizer.
 */
export function assertCoveredQueryAndCount({collection, query, project, count}) {
    let explain = collection.find(query, project).explain();
    // SERVER-77719: Update regarding the expected behavior of the CQF optimizer.
    switch (getOptimizer(explain)) {
        case "classic":
            assert(isIndexOnly(db, getWinningPlan(explain.queryPlanner)),
                   "Winning plan was not covered: " + tojson(explain.queryPlanner.winningPlan));
            break;
        default:
            break
    }

    // Same query as a count command should also be covered.
    explain = collection.explain("executionStats").find(query).count();
    // SERVER-77719: Update regarding the expected behavior of the CQF optimizer.
    switch (getOptimizer(explain)) {
        case "classic":
            assert(isIndexOnly(db, getWinningPlan(explain.queryPlanner)),
                   "Winning plan for count was not covered: " +
                       tojson(explain.queryPlanner.winningPlan));
            assertExplainCount({explainResults: explain, expectedCount: count});
            break;
        default:
            break
    }
}

/**
 * Runs explain() operation on 'cmdObj' and verifies that all the stages in 'expectedStages' are
 * present exactly once in the plan returned. When 'stagesNotExpected' array is passed, also
 * verifies that none of those stages are present in the explain() plan.
 *
 * This helper function can be used for any optimizer.
 */
export function assertStagesForExplainOfCommand({coll, cmdObj, expectedStages, stagesNotExpected}) {
    const plan = assert.commandWorked(coll.runCommand({explain: cmdObj}));
    const winningPlan = getWinningPlan(plan.queryPlanner);
    for (let expectedStage of expectedStages) {
        assert(planHasStage(coll.getDB(), winningPlan, expectedStage),
               "Could not find stage " + expectedStage + ". Plan: " + tojson(plan));
    }
    for (let stage of (stagesNotExpected || [])) {
        assert(!planHasStage(coll.getDB(), winningPlan, stage),
               "Found stage " + stage + " when not expected. Plan: " + tojson(plan));
    }
    return plan;
}

/**
 * Utility to obtain a value from 'explainRes' using 'getValueCallback'.
 *
 * This helper function can be used for any optimizer.
 */
export function getFieldValueFromExplain(explainRes, getValueCallback) {
    assert(explainRes.hasOwnProperty("queryPlanner"), explainRes);
    const plannerOutput = explainRes.queryPlanner;
    const fieldValue = getValueCallback(plannerOutput);
    assert.eq(typeof fieldValue, "string");
    return fieldValue;
}

/**
 * Get the 'planCacheKey' from 'explainRes'.
 *
 * This helper function can be used for any optimizer.
 */
export function getPlanCacheKeyFromExplain(explainRes, db) {
    explainRes = getSingleNodeExplain(explainRes);
    return getFieldValueFromExplain(explainRes, function(plannerOutput) {
        return (plannerOutput.hasOwnProperty("winningPlan") &&
                plannerOutput.winningPlan.hasOwnProperty("shards"))
            ? plannerOutput.winningPlan.shards[0].planCacheKey
            : plannerOutput.planCacheKey;
    });
}

/**
 * Get the 'queryHash' from 'explainRes'.
 *
 * This helper function can be used for any optimizer.
 */
export function getQueryHashFromExplain(explainRes, db) {
    return getFieldValueFromExplain(explainRes, function(plannerOutput) {
        return (plannerOutput.hasOwnProperty("winningPlan") &&
                plannerOutput.winningPlan.hasOwnProperty("shards"))
            ? plannerOutput.winningPlan.shards[0].queryHash
            : plannerOutput.queryHash;
    });
}

/**
 * Helper to run a explain on the given query shape and get the "planCacheKey" from the explain
 * result.
 *
 * This helper function can be used for any optimizer.
 */
export function getPlanCacheKeyFromShape(
    {query = {}, projection = {}, sort = {}, collation = {}, collection, db}) {
    const explainRes = assert.commandWorked(
        collection.explain().find(query, projection).collation(collation).sort(sort).finish());

    return getPlanCacheKeyFromExplain(explainRes, db);
}

/**
 * Helper to run a explain on the given pipeline and get the "planCacheKey" from the explain
 * result.
 */
export function getPlanCacheKeyFromPipeline(pipeline, collection, db) {
    const explainRes = assert.commandWorked(collection.explain().aggregate(pipeline));

    return getPlanCacheKeyFromExplain(explainRes, db);
}

/**
 * Given the winning query plan, flatten query plan tree into a list of plan stage names.
 *
 * This helper function can be used for any optimizer.
 */
export function flattenQueryPlanTree(winningPlan) {
    let stages = [];
    while (winningPlan) {
        stages.push(winningPlan.stage);
        winningPlan = winningPlan.inputStage;
    }
    stages.reverse();
    return stages;
}

/**
 * Assert that a command plan has no FETCH stage or if the stage is present, it has no filter.
 *
 * This helper function can be used only with the "classic" optimizer. TODO SERVER-77719: extend the
 * functionality of this helper for CQF operators
 */
export function assertNoFetchFilter({coll, cmdObj}) {
    const plan = assert.commandWorked(coll.runCommand({explain: cmdObj}));
    const winningPlan = getWinningPlan(plan.queryPlanner);
    const fetch = getPlanStage(winningPlan, "FETCH");
    assert((fetch === null || !fetch.hasOwnProperty("filter")),
           "Unexpected fetch: " + tojson(fetch));
    return winningPlan;
}

/**
 * Assert that a find plan has a FETCH stage with expected filter and returns a specified number of
 * results.
 *
 * This helper function can be used only with the "classic" optimizer.
 */
export function assertFetchFilter({coll, predicate, expectedFilter, nReturned}) {
    const exp = coll.find(predicate).explain("executionStats");
    const plan = getWinningPlan(exp.queryPlanner);
    const fetch = getPlanStage(plan, "FETCH");
    assert(fetch !== null, "Missing FETCH stage " + plan);
    assert(fetch.hasOwnProperty("filter"),
           "Expected filter in the fetch stage, got " + tojson(fetch));
    assert.eq(expectedFilter,
              fetch.filter,
              "Expected filter " + tojson(expectedFilter) + " got " + tojson(fetch.filter));

    if (nReturned !== null) {
        assert.eq(exp.executionStats.nReturned,
                  nReturned,
                  "Expected " + nReturned + " documents, got " + exp.executionStats.nReturned);
    }
}

/**
 * Recursively checks if a javascript object contains a nested property key and returns the value.
 * Note, this only recurses into other objects, array elements are ignored.
 *
 * This helper function can be used for any optimizer.
 */
function getNestedProperty(object, key) {
    if (typeof object !== "object") {
        return null;
    }

    for (const k in object) {
        if (k == key) {
            return object[k];
        }

        const result = getNestedProperty(object[k], key);
        if (result) {
            return result;
        }
    }
    return null;
}

/**
 * Recognizes the query engine used by the query (sbe/classic).
 *
 * This helper function can be used for any optimizer.
 */
export function getEngine(explain) {
    const queryPlanner = {...getQueryPlanner(explain)};
    return getNestedProperty(queryPlanner, "slotBasedPlan") ? "sbe" : "classic";
}

/**
 * Asserts that a pipeline runs with the engine that is passed in as a parameter.
 *
 * This helper function can be used for any optimizer.
 */
export function assertEngine(pipeline, engine, coll) {
    const explain = coll.explain().aggregate(pipeline);
    assert.eq(getEngine(explain), engine);
}

/**
 * Returns the optimizer (name string) used to generate the explain output ("classic" or "CQF")
 *
 * This helper function can be used for any optimizer.
 */
export function getOptimizer(explain) {
    if (usedBonsaiOptimizer(explain)) {
        return "CQF";
    } else {
        return "classic";
    }
}

/**
 * Returns the number of index scans in a query plan.
 *
 * This helper function can be used for any optimizer.
 */
export function getNumberOfIndexScans(explain) {
    let stages = {"classic": "IXSCAN", "CQF": "IndexScan"};
    let optimizer = getOptimizer(explain);
    const indexScans = getPlanStages(getWinningPlan(explain.queryPlanner), stages[optimizer]);
    return indexScans.length;
}

/**
 * Returns the number of column scans in a query plan.
 *
 * This helper function can be used for any optimizer.
 */
export function getNumberOfColumnScans(explain) {
    // SERVER-77719: Update regarding the expected behavior of the CQF optimizer (what is the stage
    // name for CQF for a column scan).
    let stages = {"classic": "COLUMN_SCAN"};
    let optimizer = getOptimizer(explain);
    if (optimizer == "CQF") {
        return 0;
    }
    const columnIndexScans = getPlanStages(getWinningPlan(explain.queryPlanner), stages[optimizer]);
    return columnIndexScans.length;
}

/*
 * Returns whether a query is using a multikey index.
 *
 * This helper function can be used only for "classic" optimizer.
 */
export function isIxscanMultikey(winningPlan) {
    // SERVER-77719: Update to expected this method to allow also use with CQF optimizer.
    let ixscanStage = getPlanStage(winningPlan, "IXSCAN");
    return ixscanStage.isMultiKey;
}

/**
 * Verify that the explain command output 'explain' shows a BATCHED_DELETE stage with an
 * nWouldDelete value equal to 'nWouldDelete'.
 */
export function checkNWouldDelete(explain, nWouldDelete) {
    assert.commandWorked(explain);
    assert("executionStats" in explain);
    var executionStats = explain.executionStats;
    assert("executionStages" in executionStats);

    // If passed through mongos, then BATCHED_DELETE stage(s) should be below the SHARD_WRITE
    // mongos stage.  Otherwise the BATCHED_DELETE stage is the root stage.
    var execStages = executionStats.executionStages;
    if ("SHARD_WRITE" === execStages.stage) {
        let totalToBeDeletedAcrossAllShards = 0;
        execStages.shards.forEach(function(shardExplain) {
            const rootStageName = shardExplain.executionStages.stage;
            assert(rootStageName === "BATCHED_DELETE", tojson(execStages));
            totalToBeDeletedAcrossAllShards += shardExplain.executionStages.nWouldDelete;
        });
        assert.eq(totalToBeDeletedAcrossAllShards, nWouldDelete, explain);
    } else {
        assert(execStages.stage === "BATCHED_DELETE", explain);
        assert.eq(execStages.nWouldDelete, nWouldDelete, explain);
    }
}
