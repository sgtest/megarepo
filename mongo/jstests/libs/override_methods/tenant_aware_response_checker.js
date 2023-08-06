
export function getDbName(nss) {
    if (nss.length === 0 || !nss.includes(".")) {
        return nss;
    }
    return nss.split(".")[0];
}

export function wordInString(str, word) {
    let escapeRegExp = word.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    let regexp = new RegExp('\\b' + escapeRegExp + '\\b', 'i');
    return regexp.test(str);
}

export function checkExpectedDbNameInString(str, dbName, prefixedDbName, originalRes) {
    // System db names (admin, local and config) should never be tenant prefixed.
    if (dbName == "admin" || dbName == "local" || dbName == "config") {
        assert.eq(false,
                  wordInString(str, prefixedDbName),
                  `Response db name "${str}" does not match sent db name "${
                      dbName}". The response is "${originalRes}"`);
        return;
    }
    // Currently, we do not expect prefixed db name in db name field as we only test with
    // "featureFlagRequireTenantID: true".
    // TODO SERVER-78300: expect prefixed db name if "expectPrefix" option in request is true.
    assert.eq(false,
              wordInString(str, prefixedDbName),
              `Response db name "${str}" does not match sent db name "${
                  dbName}". The response is "${originalRes}"`);
}

export function checkExpectedDbInErrorMsg(errMsg, dbName, prefixedDbName, originalRes) {
    // The db name in error message should always include tenant prefixed db name regardless how the
    // tenantId was received in the request.

    // If the dbName doesn't exist in the error message at all, there is no need to check that it's
    // prefixed.
    if (!wordInString(errMsg, dbName)) {
        return;
    }

    // Skip check system db names (admin, local and config) which could be tenant prefixed or not.
    if (dbName == "admin" || dbName == "local" || dbName == "config") {
        return;
    }

    // Do not check change stream NoMatchingDocument error which does not contain prefixed db name.
    if (errMsg.includes("Change stream was configured to require a post-image") ||
        errMsg.includes("Change stream was configured to require a pre-image")) {
        return;
    }

    assert.eq(true,
              errMsg.includes(prefixedDbName),
              `The db name in the errmsg does not contain expected tenant prefixed db name "${
                  prefixedDbName}". The response is "${originalRes}"`);
}

/**
 * Handle raw command responses or cases like CollectionUUIDMismatch which extend command response
 * and include tenant prefixed db name in "db" instead of "errmsg".
 * There are three different locations for error "code" and "db":
 * - they are children of root response object.
 * - they are children of "writeErrors" object.
 * - they are children of "writeConcernError" object.
 * @param {*} res raw command reponse object.
 * @param {*} tenantPrefix the expected tenant prefix in "db".
 * @returns return false if no such an error exists, otherwise, return true if the db name includes
 *     expected tenant prefix,
 */
function assertErrorExtraInfoIfExists(res, tenantPrefix) {
    if (!res.hasOwnProperty("ok")) {
        return false;
    }

    let errorCode = ErrorCodes.CollectionUUIDMismatch;
    let foundCode = false;
    let dbName = "";
    if (res.hasOwnProperty("code") && res.code == errorCode) {
        foundCode = true;
        dbName = res.db;
    } else if (res.hasOwnProperty("writeErrors")) {
        foundCode = res.writeErrors.some((err) => {
            if (err.code == errorCode) {
                dbName = err.db;
                return true;
            }
        });
    } else if (res.hasOwnProperty("writeConcernError")) {
        foundCode = (res.writeConcernError.code == errorCode);
        dbName = res.writeConcernError.db;
    }
    if (foundCode) {
        assert(dbName.startsWith(tenantPrefix),
               `The db name in the errmsg does not contain matched tenant prefix
            "${tenantPrefix}". The response is "${res}"`);
    }
    return foundCode;
}

/**
 * Check all the db names in the response are expected.
 * @param {*} res response object.
 * @param {*} requestDbName the original db name requested by jstest.
 * @param {*} prefixedDbName the tenant prefixed db name expected by inject_dollar_tenant.js and
 *     inject_security_toiken.js.
 * @param {*} originalResForLogging the original response for logging.
 */
export function assertExpectedDbNameInResponse(
    res, requestDbName, prefixedDbName, originalResForLogging) {
    if (requestDbName.length === 0) {
        return;
    }

    let tenantPrefix = prefixedDbName.substring(0, prefixedDbName.indexOf("_") + 1);
    if (assertErrorExtraInfoIfExists(res, tenantPrefix)) {
        return;
    }

    for (let k of Object.keys(res)) {
        let v = res[k];
        if (typeof v === "string") {
            if (k === "dbName" || k == "db" || k == "dropped") {
                checkExpectedDbNameInString(
                    v, requestDbName, prefixedDbName, originalResForLogging);
            } else if (k === "namespace" || k === "ns") {
                checkExpectedDbNameInString(
                    getDbName(v), requestDbName, prefixedDbName, originalResForLogging);
            } else if (k == "name") {
                checkExpectedDbNameInString(
                    v, requestDbName, prefixedDbName, originalResForLogging);
            } else if (k === "errmsg") {
                checkExpectedDbInErrorMsg(v, requestDbName, prefixedDbName, originalResForLogging);
            }
        } else if (Array.isArray(v)) {
            v.forEach((item) => {
                if (typeof item === "object" && item !== null)
                    assertExpectedDbNameInResponse(
                        item, requestDbName, prefixedDbName, originalResForLogging);
            });
        } else if (typeof v === "object" && v !== null && Object.keys(v).length > 0) {
            assertExpectedDbNameInResponse(v, requestDbName, prefixedDbName, originalResForLogging);
        }
    }
}

/**
 * Remove tenant prefix from response to avoid leaking the tenant id outside of the overrides.
 * @param {*} res response object
 * @param {*} tenantPrefix the tenant prefix which should be removed from the response object. It's
 *     a string includes tenant id and "_", for example "636d957b2646ddfaf9b5e13f_".
 */
export function removeTenantPrefixFromResponse(res, tenantPrefix) {
    for (let k of Object.keys(res)) {
        let v = res[k];
        if (typeof v === "string") {
            // Replace prefixed db name with db name.
            if (v.includes(tenantPrefix)) {
                res[k] = v.replace(tenantPrefix, "");
            }
        } else if (Array.isArray(v)) {
            v.forEach((item) => {
                if (typeof item === "object" && item !== null)
                    removeTenantPrefixFromResponse(item, tenantPrefix);
            });
        } else if (typeof v === "object" && v !== null && Object.keys(v).length > 0) {
            removeTenantPrefixFromResponse(v, tenantPrefix);
        }
    }
}
