import {FeatureFlagUtil} from "jstests/libs/feature_flag_util.js";

/**
 * Utilities for testing tenant migrations.
 */
export const kExternalKeysNs = "config.external_validation_keys";
export const kProtocolShardMerge = "shard merge";
export const kProtocolMultitenantMigrations = "multitenant migrations";

/**
 * Crafts a tenant database name.
 */
export function makeTenantDB(tenantId, dbName) {
    return `${tenantId}_${dbName}`;
}

/**
 * Returns true if feature flag 'featureFlagShardMerge' is enabled, false otherwise.
 */
export function isShardMergeEnabled(db) {
    return FeatureFlagUtil.isEnabled(db, "ShardMerge");
}

/**
 * Checks the FCV and the command object passed to see if tenantIds should be set for shard
 * merge.
 */
export function shouldUseMergeTenantIds(db) {
    const fcvDoc =
        assert.commandWorked(db.adminCommand({getParameter: 1, featureCompatibilityVersion: 1}));

    return MongoRunner.compareBinVersions(fcvDoc.featureCompatibilityVersion.version, "6.3") >= 0;
}

/**
 * Returns the external keys for the given migration id.
 */
export function getExternalKeys(conn, migrationId) {
    return conn.getCollection(kExternalKeysNs).find({migrationId}).toArray();
}

/**
 * Returns X509 options for ReplSetTest with the given certificate-key file and CA pem file.
 */
export function makeX509Options(certPemFile, caPemFile = "jstests/libs/ca.pem") {
    return {
        // When the global sslMode is preferSSL or requireSSL, the transport layer would do the
        // SSL handshake regardless of the specified sslMode for the connection. So we use a
        // allowTLS to verify that the donor and recipient use SSL to authenticate to each other
        // regardless of the global sslMode.
        tlsMode: "allowTLS",
        tlsCertificateKeyFile: certPemFile,
        tlsCAFile: caPemFile,
        tlsAllowInvalidHostnames: ''
    };
}

/**
 * Returns an object containing the certificate and private key extracted from the given
 * pem file.
 */
export function getCertificateAndPrivateKey(pemFile) {
    const lines = cat(pemFile);
    const certificate =
        lines.match(new RegExp("-*BEGIN CERTIFICATE-*\\n(.*\\n)*-*END CERTIFICATE-*\\n"))[0];
    const privateKey =
        lines.match(new RegExp("-*BEGIN PRIVATE KEY-*\\n(.*\\n)*-*END PRIVATE KEY-*\\n"))[0];
    return {certificate, privateKey};
}

/**
 * Returns an object containing the donor and recipient ReplSetTest X509 options for tenant
 * migration testing.
 */
export function makeX509OptionsForTest() {
    return {
        donor: makeX509Options("jstests/libs/rs0.pem"),
        recipient: makeX509Options("jstests/libs/rs1.pem")
    };
}

/**
 * Returns an object containing the donor and recipient's certificate and private key for
 * tenant migration testing.
 */
export function makeMigrationCertificatesForTest() {
    return {
        donorCertificateForRecipient:
            getCertificateAndPrivateKey("jstests/libs/tenant_migration_donor.pem"),
        recipientCertificateForDonor:
            getCertificateAndPrivateKey("jstests/libs/tenant_migration_recipient.pem")
    };
}

/**
 * Takes in the response to the donorStartMigration command and returns true if the state is
 * "committed" or "aborted".
 */
export function isMigrationCompleted(res) {
    return res.state === "committed" || res.state === "aborted";
}

/**
 * Runs the donorStartMigration command with the given migration options
 * until the migration commits or aborts, or until the command fails. Returns the last command
 * response.
 *
 * If 'retryOnRetryableErrors' is set, this function will retry if the command fails with a
 * retryable error.
 *
 * Only use when it is necessary to run the donorStartMigration command in its own thread. For
 * all other use cases, please consider the runMigration() function in the TenantMigrationTest
 * fixture.
 */
export async function runMigrationAsync(migrationOpts, donorRstArgs, opts = {}) {
    const {isMigrationCompleted, makeMigrationCertificatesForTest, runDonorStartMigrationCommand} =
        await import("jstests/replsets/libs/tenant_migration_util.js");
    const {createRst} = await import("jstests/replsets/rslib.js");

    const {
        retryOnRetryableErrors = false,
    } = opts;

    const donorRst = createRst(donorRstArgs, retryOnRetryableErrors);
    const migrationCertificates = makeMigrationCertificatesForTest();

    return runDonorStartMigrationCommand(
        {
            migrationId: UUID(migrationOpts.migrationIdString),
            tenantId: migrationOpts.tenantId,
            tenantIds: eval(migrationOpts.tenantIds),
            recipientConnectionString: migrationOpts.recipientConnString,
            readPreference: migrationOpts.readPreference ||
                {
                    mode: "primary"
                }
        },
        donorRst,
        {retryOnRetryableErrors, shouldStopFunc: isMigrationCompleted});
}

/**
 * Runs the donorForgetMigration command with the given migrationId and returns the response.
 *
 * If 'retryOnRetryableErrors' is set, this function will retry if the command fails with a
 * retryable error.
 *
 * Only use when it is necessary to run the donorForgetMigration command in its own thread. For
 * all other use cases, please consider the forgetMigration() function in the
 * TenantMigrationTest fixture.
 */
export async function forgetMigrationAsync(
    migrationIdString, donorRstArgs, retryOnRetryableErrors = false) {
    const {runTenantMigrationCommand} =
        await import("jstests/replsets/libs/tenant_migration_util.js");
    const {createRst} = await import("jstests/replsets/rslib.js");

    const donorRst = createRst(donorRstArgs, retryOnRetryableErrors);
    const cmdObj = {donorForgetMigration: 1, migrationId: UUID(migrationIdString)};
    return runTenantMigrationCommand(cmdObj, donorRst, {retryOnRetryableErrors});
}

/**
 * Runs the donorAbortMigration command with the given migration options and returns the
 * response.
 *
 * If 'retryOnRetryableErrors' is set, this function will retry if the command fails with a
 * retryable error.
 *
 * Only use when it is necessary to run the donorAbortMigration command in its own thread. For
 * all other use cases, please consider the tryAbortMigration() function in the
 * TenantMigrationTest fixture.
 */
export async function tryAbortMigrationAsync(
    migrationOpts, donorRstArgs, retryOnRetryableErrors = false) {
    const {runTenantMigrationCommand} =
        await import("jstests/replsets/libs/tenant_migration_util.js");
    const {createRst} = await import("jstests/replsets/rslib.js");

    const donorRst = createRst(donorRstArgs, retryOnRetryableErrors);
    const cmdObj = {
        donorAbortMigration: 1,
        migrationId: UUID(migrationOpts.migrationIdString),
    };
    return runTenantMigrationCommand(cmdObj, donorRst, {retryOnRetryableErrors});
}

/**
 * Runs the donorStartMigration command against the primary of the provided replica set. Will
 * automatically assign the correct 'protocol' and 'tenantId'/'tenantIds' based on the provided
 * 'protocol' and/or currently enabled feature flags.
 */
export function runDonorStartMigrationCommand(migrationOpts, rst, {
    retryOnRetryableErrors = false,
    shouldStopFunc = () => true,
} = {}) {
    // If we don't pass "protocol" and shard merge is enabled, we set the protocol to
    // "shard merge". Otherwise, the provided protocol is used, which defaults to
    // "multitenant migrations" if not provided.
    const db = rst.getPrimary().getDB("admin");
    const cmd = Object.assign({donorStartMigration: 1}, migrationOpts);
    if (cmd["protocol"] === undefined && isShardMergeEnabled(db)) {
        if (shouldUseMergeTenantIds(db)) {
            cmd.tenantIds = cmd.tenantIds || [ObjectId(cmd.tenantId)];
        }

        delete cmd.tenantId;
        cmd.protocol = kProtocolShardMerge;
    } else if (cmd["protocol"] == kProtocolShardMerge) {
        delete cmd.tenantId;
    } else {
        delete cmd.tenantIds;
    }

    return runTenantMigrationCommand(cmd, rst, {retryOnRetryableErrors, shouldStopFunc});
}

/**
 * Runs the given tenant migration command against the primary of the given replica set until
 * the command succeeds or fails with a non-retryable error (if 'retryOnRetryableErrors' is
 * true) or until 'shouldStopFunc' returns true. Returns the last response.
 */
export function runTenantMigrationCommand(cmdObj, rst, {
    retryOnRetryableErrors = false,
    shouldStopFunc = () => true,
} = {}) {
    let primary = rst.getPrimary();
    let res;
    assert.soon(() => {
        try {
            // Note: assert.commandWorked() considers command responses with embedded
            // writeErrors and WriteConcernErrors as a failure even if the command returned
            // "ok: 1". And, admin commands(like, donorStartMigration) doesn't generate
            // writeConcernErros or WriteErrors. So, it's safe to wrap the command invocation with
            // assert.commandWorked() here. However, in few scenarios, like
            // Mongo.prototype.recordRerouteDueToTenantMigration(), it's not safe to wrap the
            // command invocation with commandWorked() as retrying on retryable writeConcernErrors
            // can cause the retry attempt to fail with writeErrors.
            res = undefined;
            // In some tests we expects the command to fail due to a network error. We want to
            // catch the error OR the unhandled exception here and return the error to the
            // caller to assert on the result. Otherwise if this is not a network exception
            // it will be caught in the outter catch and either be retried or thrown.
            res = executeNoThrowNetworkError(() => primary.adminCommand(cmdObj));
            assert.commandWorked(res);
            return shouldStopFunc(res);
        } catch (e) {
            if (retryOnRetryableErrors && isRetryableError(e)) {
                jsTestLog(`Retryable error running runTenantMigrationCommand. Command: ${
                    tojson(cmdObj)}, Error: ${tojson(e)}`);

                primary = rst.getPrimary();
                return false;
            }
            jsTestLog(`Error running runTenantMigrationCommand. Command: ${
                tojson(cmdObj)}, Error: ${tojson(e)}`);

            // If res is defined, return true to exit assert.soon and return res to the caller.
            // Otherwise rethrow e to propagate it to the caller.
            if (res)
                return true;

            throw e;
        }
    });
    return res;
}

export const ServerlessLockType = {
    None: 0,
    ShardSplitDonor: 1,
    TenantMigrationDonor: 2,
    TenantMigrationRecipient: 3
};

/**
 * Return the active serverless operation lock, if one is acquired.
 */
export function getServerlessOperationLock(node) {
    return assert.commandWorked(node.adminCommand({serverStatus: 1, serverless: 1}))
        .serverless.operationLock;
}

/**
 * Returns the TenantMigrationAccessBlocker serverStatus output for the multi-tenant migration
 * or shard merge for the given node.
 */
export function getTenantMigrationAccessBlocker({donorNode, recipientNode, tenantId}) {
    assert(donorNode || recipientNode, "missing required parameter donorNode or recipientNode");
    if (donorNode && recipientNode) {
        throw new Error("please specify either 'donorNode' or 'recipientNode' but not both");
    }

    const node = donorNode || recipientNode;
    const {tenantMigrationAccessBlocker} =
        assert.commandWorked(node.adminCommand({serverStatus: 1}));

    if (tenantMigrationAccessBlocker && tenantId &&
        tenantMigrationAccessBlocker.hasOwnProperty(tenantId)) {
        return tenantMigrationAccessBlocker[tenantId];
    }

    return tenantMigrationAccessBlocker;
}

/**
 * Returns all TenantMigrationAccessBlocker serverStatus output for the multi-tenant migration
 * or shard merge associated with the provided tenantId for the given nodes, filtering out any
 * empty entries.
 */
export function getTenantMigrationAccessBlockers({donorNodes = [], recipientNodes = [], tenantId}) {
    const recipientAccessBlockers = recipientNodes.reduce((acc, node) => {
        const accessBlocker = getTenantMigrationAccessBlocker({recipientNode: node, tenantId});
        return accessBlocker && accessBlocker.recipient ? acc.concat(accessBlocker.recipient) : acc;
    }, []);

    const donorAccessBlockers = donorNodes.reduce((acc, node) => {
        const accessBlocker = getTenantMigrationAccessBlocker({donorNode: node, tenantId});
        return accessBlocker && accessBlocker.donor ? acc.concat(accessBlocker.donor) : acc;
    }, []);

    return {recipientAccessBlockers, donorAccessBlockers};
}

/**
 * Returns the number of reads on the given donor node that were blocked due to tenant migration
 * for the given tenant.
 */
export function getNumBlockedReads(donorNode, tenantId) {
    const mtab = getTenantMigrationAccessBlocker({donorNode, tenantId});
    if (!mtab) {
        return 0;
    }
    return mtab.donor.numBlockedReads;
}

/**
 * Returns the number of writes on the given donor node that were blocked due to tenant
 * migration for the given tenant.
 */
export function getNumBlockedWrites(donorNode, tenantId) {
    const mtab = getTenantMigrationAccessBlocker({donorNode, tenantId});
    if (!mtab) {
        return 0;
    }
    return mtab.donor.numBlockedWrites;
}

/**
 * Determines if a database name belongs to the given tenant.
 */
export function isNamespaceForTenant(tenantId, dbName) {
    return dbName.startsWith(`${tenantId}_`);
}

/**
 * Compares the hashes for DBs that belong to the specified tenant between the donor and
 * recipient primaries.
 */
export function checkTenantDBHashes({
    donorRst,
    recipientRst,
    tenantId,
    excludedDBs = [],
    msgPrefix = 'checkTenantDBHashes',
    ignoreUUIDs = false,
    skipTempCollections = false
}) {
    // Always skip db hash checks for the config, admin, and local database.
    excludedDBs = [...excludedDBs, "config", "admin", "local"];

    while (true) {
        try {
            const donorPrimaryConn = donorRst.getPrimary();
            const recipientPrimaryConn = recipientRst.getPrimary();

            // Allows listCollections and listIndexes on donor after migration for consistency
            // checks.
            const donorAllowsReadsAfterMigration =
                assert
                    .commandWorked(donorPrimaryConn.adminCommand({
                        getParameter: 1,
                        "failpoint.tenantMigrationDonorAllowsNonTimestampedReads": 1
                    }))["failpoint.tenantMigrationDonorAllowsNonTimestampedReads"]
                    .mode;
            // Only turn on the failpoint if it is not already.
            if (!donorAllowsReadsAfterMigration) {
                assert.commandWorked(donorPrimaryConn.adminCommand({
                    configureFailPoint: "tenantMigrationDonorAllowsNonTimestampedReads",
                    mode: "alwaysOn"
                }));
            }

            // Filter out all dbs that don't belong to the tenant.
            let combinedDBNames =
                [...donorPrimaryConn.getDBNames(), ...recipientPrimaryConn.getDBNames()];
            combinedDBNames =
                combinedDBNames.filter(dbName => (isNamespaceForTenant(tenantId, dbName) &&
                                                  !excludedDBs.includes(dbName)));
            combinedDBNames = new Set(combinedDBNames);

            print(`checking db hash for tenant '${tenantId}' between donor: ${
                donorPrimaryConn.host}, and recipient: ${recipientPrimaryConn.host}`);

            for (const dbName of combinedDBNames) {
                print(`checking if tenant ${tenantId} owns db ${dbName}`);

                // Pass in an empty array for the secondaries, since we only wish to compare
                // the DB hashes between the donor and recipient primary in this test.
                const donorDBHash = assert.commandWorked(
                    donorRst.getHashes(dbName, [], skipTempCollections).primary);
                const recipientDBHash = assert.commandWorked(
                    recipientRst.getHashes(dbName, [], skipTempCollections).primary);

                const donorCollections = Object.keys(donorDBHash.collections);
                const donorCollInfos = new CollInfos(donorPrimaryConn, 'donorPrimary', dbName);
                donorCollInfos.filter(donorCollections);

                const recipientCollections = Object.keys(recipientDBHash.collections);
                const recipientCollInfos =
                    new CollInfos(recipientPrimaryConn, 'recipientPrimary', dbName);
                recipientCollInfos.filter(recipientCollections);

                const collectionPrinted = new Set();
                const success = DataConsistencyChecker.checkDBHash(donorDBHash,
                                                                   donorCollInfos,
                                                                   recipientDBHash,
                                                                   recipientCollInfos,
                                                                   msgPrefix,
                                                                   ignoreUUIDs,
                                                                   true, /* syncingHasIndexes */
                                                                   collectionPrinted);
                if (!success) {
                    print(`checkTenantDBHashes dumping donor and recipient primary oplogs`);
                    donorRst.dumpOplog(donorPrimaryConn, {}, 100);
                    recipientRst.dumpOplog(recipientPrimaryConn, {}, 100);
                }
                assert(success, 'dbhash mismatch between donor and recipient primaries');
            }

            // Reset failpoint on the donor after consistency checks if it wasn't enabled
            // before.
            if (!donorAllowsReadsAfterMigration) {
                // We unset the failpoint for every node in case there was a failover at some
                // point before this.
                donorRst.nodes.forEach(node => {
                    assert.commandWorked(node.adminCommand({
                        configureFailPoint: "tenantMigrationDonorAllowsNonTimestampedReads",
                        mode: "off"
                    }));
                });
            }

            break;
        } catch (e) {
            if (!checkIfRetryableErrorForTenantDbHashCheck(e)) {
                throw e;
            } else {
                print(`Got error: ${tojson(e)}. Failover occurred during tenant dbhash check,` +
                      ` retrying tenant dbhash check.`);
            }
        }
    }
}

/**
 * Creates a role for tenant migration donor if it doesn't exist.
 */
export function createTenantMigrationDonorRoleIfNotExist(rst) {
    const adminDB = rst.getPrimary().getDB("admin");

    if (roleExists(adminDB, "tenantMigrationDonorRole")) {
        return;
    }

    assert.commandWorked(adminDB.runCommand({
        createRole: "tenantMigrationDonorRole",
        privileges: [
            {resource: {cluster: true}, actions: ["runTenantMigration"]},
            {resource: {db: "admin", collection: "system.keys"}, actions: ["find"]}
        ],
        roles: []
    }));
}

/**
 * Checks if an error gotten while doing a tenant dbhash check is retryable.
 */
export function checkIfRetryableErrorForTenantDbHashCheck(error) {
    // Due to the shell not propagating error codes correctly, if we get any of the following
    // error messages, we can retry the operation.
    const retryableErrorMessages = [
        "The server is in quiesce mode and will shut down",
        "can't connect to new replica set primary"
    ];

    // The following shell helper methods check if the error message contains some
    // notion of retryability. This is in case the error does not contain an error code.
    return isRetryableError(error) || isNetworkError(error) ||
        // If there's a failover while we're running a dbhash check, the elected secondary might
        // not have set the tenantMigrationDonorAllowsNonTimestampedReads failpoint, which means
        // that the listCollections command run when we call CollInfos would throw a
        // TenantMigrationCommitted error.
        ErrorCodes.isTenantMigrationError(error.code) ||
        // If there's a failover as we're creating a ReplSetTest from either the donor or
        // recipient URLs, it's possible to get back a NotYetInitialized error, so we want to
        // retry creating the ReplSetTest.
        error.code == ErrorCodes.NotYetInitialized ||
        // TODO (SERVER-54026): Remove check for error message once the shell correctly
        // propagates the error code.
        retryableErrorMessages.some(msg => error.message.includes(msg));
}

/**
 * Creates a role for tenant migration recipient if it doesn't exist.
 */
export function createTenantMigrationRecipientRoleIfNotExist(rst) {
    const adminDB = rst.getPrimary().getDB("admin");

    if (roleExists(adminDB, "tenantMigrationRecipientRole")) {
        return;
    }

    assert.commandWorked(adminDB.runCommand({
        createRole: "tenantMigrationRecipientRole",
        privileges: [
            {
                resource: {cluster: true},
                actions: ["listDatabases", "useUUID", "advanceClusterTime"]
            },
            {resource: {db: "", collection: ""}, actions: ["listCollections"]},
            {
                resource: {anyResource: true},
                actions: ["dbStats", "collStats", "find", "listIndexes"]
            }
        ],
        roles: []
    }));
}

/**
 * Returns true if the given database role already exists.
 */
export function roleExists(db, roleName) {
    const roles = db.getRoles({rolesInfo: 1, showPrivileges: false, showBuiltinRoles: false});
    const fullRoleName = `${db.getName()}.${roleName}`;
    for (let role of roles) {
        if (role._id == fullRoleName) {
            return true;
        }
    }
    return false;
}
