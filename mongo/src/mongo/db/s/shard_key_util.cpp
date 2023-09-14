/**
 *    Copyright (C) 2019-present MongoDB, Inc.
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

#include <absl/container/node_hash_map.h>
#include <boost/container/small_vector.hpp>
// IWYU pragma: no_include "boost/intrusive/detail/iterator.hpp"
#include <algorithm>
#include <boost/preprocessor/control/iif.hpp>
#include <iterator>
#include <list>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/simple_bsonelement_comparator.h"
#include "mongo/bson/simple_bsonobj_comparator.h"
#include "mongo/bson/util/builder.h"
#include "mongo/bson/util/builder_fwd.h"
#include "mongo/client/read_preference.h"
#include "mongo/crypto/encryption_fields_gen.h"
#include "mongo/crypto/encryption_fields_util.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/collection_options.h"
#include "mongo/db/catalog/index_catalog.h"
#include "mongo/db/catalog/index_catalog_entry.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/database_name.h"
#include "mongo/db/field_ref.h"
#include "mongo/db/hasher.h"
#include "mongo/db/index/index_descriptor.h"
#include "mongo/db/query/collation/collation_spec.h"
#include "mongo/db/s/migration_destination_manager.h"
#include "mongo/db/s/shard_key_index_util.h"
#include "mongo/db/s/shard_key_util.h"
#include "mongo/db/write_concern_options.h"
#include "mongo/rpc/get_status_from_command_result.h"
#include "mongo/s/chunk_manager.h"
#include "mongo/s/client/shard_registry.h"
#include "mongo/s/cluster_commands_helpers.h"
#include "mongo/s/grid.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/duration.h"
#include "mongo/util/namespace_string_util.h"
#include "mongo/util/str.h"
#include "mongo/util/uuid.h"

namespace mongo {
namespace shardkeyutil {
namespace {

constexpr StringData kCheckShardingIndexCmdName = "checkShardingIndex"_sd;
constexpr StringData kKeyPatternField = "keyPattern"_sd;

/**
 * Create an index specification used for create index command.
 */
BSONObj makeIndexSpec(const NamespaceString& nss,
                      const BSONObj& keys,
                      const BSONObj& collation,
                      bool unique) {
    BSONObjBuilder index;

    // Required fields for an index.
    index.append("key", keys);

    StringBuilder indexName;
    bool isFirstKey = true;
    for (BSONObjIterator keyIter(keys); keyIter.more();) {
        BSONElement currentKey = keyIter.next();

        if (isFirstKey) {
            isFirstKey = false;
        } else {
            indexName << "_";
        }

        indexName << currentKey.fieldName() << "_";
        if (currentKey.isNumber()) {
            indexName << currentKey.numberInt();
        } else {
            indexName << currentKey.str();  // This should match up with shell command.
        }
    }
    index.append("name", indexName.str());

    // Index options.
    if (!collation.isEmpty()) {
        // Creating an index with the "collation" option requires a v=2 index.
        index.append("v", static_cast<int>(IndexDescriptor::IndexVersion::kV2));
        index.append("collation", collation);
    }

    if (unique && !IndexDescriptor::isIdIndexPattern(keys)) {
        index.appendBool("unique", unique);
    }

    return index.obj();
}

/**
 * Constructs the BSON specification document for the create indexes command using the given
 * namespace, index key and options.
 */
BSONObj makeCreateIndexesCmd(const NamespaceString& nss,
                             const BSONObj& keys,
                             const BSONObj& collation,
                             bool unique) {
    auto indexSpec = makeIndexSpec(nss, keys, collation, unique);
    // The outer createIndexes command.
    BSONObjBuilder createIndexes;
    createIndexes.append("createIndexes", nss.coll());
    createIndexes.append("indexes", BSON_ARRAY(indexSpec));
    createIndexes.append("writeConcern", WriteConcernOptions::Majority);
    return createIndexes.obj();
}

}  // namespace

bool validShardKeyIndexExists(OperationContext* opCtx,
                              const NamespaceString& nss,
                              const ShardKeyPattern& shardKeyPattern,
                              const boost::optional<BSONObj>& defaultCollation,
                              bool requiresUnique,
                              const ShardKeyValidationBehaviors& behaviors,
                              std::string* errMsg) {
    auto indexes = behaviors.loadIndexes(nss);

    // 1.  Verify consistency with existing unique indexes
    for (const auto& idx : indexes) {
        BSONObj currentKey = idx["key"].embeddedObject();
        bool isUnique = idx["unique"].trueValue();
        bool isPrepareUnique = idx["prepareUnique"].trueValue();
        uassert(ErrorCodes::InvalidOptions,
                str::stream() << "can't shard collection '" << nss.toStringForErrorMsg()
                              << "' with unique index on " << currentKey
                              << " and proposed shard key " << shardKeyPattern.toBSON()
                              << ". Uniqueness can't be maintained unless shard key is a prefix",
                (!isUnique && !isPrepareUnique) ||
                    shardKeyPattern.isIndexUniquenessCompatible(currentKey));
    }

    // 2. Check for a useful index
    bool hasUsefulIndexForKey = false;
    std::string allReasons;
    for (const auto& idx : indexes) {
        std::string reasons;
        BSONObj currentKey = idx["key"].embeddedObject();
        // Check 2.i. and 2.ii.
        if (!idx["sparse"].trueValue() && idx["filter"].eoo() && idx["collation"].eoo() &&
            shardKeyPattern.toBSON().isPrefixOf(currentKey,
                                                SimpleBSONElementComparator::kInstance)) {
            // We can't currently use hashed indexes with a non-default hash seed
            // Check iv.
            // Note that this means that, for sharding, we only support one hashed index
            // per field per collection.
            uassert(ErrorCodes::InvalidOptions,
                    str::stream() << "can't shard collection " << nss.toStringForErrorMsg()
                                  << " with hashed shard key " << shardKeyPattern.toBSON()
                                  << " because the hashed index uses a non-default seed of "
                                  << idx["seed"].numberInt(),
                    !shardKeyPattern.isHashedPattern() || idx["seed"].eoo() ||
                        idx["seed"].numberInt() == BSONElementHasher::DEFAULT_HASH_SEED);
            hasUsefulIndexForKey = true;
        }
        if (idx["sparse"].trueValue()) {
            reasons += " Index key is sparse.";
        }
        if (idx["filter"].ok()) {
            reasons += " Index key is partial.";
        }
        if (idx["collation"].ok()) {
            reasons += " Index has a non-simple collation.";
        }
        if (!reasons.empty()) {
            allReasons =
                " Index " + idx["name"] + " cannot be used for sharding because [" + reasons + " ]";
        }
    }

    // 3. If proposed key is required to be unique, additionally check for exact match.
    if (hasUsefulIndexForKey && requiresUnique) {
        BSONObj eqQuery =
            BSON("ns" << NamespaceStringUtil::serialize(nss) << "key" << shardKeyPattern.toBSON());
        BSONObj eqQueryResult;

        for (const auto& idx : indexes) {
            if (SimpleBSONObjComparator::kInstance.evaluate(idx["key"].embeddedObject() ==
                                                            shardKeyPattern.toBSON())) {
                eqQueryResult = idx;
                break;
            }
        }

        if (eqQueryResult.isEmpty()) {
            // If no exact match, index not useful, but still possible to create one later
            hasUsefulIndexForKey = false;
        } else {
            bool isExplicitlyUnique = eqQueryResult["unique"].trueValue();
            BSONObj currKey = eqQueryResult["key"].embeddedObject();
            bool isCurrentID = (currKey.firstElementFieldNameStringData() == "_id");
            uassert(ErrorCodes::InvalidOptions,
                    str::stream() << "can't shard collection " << nss.toStringForErrorMsg() << ", "
                                  << shardKeyPattern.toBSON()
                                  << " index not unique, and unique index explicitly specified",
                    isExplicitlyUnique || isCurrentID);
        }
    }

    if (errMsg && !allReasons.empty()) {
        *errMsg += allReasons;
    }

    if (hasUsefulIndexForKey) {
        // Check 2.iii Make sure that there is a useful, non-multikey index available.
        behaviors.verifyUsefulNonMultiKeyIndex(nss, shardKeyPattern.toBSON());
    }

    return hasUsefulIndexForKey;
}

bool validateShardKeyIndexExistsOrCreateIfPossible(OperationContext* opCtx,
                                                   const NamespaceString& nss,
                                                   const ShardKeyPattern& shardKeyPattern,
                                                   const boost::optional<BSONObj>& defaultCollation,
                                                   bool unique,
                                                   bool enforceUniquenessCheck,
                                                   const ShardKeyValidationBehaviors& behaviors) {
    std::string errMsg;
    if (validShardKeyIndexExists(opCtx,
                                 nss,
                                 shardKeyPattern,
                                 defaultCollation,
                                 unique && enforceUniquenessCheck,
                                 behaviors,
                                 &errMsg)) {
        return false;
    }

    // 4. If no useful index, verify we can create one.
    behaviors.verifyCanCreateShardKeyIndex(nss, &errMsg);

    // 5. If no useful index exists and we can create one, create one on proposedKey. Only need
    //    to call ensureIndex on primary shard, since indexes get copied to receiving shard
    //    whenever a migrate occurs. If the collection has a default collation, explicitly send
    //    the simple collation as part of the createIndex request.
    behaviors.createShardKeyIndex(nss, shardKeyPattern.toBSON(), defaultCollation, unique);
    return true;
}

// TODO: SERVER-64187 move calls to validateShardKeyIsNotEncrypted into
// validateShardKeyIndexExistsOrCreateIfPossible
void validateShardKeyIsNotEncrypted(OperationContext* opCtx,
                                    const NamespaceString& nss,
                                    const ShardKeyPattern& shardKeyPattern) {
    AutoGetCollection collection(
        opCtx,
        nss,
        MODE_IS,
        AutoGetCollection::Options{}.viewMode(auto_get_collection::ViewMode::kViewsPermitted));
    if (!collection || collection.getView()) {
        return;
    }

    const auto& encryptConfig = collection->getCollectionOptions().encryptedFieldConfig;
    if (!encryptConfig) {
        // this collection is not encrypted
        return;
    }

    auto& encryptedFields = encryptConfig->getFields();
    std::vector<FieldRef> encryptedFieldRefs;
    std::transform(encryptedFields.begin(),
                   encryptedFields.end(),
                   std::back_inserter(encryptedFieldRefs),
                   [](auto& path) { return FieldRef(path.getPath()); });

    for (const auto& keyFieldRef : shardKeyPattern.getKeyPatternFields()) {
        auto match = findMatchingEncryptedField(*keyFieldRef, encryptedFieldRefs);
        uassert(ErrorCodes::InvalidOptions,
                str::stream() << "Sharding is not allowed on keys that are equal to, or a "
                                 "prefix of, the encrypted field "
                              << match->encryptedField.dottedField(),
                !match || !match->keyIsPrefixOrEqual);
        uassert(
            ErrorCodes::InvalidOptions,
            str::stream() << "Sharding is not allowed on keys whose prefix is the encrypted field "
                          << match->encryptedField.dottedField(),
            !match || match->keyIsPrefixOrEqual);
    }
}

std::vector<BSONObj> ValidationBehaviorsShardCollection::loadIndexes(
    const NamespaceString& nss) const {
    const bool includeBuildUUIDs = false;
    const int options = 0;
    std::list<BSONObj> indexes = _localClient->getIndexSpecs(nss, includeBuildUUIDs, options);
    // Convert std::list to a std::vector.
    return std::vector<BSONObj>{std::make_move_iterator(std::begin(indexes)),
                                std::make_move_iterator(std::end(indexes))};
}

void ValidationBehaviorsShardCollection::verifyUsefulNonMultiKeyIndex(
    const NamespaceString& nss, const BSONObj& proposedKey) const {
    BSONObj res;
    auto success = _localClient->runCommand(DatabaseName::kAdmin,
                                            BSON(kCheckShardingIndexCmdName
                                                 << NamespaceStringUtil::serialize(nss)
                                                 << kKeyPatternField << proposedKey),
                                            res);
    uassert(ErrorCodes::InvalidOptions, res["errmsg"].str(), success);
}

void ValidationBehaviorsShardCollection::verifyCanCreateShardKeyIndex(const NamespaceString& nss,
                                                                      std::string* errMsg) const {
    uassert(ErrorCodes::InvalidOptions,
            str::stream() << "Please create an index that starts with the proposed shard key before"
                             " sharding the collection. "
                          << *errMsg,
            _localClient->findOne(nss, BSONObj{}).isEmpty());
}

void ValidationBehaviorsShardCollection::createShardKeyIndex(
    const NamespaceString& nss,
    const BSONObj& proposedKey,
    const boost::optional<BSONObj>& defaultCollation,
    bool unique) const {
    BSONObj collation =
        defaultCollation && !defaultCollation->isEmpty() ? CollationSpec::kSimpleSpec : BSONObj();
    auto createIndexesCmd = makeCreateIndexesCmd(nss, proposedKey, collation, unique);
    BSONObj res;
    _localClient->runCommand(nss.dbName(), createIndexesCmd, res);
    uassertStatusOK(getStatusFromCommandResult(res));
}

ValidationBehaviorsRefineShardKey::ValidationBehaviorsRefineShardKey(OperationContext* opCtx,
                                                                     const NamespaceString& nss)
    : _opCtx(opCtx),
      _cri(uassertStatusOK(
          Grid::get(opCtx)->catalogCache()->getShardedCollectionRoutingInfoWithRefresh(opCtx,
                                                                                       nss))),
      _indexShard(uassertStatusOK(Grid::get(opCtx)->shardRegistry()->getShard(
          opCtx, _cri.cm.getMinKeyShardIdWithSimpleCollation()))) {}

std::vector<BSONObj> ValidationBehaviorsRefineShardKey::loadIndexes(
    const NamespaceString& nss) const {
    auto indexesRes = _indexShard->runExhaustiveCursorCommand(
        _opCtx,
        ReadPreferenceSetting(ReadPreference::PrimaryOnly),
        nss.dbName(),
        appendShardVersion(BSON("listIndexes" << nss.coll()),
                           _cri.getShardVersion(_indexShard->getId())),
        Milliseconds(-1));
    if (indexesRes.getStatus().code() != ErrorCodes::NamespaceNotFound) {
        return uassertStatusOK(indexesRes).docs;
    }
    return {};
}

void ValidationBehaviorsRefineShardKey::verifyUsefulNonMultiKeyIndex(
    const NamespaceString& nss, const BSONObj& proposedKey) const {
    auto checkShardingIndexRes = uassertStatusOK(_indexShard->runCommand(
        _opCtx,
        ReadPreferenceSetting(ReadPreference::PrimaryOnly),
        DatabaseName::kAdmin,
        appendShardVersion(BSON(kCheckShardingIndexCmdName << NamespaceStringUtil::serialize(nss)
                                                           << kKeyPatternField << proposedKey),
                           _cri.getShardVersion(_indexShard->getId())),
        Shard::RetryPolicy::kIdempotent));
    if (checkShardingIndexRes.commandStatus == ErrorCodes::UnknownError) {
        // CheckShardingIndex returns UnknownError if a compatible shard key index cannot be found,
        // but we return InvalidOptions to correspond with the shardCollection behavior.
        uasserted(ErrorCodes::InvalidOptions, checkShardingIndexRes.response["errmsg"].str());
    }
    // Rethrow any other error to allow retries on retryable errors.
    uassertStatusOK(checkShardingIndexRes.commandStatus);
}

void ValidationBehaviorsRefineShardKey::verifyCanCreateShardKeyIndex(const NamespaceString& nss,
                                                                     std::string* errMsg) const {
    uasserted(
        ErrorCodes::InvalidOptions,
        str::stream() << "Please create an index that starts with the proposed shard key before"
                         " refining the collection's shard key. "
                      << *errMsg);
}

void ValidationBehaviorsRefineShardKey::createShardKeyIndex(
    const NamespaceString& nss,
    const BSONObj& proposedKey,
    const boost::optional<BSONObj>& defaultCollation,
    bool unique) const {
    MONGO_UNREACHABLE;
}

ValidationBehaviorsLocalRefineShardKey::ValidationBehaviorsLocalRefineShardKey(
    OperationContext* opCtx, const CollectionPtr& coll)
    : _opCtx(opCtx), _coll(coll) {}


std::vector<BSONObj> ValidationBehaviorsLocalRefineShardKey::loadIndexes(
    const NamespaceString& nss) const {
    std::vector<BSONObj> indexes;
    auto it = _coll->getIndexCatalog()->getIndexIterator(
        _opCtx, mongo::IndexCatalog::InclusionPolicy::kReady);
    while (it->more()) {
        auto entry = it->next();
        indexes.push_back(entry->descriptor()->toBSON());
    }
    return indexes;
}

void ValidationBehaviorsLocalRefineShardKey::verifyUsefulNonMultiKeyIndex(
    const NamespaceString& nss, const BSONObj& proposedKey) const {
    std::string tmpErrMsg = "couldn't find valid index for shard key";
    uassert(ErrorCodes::InvalidOptions,
            tmpErrMsg,
            findShardKeyPrefixedIndex(_opCtx,
                                      _coll,
                                      proposedKey,
                                      /*requireSingleKey=*/true,
                                      &tmpErrMsg));
}

void ValidationBehaviorsLocalRefineShardKey::verifyCanCreateShardKeyIndex(
    const NamespaceString& nss, std::string* errMsg) const {
    uasserted(
        ErrorCodes::InvalidOptions,
        str::stream() << "Please create an index that starts with the proposed shard key before"
                         " refining the collection's shard key. "
                      << *errMsg);
}

void ValidationBehaviorsLocalRefineShardKey::createShardKeyIndex(
    const NamespaceString& nss,
    const BSONObj& proposedKey,
    const boost::optional<BSONObj>& defaultCollation,
    bool unique) const {
    MONGO_UNREACHABLE;
}

ValidationBehaviorsReshardingBulkIndex::ValidationBehaviorsReshardingBulkIndex()
    : _opCtx(nullptr), _cloneTimestamp(), _shardKeyIndexSpec() {}

std::vector<BSONObj> ValidationBehaviorsReshardingBulkIndex::loadIndexes(
    const NamespaceString& nss) const {
    invariant(_opCtx);
    auto catalogCache = Grid::get(_opCtx)->catalogCache();
    auto cri = catalogCache->getTrackedCollectionRoutingInfo(_opCtx, nss);
    auto [indexSpecs, _] = MigrationDestinationManager::getCollectionIndexes(
        _opCtx, nss, cri.cm.getMinKeyShardIdWithSimpleCollation(), cri, _cloneTimestamp);
    return indexSpecs;
}

void ValidationBehaviorsReshardingBulkIndex::verifyUsefulNonMultiKeyIndex(
    const NamespaceString& nss, const BSONObj& proposedKey) const {
    invariant(_opCtx);
    auto catalogCache = Grid::get(_opCtx)->catalogCache();
    auto cri = catalogCache->getTrackedCollectionRoutingInfo(_opCtx, nss);
    auto shard = uassertStatusOK(Grid::get(_opCtx)->shardRegistry()->getShard(
        _opCtx, cri.cm.getMinKeyShardIdWithSimpleCollation()));
    auto checkShardingIndexRes = uassertStatusOK(shard->runCommand(
        _opCtx,
        ReadPreferenceSetting(ReadPreference::PrimaryOnly),
        DatabaseName::kAdmin,
        appendShardVersion(BSON(kCheckShardingIndexCmdName << NamespaceStringUtil::serialize(nss)
                                                           << kKeyPatternField << proposedKey),
                           cri.getShardVersion(shard->getId())),
        Shard::RetryPolicy::kIdempotent));
    if (checkShardingIndexRes.commandStatus == ErrorCodes::UnknownError) {
        // CheckShardingIndex returns UnknownError if a compatible shard key index cannot be found,
        // but we return InvalidOptions to correspond with the shardCollection behavior.
        uasserted(ErrorCodes::InvalidOptions, checkShardingIndexRes.response["errmsg"].str());
    }
    // Rethrow any other error to allow retries on retryable errors.
    uassertStatusOK(checkShardingIndexRes.commandStatus);
}

void ValidationBehaviorsReshardingBulkIndex::verifyCanCreateShardKeyIndex(
    const NamespaceString& nss, std::string* errMsg) const {}

void ValidationBehaviorsReshardingBulkIndex::createShardKeyIndex(
    const NamespaceString& nss,
    const BSONObj& proposedKey,
    const boost::optional<BSONObj>& defaultCollation,
    bool unique) const {
    BSONObj collation =
        defaultCollation && !defaultCollation->isEmpty() ? CollationSpec::kSimpleSpec : BSONObj();
    _shardKeyIndexSpec = makeIndexSpec(nss, proposedKey, collation, unique);
}

void ValidationBehaviorsReshardingBulkIndex::setOpCtxAndCloneTimestamp(OperationContext* opCtx,
                                                                       Timestamp cloneTimestamp) {
    _opCtx = opCtx;
    _cloneTimestamp = cloneTimestamp;
}

boost::optional<BSONObj> ValidationBehaviorsReshardingBulkIndex::getShardKeyIndexSpec() const {
    return _shardKeyIndexSpec;
}

}  // namespace shardkeyutil
}  // namespace mongo
