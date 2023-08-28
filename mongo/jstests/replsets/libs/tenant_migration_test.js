/**
 * Wrapper around ReplSetTest for testing tenant migration behavior.
 */

import {arrayEq} from "jstests/aggregation/extras/utils.js";
import {
    checkTenantDBHashes,
    createTenantMigrationDonorRoleIfNotExist,
    createTenantMigrationRecipientRoleIfNotExist,
    getExternalKeys,
    getTenantMigrationAccessBlocker,
    isMigrationCompleted,
    isNamespaceForTenant,
    isShardMergeEnabled,
    kProtocolMultitenantMigrations,
    kProtocolShardMerge,
    makeMigrationCertificatesForTest,
    makeX509OptionsForTest,
    runDonorStartMigrationCommand,
    runTenantMigrationCommand,
} from "jstests/replsets/libs/tenant_migration_util.js";

function loadDummyData() {
    const numDocs = 20;
    const testData = [];
    for (let i = 0; i < numDocs; ++i) {
        testData.push({_id: i, x: i});
    }
    return testData;
}

function buildErrorMsg(migrationId, expectedState, expectedAccessState, configDoc, recipientMtab) {
    return tojson({migrationId, expectedState, expectedAccessState, configDoc, recipientMtab});
}

/**
 * This fixture allows the user to optionally pass in a custom ReplSetTest for the donor and
 * recipient replica sets, to be used for the test.
 *
 * If the caller does not provide their own replica set, a two node replset will be initialized
 * instead, with all nodes running the latest version.
 */
export class TenantMigrationTest {
    /**
     * Takes in the response to the donarStartMigration command and asserts the command
     * works and the state is 'committed'.
     */
    static assertCommitted(stateRes) {
        assert.commandWorked(stateRes);
        assert.eq(stateRes.state, TenantMigrationTest.DonorState.kCommitted, tojson(stateRes));
        return stateRes;
    }

    /**
     * Takes in the response to the donarStartMigration command and asserts the command
     * works and the state is 'aborted', with optional errorCode.
     */
    static assertAborted(stateRes, errorCode) {
        assert.commandWorked(stateRes);
        assert.eq(stateRes.state, TenantMigrationTest.DonorState.kAborted, tojson(stateRes));
        if (errorCode !== undefined) {
            assert.eq(stateRes.abortReason.code, errorCode, tojson(stateRes));
        }
        return stateRes;
    }

    /**
     * Make a new TenantMigrationTest
     *
     * @param {string} [name] the name of the replica sets
     * @param {string} [protocol] the migration protocol to use, either "multitenant migrations" or
     *     "shard merge". If no value is provided, will default to "shard merge" if the shard merge
     *     feature flag is enabled, otherwise will be set to "multitenant migrations"
     * @param {boolean} [enableRecipientTesting] whether recipient would actually migrate tenant
     *     data
     * @param {Object} [donorRst] the ReplSetTest instance to adopt for the donor
     * @param {Object} [recipientRst] the ReplSetTest instance to adopt for the recipient
     * @param {Object} [sharedOptions] an object that can contain 'nodes' <number>, the number of
     *     nodes each RST will contain, and 'setParameter' <object>, an object with various server
     *     parameters.
     * @param {boolean} [allowDonorReadAfterMigration] whether donor would allow reads after a
     *     committed migration.
     * @param {boolean} [initiateRstWithHighElectionTimeout] whether donor and recipient replica
     *     sets should be initiated with high election timeout.
     * @param {boolean} [quickGarbageCollection] whether to set a low garbageCollectionDelayMS.
     * @param {string} [insertDataForTenant] create dummy data in <tenantId>_test database.
     */
    constructor({
        name = "TenantMigrationTest",
        protocol = "",
        enableRecipientTesting = true,
        donorRst,
        recipientRst,
        sharedOptions = {},
        // Default this to true so it is easier for data consistency checks.
        allowStaleReadsOnDonor = true,
        initiateRstWithHighElectionTimeout = true,
        quickGarbageCollection = false,
        insertDataForTenant,
        optimizeMigrations = true,
    }) {
        this._donorPassedIn = (donorRst !== undefined);
        this._recipientPassedIn = (recipientRst !== undefined);
        const migrationX509Options = makeX509OptionsForTest();
        const nodes = sharedOptions.nodes || 2;
        const setParameterOpts = sharedOptions.setParameter || {};
        if (optimizeMigrations) {
            // A tenant migration recipient's `OplogFetcher` uses aggregation which does not support
            // tailable awaitdata cursors. For aggregation commands `OplogFetcher` will default to
            // half the election timeout (e.g: 5 seconds) between getMores. That wait is largely
            // unnecessary.
            setParameterOpts["failpoint.setSmallOplogGetMoreMaxTimeMS"] =
                tojson({"mode": "alwaysOn"});
        }
        if (quickGarbageCollection) {
            setParameterOpts.tenantMigrationGarbageCollectionDelayMS = 0;
            setParameterOpts.ttlMonitorSleepSecs = 1;
        }

        /**
         * Creates a ReplSetTest instance. The repl set will have 2 nodes if not otherwise
         * specified.
         */
        function performSetUp(isDonor) {
            if (TestData.logComponentVerbosity) {
                setParameterOpts["logComponentVerbosity"] =
                    tojsononeline(TestData.logComponentVerbosity);
            }

            if (!(isDonor || enableRecipientTesting)) {
                setParameterOpts["failpoint.returnResponseOkForRecipientSyncDataCmd"] =
                    tojson({mode: 'alwaysOn'});
            }

            if (allowStaleReadsOnDonor) {
                setParameterOpts["failpoint.tenantMigrationDonorAllowsNonTimestampedReads"] =
                    tojson({mode: 'alwaysOn'});
            }

            const nodeOptions =
                isDonor ? migrationX509Options.donor : migrationX509Options.recipient;
            nodeOptions["setParameter"] = setParameterOpts;

            const rstName = `${name}_${(isDonor ? "donor" : "recipient")}`;
            const rst = new ReplSetTest({name: rstName, nodes, serverless: true, nodeOptions});
            rst.startSet();
            if (initiateRstWithHighElectionTimeout) {
                rst.initiateWithHighElectionTimeout();
            } else {
                rst.initiate();
            }

            return rst;
        }

        this._donorRst = this._donorPassedIn ? donorRst : performSetUp(true /* isDonor */);
        this._recipientRst =
            this._recipientPassedIn ? recipientRst : performSetUp(false /* isDonor */);

        // If we don't pass "protocol" and shard merge is enabled, we set the protocol to
        // "shard merge". Otherwise, the provided protocol is used, which defaults to
        // "multitenant migrations" if not provided.
        if (protocol === "" && isShardMergeEnabled(this.getDonorPrimary().getDB("admin"))) {
            this.protocol = kProtocolShardMerge;
        } else if (protocol === "") {
            this.protocol = kProtocolMultitenantMigrations;
        }

        this.configRecipientsNs = this.protocol === kProtocolShardMerge
            ? TenantMigrationTest.kConfigShardMergeRecipientsNS
            : TenantMigrationTest.kConfigRecipientsNS;

        this._donorRst.asCluster(this._donorRst.nodes, () => {
            this._donorRst.getPrimary();
            this._donorRst.awaitReplication();
            createTenantMigrationRecipientRoleIfNotExist(this._donorRst);
        });

        this._recipientRst.asCluster(this._recipientRst.nodes, () => {
            this._recipientRst.getPrimary();
            this._recipientRst.awaitReplication();
            createTenantMigrationDonorRoleIfNotExist(this._recipientRst);
        });

        // Shard Merge installs TenantRecipientAccessBlockers only for tenants with data, so most
        // tests require some data.
        if (insertDataForTenant !== undefined) {
            this.insertDonorDB(`${insertDataForTenant}_test`, "test");
        }
    }

    /**
     * Inserts documents into the specified collection on the donor primary.
     */
    insertDonorDB(dbName, collName, data = loadDummyData()) {
        jsTestLog(`Inserting data into collection ${collName} of DB ${dbName} on the donor`);
        const primary = this._donorRst.getPrimary();
        const db = primary.getDB(dbName);
        const res = assert.commandWorked(
            db.runCommand({insert: collName, documents: data, writeConcern: {w: 'majority'}}));
        jsTestLog(`Inserted with w: majority, opTime ${tojson(res.operationTime)}`);
    }

    /**
     * Inserts documents into the specified collection on the recipient primary.
     */
    insertRecipientDB(dbName, collName, data = loadDummyData()) {
        jsTestLog(`Inserting data into collection ${collName} of DB ${dbName} on the recipient`);
        const primary = this._recipientRst.getPrimary();
        const db = primary.getDB(dbName);
        const res = assert.commandWorked(
            db.runCommand({insert: collName, documents: data, writeConcern: {w: 'majority'}}));
        jsTestLog(`Inserted with w: majority, opTime ${tojson(res.operationTime)}`);
    }

    /**
     * Runs a tenant migration with the given migration options and waits for the migration to
     * be committed or aborted.
     *
     * Returns the result of the initial donorStartMigration if it was unsuccessful. Otherwise,
     * returns the command response containing the migration state on the donor after the
     * migration has completed.
     */
    runMigration(migrationOpts, opts = {}) {
        const {retryOnRetryableErrors = false, automaticForgetMigration = true} = opts;

        const startRes = this.startMigration(migrationOpts, opts);
        if (!startRes.ok) {
            return startRes;
        }

        const completeRes = this.waitForMigrationToComplete(migrationOpts, retryOnRetryableErrors);

        if (automaticForgetMigration &&
            (completeRes.state === TenantMigrationTest.State.kCommitted ||
             completeRes.state === TenantMigrationTest.State.kAborted)) {
            jsTestLog(`Automatically forgetting ${completeRes.state} migration with migrationId: ${
                migrationOpts.migrationIdString}`);
            this.forgetMigration(migrationOpts.migrationIdString);
        }

        return completeRes;
    }

    /**
     * Starts a tenant migration by running the 'donorStartMigration' command once.
     *
     * Returns the result of the 'donorStartMigration' command.
     */
    startMigration(migrationOpts, {retryOnRetryableErrors = false} = {}) {
        return this.runDonorStartMigration(migrationOpts, {retryOnRetryableErrors});
    }

    /**
     * Waits for a migration to complete by continuously polling the donor primary with
     * 'donorStartMigration' until the returned state is committed or aborted. Must be used with
     * startMigration, after the migration has been started for the specified tenantId.
     *
     * Returns the result of the last 'donorStartMigration' command executed.
     */
    waitForMigrationToComplete(migrationOpts,
                               retryOnRetryableErrors = false,
                               forgetMigration = false) {
        // Assert that the migration has already been started.
        assert(this.getDonorPrimary().getCollection(TenantMigrationTest.kConfigDonorsNS).findOne({
            _id: UUID(migrationOpts.migrationIdString)
        }));

        const donorStartReply = this.runDonorStartMigration(
            migrationOpts, {waitForMigrationToComplete: true, retryOnRetryableErrors});
        if (!forgetMigration) {
            return donorStartReply;
        }

        this.forgetMigration(migrationOpts.migrationIdString, retryOnRetryableErrors);
        return donorStartReply;
    }

    /**
     * Executes the 'donorStartMigration' command on the donor primary.
     *
     * This will return on the first successful command if 'waitForMigrationToComplete' is
     * set to false. Otherwise, it will continuously poll the donor primary until the
     * migration has been committed or aborted.
     *
     * If 'retryOnRetryableErrors' is set, this function will retry if the command fails
     * with a NotPrimary or network error.
     */
    runDonorStartMigration({
        migrationIdString,
        tenantId,
        protocol,
        tenantIds,
        recipientConnectionString = this._recipientRst.getURL(),
        readPreference = {mode: "primary"},
        donorCertificateForRecipient,
        recipientCertificateForDonor,
    },
                           opts = {}) {
        const migrationCertificates = makeMigrationCertificatesForTest();
        donorCertificateForRecipient =
            donorCertificateForRecipient || migrationCertificates.donorCertificateForRecipient;
        recipientCertificateForDonor =
            recipientCertificateForDonor || migrationCertificates.recipientCertificateForDonor;

        const {
            waitForMigrationToComplete = false,
            retryOnRetryableErrors = false,
        } = opts;

        const migrationOpts = {
            migrationId: UUID(migrationIdString),
            tenantId,
            tenantIds,
            recipientConnectionString,
            readPreference,
            protocol
        };

        const stateRes = runDonorStartMigrationCommand(migrationOpts, this.getDonorRst(), {
            retryOnRetryableErrors,
            shouldStopFunc: stateRes =>
                (!waitForMigrationToComplete || isMigrationCompleted(stateRes))
        });

        // If the migration has been successfully committed, check the db hashes for the tenantId
        // between the donor and recipient.
        if (stateRes.state === TenantMigrationTest.State.kCommitted) {
            checkTenantDBHashes(
                {donorRst: this.getDonorRst(), recipientRst: this.getRecipientRst(), tenantId});
        }

        return stateRes;
    }

    /**
     * Runs the donorForgetMigration command with the given migrationId and returns the response.
     *
     * If 'retryOnRetryableErrors' is set, this function will retry if the command fails with a
     * NotPrimary or network error.
     */
    forgetMigration(migrationIdString, retryOnRetryableErrors = false) {
        const cmdObj = {donorForgetMigration: 1, migrationId: UUID(migrationIdString)};
        const res = runTenantMigrationCommand(cmdObj, this.getDonorRst(), {retryOnRetryableErrors});

        // If the command succeeded, we expect that the migration is marked garbage collectable on
        // the donor and the recipient. Check the state docs for expireAt, check that the oplog
        // buffer collection has been dropped, and external keys have ttlExpiresAt.
        if (res.ok) {
            const donorPrimary = this.getDonorPrimary();
            const recipientPrimary = this.getRecipientPrimary();

            const donorStateDoc =
                donorPrimary.getCollection(TenantMigrationTest.kConfigDonorsNS).findOne({
                    _id: UUID(migrationIdString)
                });

            const recipientStateDoc =
                recipientPrimary.getCollection(this.configRecipientsNs).findOne({
                    _id: UUID(migrationIdString)
                });

            if (donorStateDoc) {
                assert(donorStateDoc.expireAt);
            }
            if (recipientStateDoc) {
                assert(recipientStateDoc.expireAt);
            }

            const configDBCollections = recipientPrimary.getDB('config').getCollectionNames();
            assert(!configDBCollections.includes(`repl.migration.oplog_${migrationIdString}`),
                   configDBCollections);

            this.getDonorRst().asCluster(donorPrimary, () => {
                const donorKeys = getExternalKeys(donorPrimary, UUID(migrationIdString));
                if (donorKeys.length) {
                    donorKeys.forEach(key => {
                        assert(key.hasOwnProperty("ttlExpiresAt"), tojson(key));
                    });
                }
            });

            this.getRecipientRst().asCluster(recipientPrimary, () => {
                const recipientKeys = getExternalKeys(recipientPrimary, UUID(migrationIdString));
                if (recipientKeys.length) {
                    recipientKeys.forEach(key => {
                        assert(key.hasOwnProperty("ttlExpiresAt"), tojson(key));
                    });
                }
            });
        }

        return res;
    }

    /**
     * Runs the donorAbortMigration command with the given migration options and returns the
     * response.
     */
    tryAbortMigration(migrationOpts, retryOnRetryableErrors = false) {
        const cmdObj = {
            donorAbortMigration: 1,
            migrationId: UUID(migrationOpts.migrationIdString),
        };
        return runTenantMigrationCommand(cmdObj, this.getDonorRst(), {retryOnRetryableErrors});
    }

    /**
     * Asserts that durable and in-memory state for the migration 'migrationId' and 'tenantId' is
     * eventually deleted from the given nodes.
     */
    waitForMigrationGarbageCollection(migrationId, tenantId, donorNodes, recipientNodes) {
        donorNodes = donorNodes || this._donorRst.nodes;
        recipientNodes = recipientNodes || this._recipientRst.nodes;

        if (typeof migrationId === "string") {
            migrationId = UUID(migrationId);
        }

        donorNodes.forEach(node => {
            const configDonorsColl = node.getCollection(TenantMigrationTest.kConfigDonorsNS);
            assert.soon(() => 0 === configDonorsColl.count({_id: migrationId}), tojson(node));

            let mtab;
            assert.soon(() => {
                mtab = this.getTenantMigrationAccessBlocker({donorNode: node, tenantId});
                return !mtab;
            }, tojson(mtab));
        });

        recipientNodes.forEach(node => {
            const configRecipientsColl = node.getCollection(this.configRecipientsNs);
            assert.soon(() => 0 === configRecipientsColl.count({_id: migrationId}), tojson(node));

            let mtab;
            assert.soon(() => {
                mtab =
                    this.getTenantMigrationAccessBlocker({recipientNode: node, tenantId: tenantId});
                return !mtab;
            }, tojson(mtab));
        });
    }

    /**
     * Asserts that the migration 'migrationId' and 'tenantId' eventually goes to the
     * expected state on all the given donor nodes.
     */
    waitForDonorNodesToReachState(nodes, migrationId, tenantId, expectedState) {
        nodes.forEach(node => {
            assert.soon(
                () => this.isDonorNodeInExpectedState(node, migrationId, tenantId, expectedState));
        });
    }

    /**
     * Asserts that the migration 'migrationId' and 'tenantId' is in the expected state on all the
     * given donor nodes.
     */
    assertDonorNodesInExpectedState(nodes, migrationId, tenantId, expectedState) {
        nodes.forEach(node => {
            assert(this.isDonorNodeInExpectedState(node, migrationId, tenantId, expectedState));
        });
    }

    /**
     * Returns true if the durable and in-memory state for the migration 'migrationId' and
     * 'tenantId' is in the expected state, and false otherwise.
     */
    isDonorNodeInExpectedState(node, migrationId, tenantId, expectedState) {
        const configDonorsColl =
            this.getDonorPrimary().getCollection(TenantMigrationTest.kConfigDonorsNS);
        const configDoc = configDonorsColl.findOne({_id: migrationId});
        if (!configDoc || configDoc.state !== expectedState) {
            return false;
        }

        const expectedAccessState = (expectedState === TenantMigrationTest.State.kCommitted)
            ? TenantMigrationTest.DonorAccessState.kReject
            : TenantMigrationTest.DonorAccessState.kAborted;
        const mtab = this.getTenantMigrationAccessBlocker({donorNode: node, tenantId});
        return (mtab.donor.state === expectedAccessState);
    }

    /**
     * Asserts that the migration 'migrationId' and 'tenantId' eventually goes to the expected state
     * on all the given recipient nodes.
     */
    waitForRecipientNodesToReachState(
        nodes, migrationId, tenantId, expectedState, expectedAccessState) {
        nodes.forEach(node => {
            let result = {};
            assert.soon(
                () => {
                    result = this.isRecipientNodeInExpectedState(
                        {node, migrationId, tenantId, expectedState, expectedAccessState});
                    return result.value;
                },
                () => {
                    return "waitForRecipientNodesToReachState failed: " +
                        buildErrorMsg(migrationId,
                                      expectedState,
                                      expectedAccessState,
                                      result.configDoc,
                                      result.recipientMtab);
                });
        });
    }

    /**
     * Asserts that the migration 'migrationId' and 'tenantId' is in the expected state on all the
     * given recipient nodes.
     */
    assertRecipientNodesInExpectedState({
        nodes,
        migrationId,
        tenantId,
        expectedState,
        expectedAccessState,
    }) {
        nodes.forEach(node => {
            let result = this.isRecipientNodeInExpectedState(
                {node, migrationId, tenantId, expectedState, expectedAccessState});
            assert(result.value, () => {
                return "assertRecipientNodesInExpectedState failed: " +
                    buildErrorMsg(migrationId,
                                  expectedState,
                                  expectedAccessState,
                                  result.configDoc,
                                  result.recipientMtab);
            });
        });
    }

    /**
     * Returns true if the durable and in-memory state for the migration 'migrationId' and
     * 'tenantId' is in the expected state, and false otherwise.
     */
    isRecipientNodeInExpectedState({
        node,
        migrationId,
        tenantId,
        expectedState,
        expectedAccessState,
    }) {
        const configRecipientsColl =
            this.getRecipientPrimary().getCollection(this.configRecipientsNs);
        const configDoc = configRecipientsColl.findOne({_id: migrationId});

        const mtab = this.getTenantMigrationAccessBlocker({recipientNode: node, tenantId});

        let checkStates = () => {
            if (!configDoc || configDoc.state !== expectedState) {
                return false;
            }
            return (mtab.recipient.state === expectedAccessState);
        };

        return {value: checkStates(), configDoc: configDoc, recipientMtab: mtab.recipient};
    }

    /**
     * Verifies that the documents on the recipient primary are correct.
     */
    verifyRecipientDB(
        tenantId, dbName, collName, migrationCommitted = true, data = loadDummyData()) {
        // We should migrate all data regardless of tenant id for shard merge.
        const shouldMigrate = migrationCommitted &&
            (isShardMergeEnabled(this.getRecipientPrimary().getDB("admin")) ||
             isNamespaceForTenant(tenantId, dbName));

        jsTestLog(`Verifying that data in collection ${collName} of DB ${dbName} was ${
            (shouldMigrate ? "" : "not")} migrated to the recipient`);

        const db = this.getRecipientPrimary().getDB(dbName);
        const coll = db.getCollection(collName);

        const findRes = coll.find();
        const numDocsFound = findRes.count();

        if (!shouldMigrate) {
            assert.eq(0,
                      numDocsFound,
                      `Find command on recipient collection ${collName} of DB ${
                          dbName} should return 0 docs, instead has count of ${numDocsFound}`);
            return;
        }

        const numDocsExpected = data.length;
        assert.eq(numDocsExpected,
                  numDocsFound,
                  `Find command on recipient collection ${collName} of DB ${dbName} should return ${
                      numDocsExpected} docs, instead has count of ${numDocsFound}`);

        const docsReturned = findRes.sort({_id: 1}).toArray();
        assert(arrayEq(docsReturned, data),
               () => (`${tojson(docsReturned)} is not equal to ${tojson(data)}`));
    }

    /**
     * Returns the TenantMigrationAccessBlocker serverStatus output for the migration or shard merge
     * for the given node.
     */
    getTenantMigrationAccessBlocker(obj) {
        return getTenantMigrationAccessBlocker(obj);
    }

    /**
     * Returns the TenantMigrationStats on the node.
     */
    getTenantMigrationStats(node) {
        return assert.commandWorked(node.adminCommand({serverStatus: 1})).tenantMigrations;
    }

    /**
     * Returns the donor ReplSetTest.
     */
    getDonorRst() {
        return this._donorRst;
    }

    /**
     * Returns the recipient ReplSetTest.
     */
    getRecipientRst() {
        return this._recipientRst;
    }

    /**
     * Returns the donor's primary.
     */
    getDonorPrimary() {
        return this.getDonorRst().getPrimary();
    }

    /**
     * Returns the recipient's primary.
     */
    getRecipientPrimary() {
        return this.getRecipientRst().getPrimary();
    }

    /**
     * Returns the recipient's connection string.
     */
    getRecipientConnString() {
        return this.getRecipientRst().getURL();
    }

    /**
     * Shuts down the donor and recipient sets, only if they were not passed in as parameters.
     * If they were passed in, the test that initialized them should be responsible for shutting
     * them down.
     */
    stop() {
        if (!this._donorPassedIn)
            this._donorRst.stopSet();
        if (!this._recipientPassedIn)
            this._recipientRst.stopSet();
    }
}

TenantMigrationTest.DonorState = {
    kCommitted: "committed",
    kAborted: "aborted",
    kDataSync: "data sync",
    kBlocking: "blocking",
    kAbortingIndexBuilds: "aborting index builds",
};

TenantMigrationTest.RecipientState = {
    kUninitialized: "uninitialized",
    kStarted: "started",
    kConsistent: "consistent",
    kDone: "done",
    kLearnedFilenames: "learned filenames",
    kCommitted: "committed",
    kAborted: "aborted",
};

TenantMigrationTest.ShardMergeRecipientState = {
    kStarted: "started",
    kLearnedFilenames: "learned filenames",
    kConsistent: "consistent",
    kCommitted: "committed",
    kAborted: "aborted",
};

TenantMigrationTest.RecipientStateEnum =
    Object.keys(TenantMigrationTest.RecipientState).reduce((acc, key, idx) => {
        acc[key] = idx;
        return acc;
    }, {});

TenantMigrationTest.State = TenantMigrationTest.DonorState;

TenantMigrationTest.DonorAccessState = {
    kAllow: "allow",
    kBlockWrites: "blockWrites",
    kBlockWritesAndReads: "blockWritesAndReads",
    kReject: "reject",
    kAborted: "aborted",
};

TenantMigrationTest.RecipientAccessState = {
    kRejectReadsAndWrites: "rejectReadsAndWrites",
    kRejectReadsBefore: "rejectReadsBefore"
};

TenantMigrationTest.kConfigDonorsNS = "config.tenantMigrationDonors";
TenantMigrationTest.kConfigRecipientsNS = "config.tenantMigrationRecipients";
TenantMigrationTest.kConfigShardMergeRecipientsNS = "config.shardMergeRecipients";
