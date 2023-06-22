/**
 *    Copyright (C) 2018-present MongoDB, Inc.
 *
 *    This program is free software: you can redistribute it and/or modify
 *    it under the terms of the Server Side Public License, version 1,
 *    as published by MongoDB, Inc.
 *
 *    This program is distributed in the hope that it will be useful,
 *    but WITHOUT ANY WARRANTY; without even the implied warranty of
 *    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *    Server Side Public License for more details.
 *
 *    You should have received a copy of the Server Side Public License
 *    along with this program. If not, see
 *    <http://www.mongodb.com/licensing/server-side-public-license>.
 *
 *    As a special exception, the copyright holders give permission to link the
 *    code of portions of this program with the OpenSSL library under certain
 *    conditions as described in each individual source file and distribute
 *    linked combinations including the program with the OpenSSL library. You
 *    must comply with the Server Side Public License in all respects for
 *    all of the code used other than as permitted herein. If you modify file(s)
 *    with this exception, you may extend this exception to your version of the
 *    file(s), but you are not obligated to do so. If you do not wish to do so,
 *    delete this exception statement from your version. If you delete this
 *    exception statement from all source files in the program, then also delete
 *    it in the license file.
 */

#pragma once

#include <cstdint>
#include <functional>
#include <limits>
#include <set>

#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/db/index/index_descriptor.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/query/collation/collator_interface.h"
#include "mongo/util/duration.h"

namespace mongo {
class BSONObj;
class CollatorInterface;
class NamespaceString;
class Status;
template <typename T>
class StatusWith;

namespace index_key_validate {

// TTL indexes with 'expireAfterSeconds' are repaired with this duration, which is chosen to be
// the largest possible value for the 'safeInt' type that can be returned in the listIndexes
// response.
constexpr auto kExpireAfterSecondsForInactiveTTLIndex =
    Seconds(std::numeric_limits<int32_t>::max());

static std::set<StringData> allowedFieldNames = {
    IndexDescriptor::k2dIndexBitsFieldName,
    IndexDescriptor::k2dIndexMaxFieldName,
    IndexDescriptor::k2dIndexMinFieldName,
    IndexDescriptor::k2dsphereCoarsestIndexedLevel,
    IndexDescriptor::k2dsphereFinestIndexedLevel,
    IndexDescriptor::k2dsphereVersionFieldName,
    IndexDescriptor::kBackgroundFieldName,
    IndexDescriptor::kCollationFieldName,
    IndexDescriptor::kDefaultLanguageFieldName,
    IndexDescriptor::kDropDuplicatesFieldName,
    IndexDescriptor::kExpireAfterSecondsFieldName,
    IndexDescriptor::kHiddenFieldName,
    IndexDescriptor::kIndexNameFieldName,
    IndexDescriptor::kIndexVersionFieldName,
    IndexDescriptor::kKeyPatternFieldName,
    IndexDescriptor::kLanguageOverrideFieldName,
    IndexDescriptor::kNamespaceFieldName,
    IndexDescriptor::kPartialFilterExprFieldName,
    IndexDescriptor::kWildcardProjectionFieldName,
    IndexDescriptor::kColumnStoreProjectionFieldName,
    IndexDescriptor::kSparseFieldName,
    IndexDescriptor::kStorageEngineFieldName,
    IndexDescriptor::kTextVersionFieldName,
    IndexDescriptor::kUniqueFieldName,
    IndexDescriptor::kWeightsFieldName,
    IndexDescriptor::kOriginalSpecFieldName,
    IndexDescriptor::kPrepareUniqueFieldName,
    IndexDescriptor::kColumnStoreCompressorFieldName,
    // Index creation under legacy writeMode can result in an index spec with an _id field.
    "_id",
    // TODO SERVER-76108: Field names are not validated to match index type. This was used for the
    // removed 'geoHaystack' index type, but users could have set it for other index types as well.
    // We need to keep allowing it until FCV upgrade is implemented to clean this up.
    "bucketSize"_sd,
};

/**
 * Checks if the key is valid for building an index according to the validation rules for the given
 * index version. If 'checkFCV' is true we will check FCV for compound wildcard indexes validation.
 *
 * TODO SERVER-68303: Consider removing 'checkFCV' flag when 'CompoundWildcardIndexes'
 * feature flag is removed.
 */
Status validateKeyPattern(const BSONObj& key,
                          IndexDescriptor::IndexVersion indexVersion,
                          bool checkFCV = false);

/**
 * Validates the index specification 'indexSpec' and returns an equivalent index specification that
 * has any missing attributes filled in. If the index specification is malformed, then an error
 * status is returned. If 'checkFCV' is true we will check FCV for compound wildcard indexes
 * validation.
 *
 * TODO SERVER-68303: Consider removing 'checkFCV' flag when 'CompoundWildcardIndexes'
 * feature flag is removed.
 */
StatusWith<BSONObj> validateIndexSpec(OperationContext* opCtx,
                                      const BSONObj& indexSpec,
                                      bool checkFCV = false);

/**
 * Returns a new index spec with any unknown field names removed from 'indexSpec'.
 */
BSONObj removeUnknownFields(const NamespaceString& ns, const BSONObj& indexSpec);

/**
 * Returns a new index spec with boolean values in correct types and unkown field names removed.
 */
BSONObj repairIndexSpec(
    const NamespaceString& ns,
    const BSONObj& indexSpec,
    const std::set<StringData>& allowedFieldNames = index_key_validate::allowedFieldNames);

/**
 * Performs additional validation for _id index specifications. This should be called after
 * validateIndexSpec().
 */
Status validateIdIndexSpec(const BSONObj& indexSpec);

/**
 * Confirms that 'indexSpec' contains only valid field names. Returns an error if an unexpected
 * field name is found.
 */
Status validateIndexSpecFieldNames(const BSONObj& indexSpec);

/**
 * Validates the 'collation' field in the index specification 'indexSpec' and fills in the full
 * collation spec. If 'collation' is missing, fills it in with the spec for 'defaultCollator'.
 * Returns the index specification with 'collation' filled in.
 */
StatusWith<BSONObj> validateIndexSpecCollation(OperationContext* opCtx,
                                               const BSONObj& indexSpec,
                                               const CollatorInterface* defaultCollator);

/**
 * Validates the the 'expireAfterSeconds' value for a TTL index or clustered collection.
 */
enum class ValidateExpireAfterSecondsMode {
    kSecondaryTTLIndex,
    kClusteredTTLIndex,
};
Status validateExpireAfterSeconds(std::int64_t expireAfterSeconds,
                                  ValidateExpireAfterSecondsMode mode);

Status validateExpireAfterSeconds(BSONElement expireAfterSeconds,
                                  ValidateExpireAfterSecondsMode mode);
/**
 * Returns true if 'indexSpec' refers to a TTL index.
 */
bool isIndexTTL(const BSONObj& indexSpec);

/**
 * Validates the key pattern and the 'expireAfterSeconds' duration in the index specification
 * 'indexSpec' for a TTL index. Returns success if 'indexSpec' does not refer to a TTL index.
 */
Status validateIndexSpecTTL(const BSONObj& indexSpec);

/**
 * Returns whether an index is allowed in API version 1.
 */
bool isIndexAllowedInAPIVersion1(const IndexDescriptor& indexDesc);

/**
 * Parses the index specifications from 'indexSpecObj', validates them, and returns equivalent index
 * specifications that have any missing attributes filled in. If any index specification is
 * malformed, then an error status is returned. If 'checkFCV' is true we should validate the index
 * spec taking into account the FCV value. Some certain type of index cannot be created with
 * downgraded FCV but can be continuously used if it's already created before FCV downgrade.
 */
BSONObj parseAndValidateIndexSpecs(OperationContext* opCtx,
                                   const BSONObj& indexSpecObj,
                                   bool checkFCV);

/**
 * Optional filtering function to adjust allowed index field names at startup.
 * Set it in a MONGO_INITIALIZER with 'FilterAllowedIndexFieldNames' as a dependant.
 */
extern std::function<void(std::set<StringData>& allowedIndexFieldNames)>
    filterAllowedIndexFieldNames;

}  // namespace index_key_validate
}  // namespace mongo
