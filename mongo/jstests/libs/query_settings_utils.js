/**
 * Utility class for testing query settings.
 */
import {
    getEngine,
    getPlanStages,
    getQueryPlanners,
    getWinningPlan,
    getWinningPlanFromExplain
} from "jstests/libs/analyze_plan.js";
import {checkSbeFullyEnabled} from "jstests/libs/sbe_util.js";

export class QuerySettingsUtils {
    /**
     * Create a query settings utility class.
     */
    constructor(db, collName) {
        this.db = db;
        this.adminDB = this.db.getSiblingDB("admin");
        this.collName = collName;
    }

    /**
     * Makes an query instance of the find command.
     */
    makeFindQueryInstance(findObj) {
        return {find: this.collName, $db: this.db.getName(), ...findObj};
    }

    /**
     * Makes a query instance of the distinct command.
     */
    makeDistinctQueryInstance(distinctObj) {
        return {distinct: this.collName, $db: this.db.getName(), ...distinctObj};
    }

    /**
     * Makes a query instance of the aggregate command with an optional pipeline clause.
     */
    makeAggregateQueryInstance(aggregateObj, collectionless = false) {
        return {
            aggregate: collectionless ? 1 : this.collName,
            $db: this.db.getName(),
            ...aggregateObj
        };
    }

    /**
     * Makes a QueryShapeConfiguration object without the QueryShapeHash.
     */
    makeQueryShapeConfiguration(settings, representativeQuery) {
        return {settings, representativeQuery};
    }

    /**
     * Return query settings for the current tenant without query hashes.
     */
    getQuerySettings(opts = {}) {
        return this.adminDB
            .aggregate([
                {$querySettings: opts},
                {$project: {queryShapeHash: 0}},
                {$sort: {representativeQuery: 1}},
            ])
            .toArray();
    }

    /**
     * Return queryShapeHash for a given query from querySettings.
     */
    getQueryHashFromQuerySettings(shape) {
        print("looking for hash for the shape: \n" + tojson(shape))
        const settings = this.adminDB
                             .aggregate([
                                 {$querySettings: {showDebugQueryShape: true}},
                                 {$project: {"debugQueryShape.cmdNs": 0}},
                                 {$match: {debugQueryShape: shape}},
                             ])
                             .toArray();
        if (settings.length == 0) {
            const allSettings = this.adminDB
                                    .aggregate([
                                        {$querySettings: {showDebugQueryShape: true}},
                                        {$project: {"debugQueryShape.cmdNs": 0}},
                                    ])
                                    .toArray();
            print("Couldn't find any. All settings:\n" + tojson(allSettings));
            return undefined;
        }
        assert.eq(settings.length, 1);
        return settings[0].queryShapeHash;
    }

    /**
     * Return the query settings section of the server status.
     */
    getQuerySettingsServerStatus() {
        return this.db.runCommand({serverStatus: 1}).querySettings;
    }

    /**
     * Helper function to assert equality of QueryShapeConfigurations. In order to ease the
     * assertion logic, 'queryShapeHash' field is removed from the QueryShapeConfiguration prior
     * to assertion.
     *
     * Since in sharded clusters the query settings may arrive with a delay to the mongos, the
     * assertion is done via 'assert.soon'.
     *
     * The settings list is not expected to be in any particular order.
     */
    assertQueryShapeConfiguration(expectedQueryShapeConfigurations, shouldRunExplain = true) {
        assert.soon(
            () => {
                let currentQueryShapeConfigurationWo = this.getQuerySettings();
                currentQueryShapeConfigurationWo.sort(bsonWoCompare);
                let expectedQueryShapeConfigurationWo = [...expectedQueryShapeConfigurations];
                expectedQueryShapeConfigurationWo.sort(bsonWoCompare);
                return bsonWoCompare(currentQueryShapeConfigurationWo,
                                     expectedQueryShapeConfigurationWo) == 0;
            },
            "current query settings = " + tojson(this.getQuerySettings()) +
                ", expected query settings = " + tojson(expectedQueryShapeConfigurations));

        if (shouldRunExplain) {
            for (let {representativeQuery, settings} of expectedQueryShapeConfigurations) {
                this.assertExplainQuerySettings(representativeQuery, settings);
            }
        }
    }

    /**
     * Asserts that the explain output for 'query' contains 'expectedQuerySettings'.
     */
    assertExplainQuerySettings(query, expectedQuerySettings) {
        // Pass query without the $db field to explain command, because it injects the $db field
        // inside the query before processing.
        const queryWithoutDollarDb = this.withoutDollarDB(query);
        const explain = (() => {
            if (query.find || query.distinct) {
                return assert.commandWorked(this.db.runCommand({explain: queryWithoutDollarDb}));
            } else if (query.aggregate) {
                return assert.commandWorked(
                    this.db.runCommand({explain: {...queryWithoutDollarDb, cursor: {}}}));
            } else {
                assert(false,
                       `Attempting to run explain for unknown query type. Query: ${tojson(query)}`);
            }
        })();
        if (explain) {
            getQueryPlanners(explain).forEach(queryPlanner => {
                assert.docEq(expectedQuerySettings, queryPlanner.querySettings, queryPlanner);
            });
        }
    }

    /**
     * Asserts that after executing 'command' the most recent query plan from cache would have
     * 'querySettings' set.
     */
    assertQuerySettingsInCacheForCommand(command, querySettings) {
        // Single solution plans are not cached in classic, therefore do not perform plan cache
        // checks for classic.
        if (!checkSbeFullyEnabled(this.db)) {
            return;
        }

        // Clear the plan cache before running any queries.
        this.db[this.collName].getPlanCache().clear();

        // Take the newest plan cache entry (based on 'timeOfCreation' sorting) and ensure that it
        // contains the 'settings'.
        assert.commandWorked(this.db.runCommand(command));
        const planCacheStatsAfterRunningCmd =
            this.db[this.collName].aggregate([{$planCacheStats: {}}]).toArray();
        assert.gte(planCacheStatsAfterRunningCmd.length,
                   1,
                   "Expecting at least 1 entry in query plan cache");
        planCacheStatsAfterRunningCmd.forEach(
            plan => assert.docEq(plan.querySettings, querySettings, plan));
    }

    assertIndexScanStage(cmd, expectedIndex) {
        const explain = assert.commandWorked(this.db.runCommand({explain: cmd}));
        const winningPlan = getWinningPlan(explain.queryPlanner);
        const ixscanStages = getPlanStages(winningPlan, "IXSCAN");
        assert.gte(ixscanStages.length, 1, explain);
        if (expectedIndex == undefined) {
            // Don't verify index name.
            return;
        }
        for (const ixscanStage of ixscanStages) {
            assert.docEq(ixscanStage.keyPattern, expectedIndex, explain);
        }
    }

    assertDistinctScanStage(cmd, expectedIndex) {
        const explain = assert.commandWorked(this.db.runCommand({explain: cmd}));
        const winningPlan = getWinningPlan(explain.queryPlanner);
        const ixscanStages = getPlanStages(winningPlan, "DISTINCT_SCAN");
        assert.gte(ixscanStages.length, 1, explain);
        if (expectedIndex == undefined) {
            // Don't verify index name.
            return;
        }
        for (const ixscanStage of ixscanStages) {
            assert.docEq(ixscanStage.keyPattern, expectedIndex, explain);
        }
    }

    assertCollScanStage(cmd, allowedDirections) {
        const explain = assert.commandWorked(this.db.runCommand({explain: cmd}));
        const winningPlan = getWinningPlan(explain.queryPlanner);
        const collscanStages = getPlanStages(winningPlan, "COLLSCAN");
        assert.gte(collscanStages.length, 1, explain);
        for (const collscanStage of collscanStages) {
            assert(allowedDirections.includes(collscanStage.direction), explain);
        }
    }

    /**
     * Remove all query settings for the current tenant.
     */
    removeAllQuerySettings() {
        let settingsArray = this.getQuerySettings();
        while (settingsArray.length > 0) {
            const setting = settingsArray.pop();
            // Some suites may transparently retry this request if it fails due to e.g., step down.
            // However, the settings may already be modified. The retry will then fail with:
            //  "A matching query settings entry does not exist"
            // despite the call actually succeeding.
            // Since we immediately check the correct set of settings exists, this test still
            // verifies the correct behaviour, even without an assert.commandWorked here.
            this.adminDB.runCommand({removeQuerySettings: setting.representativeQuery});
            // Check that the given setting has indeed been removed.
            this.assertQueryShapeConfiguration(settingsArray);
        }
    }

    /**
     * Helper method for setting & removing query settings for testing purposes. Accepts a
     * 'runTest' anonymous function which will be executed once the provided query settings have
     * been propagated throughout the cluster.
     */
    withQuerySettings(representativeQuery, settings, runTest) {
        assert.commandWorked(
            db.adminCommand({setQuerySettings: representativeQuery, settings: settings}));
        this.assertQueryShapeConfiguration(
            [this.makeQueryShapeConfiguration(settings, representativeQuery)]);
        runTest();
        this.removeAllQuerySettings();
    }

    withoutDollarDB(cmd) {
        const {$db: _, ...rest} = cmd;
        return rest;
    }

    /**
     * Asserts that the expected engine is run on the input query and settings,
     * including overriding a provided internalQueryFrameworkControl query knob.
     */
    testQueryFramework({query, settings, knob, expectedEngine}) {
        // Ensure that query settings cluster parameter is empty.
        this.assertQueryShapeConfiguration([]);

        // Apply the provided settings for the query.
        if (settings) {
            assert.commandWorked(
                this.db.adminCommand({setQuerySettings: query, settings: settings}));
            // Wait until the settings have taken effect.
            const expectedConfiguration = [this.makeQueryShapeConfiguration(settings, query)];
            this.assertQueryShapeConfiguration(expectedConfiguration);
        }

        // Set framework control knob
        if (knob) {
            assert.commandWorked(
                this.db.adminCommand({setParameter: 1, internalQueryFrameworkControl: knob}))
        }
        const withoutDollarDB = query.aggregate ? {...this.withoutDollarDB(query), cursor: {}}
                                                : this.withoutDollarDB(query);
        const explain = assert.commandWorked(this.db.runCommand({explain: withoutDollarDB}));
        const engine = getEngine(explain);
        assert.eq(
            engine, expectedEngine, `Expected engine to be ${expectedEngine} but found ${engine}`);

        // If a hinted index exists, assert it was used.
        if (query.hint) {
            const winningPlan = getWinningPlanFromExplain(explain);
            const ixscanStage = getPlanStages(winningPlan, "IXSCAN")[0];
            assert.eq(query.hint, ixscanStage.keyPattern, winningPlan);
        }

        this.removeAllQuerySettings();
    }
}
