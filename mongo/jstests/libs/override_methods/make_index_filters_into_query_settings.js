import {getWinningPlan, isIdhack} from "jstests/libs/analyze_plan.js";
import {OverrideHelpers} from "jstests/libs/override_methods/override_helpers.js";

function addOptionalQueryFields(src, dst) {
    for (let field of ["projection", "sort", "collation"]) {
        if (src[field]) {
            dst[field] = src[field];
        }
    }
}

/**
 * Since the existing tests run asserts over 'indexFilterSet' field, we need to set it to true, when
 * query settings are set.
 */
function populateIndexFilterSetIfQuerySettingsArePresent(response) {
    if (response.queryPlanner && response.queryPlanner.querySettings) {
        response.queryPlanner.indexFilterSet = true;
    }
    return response;
}

/**
 * If aggregate command was running '$planCacheStats', then set 'indexFilterSet' flag to true, if
 * query settings are set.
 */
function processAggregateResponse(cmdObj, response) {
    if (cmdObj.pipeline.some(stage => stage.hasOwnProperty("$planCacheStats"))) {
        for (let cacheEntry of response.cursor.firstBatch) {
            cacheEntry.indexFilterSet = cacheEntry.hasOwnProperty('querySettings')
        }
    }

    return response;
}

function planCacheSetFilterToSetQuerySettings(conn, dbName, cmdObj) {
    // NOTE: If a collection doesn't exist, then setting query settings should fail.
    const db = conn.getDB(dbName);
    const collName = cmdObj.planCacheSetFilter;
    const collExists = db.getCollectionNames().indexOf(collName) != -1;
    if (!collExists) {
        return {ok: 0};
    }

    // Map the input of the index filters into query settings command.
    const queryInstance = {find: collName, filter: cmdObj.query};
    addOptionalQueryFields(cmdObj, queryInstance);

    // Setting index filters on idhack query is no-op for index filter command, but is a failure
    // for query settings command, therefore avoid specifying query settings and return success.
    const explain = db.runCommand({explain: queryInstance});
    if (isIdhack(db, getWinningPlan(explain.queryPlanner))) {
        return {ok: 1};
    }

    queryInstance["$db"] = dbName;
    const settings = {indexHints: {allowedIndexes: cmdObj["indexes"]}};

    // Run setQuerySettings command.
    const adminDb = conn.getDB("admin");
    return adminDb.runCommand({setQuerySettings: queryInstance, settings: settings});
}

function planCacheClearFiltersToRemoveAllQuerySettings(conn, cmdObj) {
    const adminDb = conn.getDB("admin");
    let pipeline = [
        {$querySettings: {}},
        // Perform the $match on the collection name.
        {$match: {"representativeQuery.find": cmdObj.planCacheClearFilters}}
    ];

    // Add additional $match stage for the query filter.
    function addMatchIfPresent(attr, outAttr = attr) {
        if (cmdObj[attr]) {
            pipeline.push({$match: {[`representativeQuery.${outAttr}`]: cmdObj[attr]}});
        }
    }
    addMatchIfPresent("query", "filter");
    addMatchIfPresent("projection");
    addMatchIfPresent("sort");
    addMatchIfPresent("collation");

    adminDb.aggregate(pipeline).toArray().forEach((queryShapeConfig, _) => {
        assert.commandWorked(
            adminDb.runCommand({removeQuerySettings: queryShapeConfig.queryShapeHash}));
    });
    return {ok: 1};
}

function planCacheListFiltersToDollarQuerySettings(conn, cmdObj) {
    const adminDb = conn.getDB("admin");
    const allQueryShapeConfigurations =
        adminDb
            .aggregate([
                {$querySettings: {}},
                // Perform the match on the collection name.
                {$match: {"representativeQuery.find": cmdObj.planCacheListFilters}}
            ])
            .toArray();

    function fromQueryShapeConfigurationToIndexFilter(queryShapeConfig) {
        let indexFilter = {
            query: queryShapeConfig.representativeQuery.filter,
            indexes: queryShapeConfig.settings.indexHints.allowedIndexes
        };
        addOptionalQueryFields(queryShapeConfig.representativeQuery, indexFilter);
        return indexFilter;
    }
    return {
        ok: 1,
        filters: allQueryShapeConfigurations.map(fromQueryShapeConfigurationToIndexFilter)
    };
}

function runCommandOverride(conn, dbName, cmdName, cmdObj, clientFunction, makeFuncArgs) {
    if (cmdName == "drop") {
        // Remove all query settings associated with that collection upon collection drop. This is
        // the semantics of index filters.
        planCacheClearFiltersToRemoveAllQuerySettings(conn, {planCacheClearFilters: cmdObj.drop})

        // Drop the collection.
        return clientFunction.apply(conn, makeFuncArgs(cmdObj));
    } else if (cmdName == "aggregate") {
        let response = clientFunction.apply(conn, makeFuncArgs(cmdObj));
        return processAggregateResponse(cmdObj, response);
    } else if (cmdName == "explain") {
        const response = clientFunction.apply(conn, makeFuncArgs(cmdObj));
        return populateIndexFilterSetIfQuerySettingsArePresent(response);
    } else if (cmdName == "planCacheSetFilter") {
        return planCacheSetFilterToSetQuerySettings(conn, dbName, cmdObj);
    } else if (cmdName == "planCacheClearFilters") {
        return planCacheClearFiltersToRemoveAllQuerySettings(conn, cmdObj);
    } else if (cmdName == "planCacheListFilters") {
        return planCacheListFiltersToDollarQuerySettings(conn, cmdObj);
    } else {
        return clientFunction.apply(conn, makeFuncArgs(cmdObj));
    }
}

// Override the default runCommand with our custom version.
OverrideHelpers.overrideRunCommand(runCommandOverride);

// Always apply the override if a test spawns a parallel shell.
OverrideHelpers.prependOverrideInParallelShell("jstests/libs/override_methods/rerun_queries.js");
