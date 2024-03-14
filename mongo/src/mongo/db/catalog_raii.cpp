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

#include "mongo/db/catalog_raii.h"

#include <boost/optional.hpp>
#include <fmt/format.h>
#include <functional>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/db/catalog/catalog_helper.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/catalog/collection_uuid_mismatch.h"
#include "mongo/db/catalog/collection_yield_restore.h"
#include "mongo/db/catalog/database_holder.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/repl/collection_utils.h"
#include "mongo/db/s/collection_sharding_state.h"
#include "mongo/db/s/database_sharding_state.h"
#include "mongo/db/s/operation_sharding_state.h"
#include "mongo/db/s/scoped_collection_metadata.h"
#include "mongo/db/shard_role.h"
#include "mongo/db/transaction_resources.h"
#include "mongo/s/shard_version.h"
#include "mongo/s/sharding_state.h"
#include "mongo/s/stale_exception.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/duration.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kStorage

namespace mongo {
namespace {

/**
 * Performs some sanity checks on the collection and database.
 */
void verifyDbAndCollection(OperationContext* opCtx,
                           LockMode modeColl,
                           const NamespaceStringOrUUID& nsOrUUID,
                           const NamespaceString& resolvedNss,
                           const Collection* coll,
                           Database* db,
                           bool verifyWriteEligible) {
    invariant(!nsOrUUID.isUUID() || coll,
              str::stream() << "Collection for " << resolvedNss.toStringForErrorMsg()
                            << " disappeared after successfully resolving "
                            << nsOrUUID.toStringForErrorMsg());

    invariant(!nsOrUUID.isUUID() || db,
              str::stream() << "Database for " << resolvedNss.toStringForErrorMsg()
                            << " disappeared after successfully resolving "
                            << nsOrUUID.toStringForErrorMsg());

    // In most cases we expect modifications for system.views to upgrade MODE_IX to MODE_X before
    // taking the lock. One exception is a query by UUID of system.views in a transaction. Usual
    // queries of system.views (by name, not UUID) within a transaction are rejected. However, if
    // the query is by UUID we can't determine whether the namespace is actually system.views until
    // we take the lock here. So we have this one last assertion.
    uassert(51070,
            "Modifications to system.views must take an exclusive lock",
            !resolvedNss.isSystemDotViews() || modeColl != MODE_IX);

    if (!db || !coll) {
        return;
    }

    // Verify that we are using the latest instance if we intend to perform writes.
    if (verifyWriteEligible) {
        auto latest = CollectionCatalog::latest(opCtx);
        if (!latest->isLatestCollection(opCtx, coll)) {
            throwWriteConflictException(str::stream() << "Unable to write to collection '"
                                                      << coll->ns().toStringForErrorMsg()
                                                      << "' due to catalog changes; please "
                                                         "retry the operation");
        }
        if (shard_role_details::getRecoveryUnit(opCtx)->isActive()) {
            const auto mySnapshot =
                shard_role_details::getRecoveryUnit(opCtx)->getPointInTimeReadTimestamp(opCtx);
            if (mySnapshot && *mySnapshot < coll->getMinimumValidSnapshot()) {
                throwWriteConflictException(str::stream()
                                            << "Unable to write to collection '"
                                            << coll->ns().toStringForErrorMsg()
                                            << "' due to snapshot timestamp " << *mySnapshot
                                            << " being older than collection minimum "
                                            << *coll->getMinimumValidSnapshot()
                                            << "; please retry the operation");
            }
        }
    }
}

}  // namespace

AutoGetDb::AutoGetDb(OperationContext* opCtx,
                     const DatabaseName& dbName,
                     LockMode mode,
                     boost::optional<LockMode> tenantLockMode,
                     Date_t deadline)
    : AutoGetDb(opCtx, dbName, mode, tenantLockMode, deadline, [] {
          Lock::GlobalLockSkipOptions options;
          return options;
      }()) {}

AutoGetDb::AutoGetDb(OperationContext* opCtx,
                     const DatabaseName& dbName,
                     LockMode mode,
                     boost::optional<LockMode> tenantLockMode,
                     Date_t deadline,
                     Lock::DBLockSkipOptions options)
    : _dbName(dbName),
      _dbLock(opCtx, dbName, mode, deadline, std::move(options), tenantLockMode),
      _db([&] {
          auto databaseHolder = DatabaseHolder::get(opCtx);
          return databaseHolder->getDb(opCtx, dbName);
      }()) {
    // The 'primary' database must be version checked for sharding.
    DatabaseShardingState::assertMatchingDbVersion(opCtx, _dbName);
}

bool AutoGetDb::canSkipRSTLLock(const NamespaceStringOrUUID& nsOrUUID) {
    if (nsOrUUID.isNamespaceString()) {
        return repl::canCollectionSkipRSTLLockAcquisition(nsOrUUID.nss());
    }
    return false;
}

bool AutoGetDb::canSkipFlowControlTicket(const NamespaceStringOrUUID& nsOrUUID) {
    if (nsOrUUID.isNamespaceString()) {
        const auto& nss = nsOrUUID.nss();
        bool notReplicated = !nss.isReplicated();
        // TODO: Improve comment
        //
        // If the 'opCtx' is in a multi document transaction, pure reads on the
        // transaction session collections would acquire the global lock in the IX
        // mode and acquire a flow control ticket.
        bool isTransactionCollection = nss == NamespaceString::kSessionTransactionsTableNamespace ||
            nss == NamespaceString::kTransactionCoordinatorsNamespace;
        return notReplicated || isTransactionCollection;
    }
    return false;
}

AutoGetDb AutoGetDb::createForAutoGetCollection(
    OperationContext* opCtx,
    const NamespaceStringOrUUID& nsOrUUID,
    LockMode modeColl,
    const auto_get_collection::OptionsWithSecondaryCollections& options) {
    auto& deadline = options._deadline;

    invariant(!opCtx->isLockFreeReadsOp());

    // Acquire the global/RSTL and all the database locks (may or may not be multiple
    // databases).
    Lock::DBLockSkipOptions dbLockOptions;
    if (options._globalLockSkipOptions) {
        dbLockOptions = *options._globalLockSkipOptions;
    } else {
        dbLockOptions.skipRSTLLock = canSkipRSTLLock(nsOrUUID);
        dbLockOptions.skipFlowControlTicket = canSkipFlowControlTicket(nsOrUUID);
    }

    return AutoGetDb(opCtx,
                     nsOrUUID.dbName(),
                     isSharedLockMode(modeColl) ? MODE_IS : MODE_IX,
                     boost::none /* tenantLockMode */,
                     deadline,
                     std::move(dbLockOptions));
}

AutoGetDb::AutoGetDb(OperationContext* opCtx,
                     const DatabaseName& dbName,
                     LockMode mode,
                     Date_t deadline)
    : AutoGetDb(opCtx, dbName, mode, boost::none, deadline) {}

Database* AutoGetDb::ensureDbExists(OperationContext* opCtx) {
    if (_db) {
        return _db;
    }

    auto databaseHolder = DatabaseHolder::get(opCtx);
    _db = databaseHolder->openDb(opCtx, _dbName, nullptr);
    DatabaseShardingState::assertMatchingDbVersion(opCtx, _dbName);

    return _db;
}

Database* AutoGetDb::refreshDbReferenceIfNull(OperationContext* opCtx) {
    if (!_db) {
        auto databaseHolder = DatabaseHolder::get(opCtx);
        _db = databaseHolder->getDb(opCtx, _dbName);
        DatabaseShardingState::assertMatchingDbVersion(opCtx, _dbName);
    }
    return _db;
}


CollectionNamespaceOrUUIDLock::CollectionNamespaceOrUUIDLock(OperationContext* opCtx,
                                                             const NamespaceStringOrUUID& nsOrUUID,
                                                             LockMode mode,
                                                             Date_t deadline)
    : _lock([opCtx, &nsOrUUID, mode, deadline] {
          if (nsOrUUID.isNamespaceString()) {
              return Lock::CollectionLock{opCtx, nsOrUUID.nss(), mode, deadline};
          }

          auto resolveNs = [opCtx, &nsOrUUID] {
              return CollectionCatalog::get(opCtx)->resolveNamespaceStringOrUUID(opCtx, nsOrUUID);
          };

          // We cannot be sure that the namespace we lock matches the UUID given because we resolve
          // the namespace from the UUID without the safety of a lock. Therefore, we will continue
          // to re-lock until the namespace we resolve from the UUID before and after taking the
          // lock is the same.
          while (true) {
              auto ns = resolveNs();
              Lock::CollectionLock lock{opCtx, ns, mode, deadline};
              if (ns == resolveNs()) {
                  return lock;
              }
          }
      }()) {}

AutoGetCollection::AutoGetCollection(OperationContext* opCtx,
                                     const NamespaceStringOrUUID& nsOrUUID,
                                     LockMode modeColl,
                                     const Options& options)
    : AutoGetCollection(opCtx,
                        nsOrUUID,
                        modeColl,
                        options,
                        /*verifyWriteEligible=*/modeColl != MODE_IS) {}

AutoGetCollection::AutoGetCollection(OperationContext* opCtx,
                                     const NamespaceStringOrUUID& nsOrUUID,
                                     LockMode modeColl,
                                     const Options& options,
                                     ForReadTag reader)
    : AutoGetCollection(opCtx, nsOrUUID, modeColl, options, /*verifyWriteEligible=*/false) {}

AutoGetCollection::AutoGetCollection(OperationContext* opCtx,
                                     const NamespaceStringOrUUID& nsOrUUID,
                                     LockMode modeColl,
                                     const Options& options,
                                     bool verifyWriteEligible)
    : _autoDb(AutoGetDb::createForAutoGetCollection(opCtx, nsOrUUID, modeColl, options)) {

    auto& viewMode = options._viewMode;
    auto& deadline = options._deadline;
    auto& secondaryNssOrUUIDsBegin = options._secondaryNssOrUUIDsBegin;
    auto& secondaryNssOrUUIDsEnd = options._secondaryNssOrUUIDsEnd;

    // Out of an abundance of caution, force operations to acquire new snapshots after
    // acquiring exclusive collection locks. Operations that hold MODE_X locks make an
    // assumption that all writes are visible in their snapshot and no new writes will commit.
    // This may not be the case if an operation already has a snapshot open before acquiring an
    // exclusive lock.
    if (modeColl == MODE_X) {
        invariant(!shard_role_details::getRecoveryUnit(opCtx)->isActive(),
                  str::stream() << "Snapshot opened before acquiring X lock for "
                                << toStringForLogging(nsOrUUID));
    }

    // Acquire the collection locks. If there's only one lock, then it can simply be taken. If
    // there are many, however, the locks must be taken in _ascending_ ResourceId order to avoid
    // deadlocks across threads.
    if (secondaryNssOrUUIDsBegin == secondaryNssOrUUIDsEnd) {
        uassert(ErrorCodes::InvalidNamespace,
                fmt::format("Namespace {} is not a valid collection name",
                            nsOrUUID.toStringForErrorMsg()),
                nsOrUUID.isUUID() || (nsOrUUID.isNamespaceString() && nsOrUUID.nss().isValid()));

        _collLocks.emplace_back(opCtx, nsOrUUID, modeColl, deadline);
    } else {
        catalog_helper::acquireCollectionLocksInResourceIdOrder(opCtx,
                                                                nsOrUUID,
                                                                modeColl,
                                                                deadline,
                                                                secondaryNssOrUUIDsBegin,
                                                                secondaryNssOrUUIDsEnd,
                                                                &_collLocks);
    }

    // Wait for a configured amount of time after acquiring locks if the failpoint is enabled
    catalog_helper::setAutoGetCollectionWaitFailpointExecute(
        [&](const BSONObj& data) { sleepFor(Milliseconds(data["waitForMillis"].numberInt())); });

    auto catalog = CollectionCatalog::get(opCtx);
    auto databaseHolder = DatabaseHolder::get(opCtx);

    // Check that the collections are all safe to use.
    _resolvedNss = catalog->resolveNamespaceStringOrUUID(opCtx, nsOrUUID);
    _coll = CollectionPtr(catalog->lookupCollectionByNamespace(opCtx, _resolvedNss));
    _coll.makeYieldable(opCtx, LockedCollectionYieldRestore{opCtx, _coll});

    if (_coll) {
        // It is possible for an operation to have created the database and collection after this
        // AutoGetCollection initialized its AutoGetDb, but before it has performed the collection
        // lookup. Thus, it is possible for AutoGetDb to hold nullptr while _coll is a valid
        // pointer. This would be unexpected, as for a collection to exist the database must exist.
        // We ensure the database reference is valid by refreshing it.
        _autoDb.refreshDbReferenceIfNull(opCtx);
    }

    verifyDbAndCollection(
        opCtx, modeColl, nsOrUUID, _resolvedNss, _coll.get(), _autoDb.getDb(), verifyWriteEligible);
    for (auto iter = secondaryNssOrUUIDsBegin; iter != secondaryNssOrUUIDsEnd; ++iter) {
        const auto& secondaryNssOrUUID = *iter;
        auto secondaryResolvedNss =
            catalog->resolveNamespaceStringOrUUID(opCtx, secondaryNssOrUUID);
        auto secondaryColl = catalog->lookupCollectionByNamespace(opCtx, secondaryResolvedNss);
        auto secondaryDbName = secondaryNssOrUUID.dbName();
        verifyDbAndCollection(opCtx,
                              MODE_IS,
                              secondaryNssOrUUID,
                              secondaryResolvedNss,
                              secondaryColl,
                              databaseHolder->getDb(opCtx, secondaryDbName),
                              verifyWriteEligible);
    }

    const auto receivedShardVersion{
        OperationShardingState::get(opCtx).getShardVersion(_resolvedNss)};

    if (_coll) {
        // Fetch and store the sharding collection description data needed for use during the
        // operation. The shardVersion will be checked later if the shard filtering metadata is
        // fetched, ensuring both that the collection description info used here and the routing
        // table are consistent with the read request's shardVersion.
        //
        // Note: sharding versioning for an operation has no concept of multiple collections.
        auto scopedCss = CollectionShardingState::acquire(opCtx, _resolvedNss);
        scopedCss->checkShardVersionOrThrow(opCtx);

        auto collDesc = scopedCss->getCollectionDescription(opCtx);
        // TODO SERVER-79296 remove call to isSharded
        if (collDesc.isSharded()) {
            _coll.setShardKeyPattern(collDesc.getKeyPattern());
        }

        checkCollectionUUIDMismatch(opCtx, *catalog, _resolvedNss, _coll, options._expectedUUID);

        if (receivedShardVersion && *receivedShardVersion == ShardVersion::UNSHARDED()) {
            shard_role_details::checkLocalCatalogIsValidForUnshardedShardVersion(
                opCtx, *catalog, _coll, _resolvedNss);
        }

        if (receivedShardVersion) {
            shard_role_details::checkShardingAndLocalCatalogCollectionUUIDMatch(
                opCtx, _resolvedNss, *receivedShardVersion, collDesc, _coll);
        }

        return;
    }

    if (receivedShardVersion && *receivedShardVersion == ShardVersion::UNSHARDED()) {
        shard_role_details::checkLocalCatalogIsValidForUnshardedShardVersion(
            opCtx, *catalog, _coll, _resolvedNss);
    }

    if (!options._expectedUUID) {
        // We only need to look up a view if an expected collection UUID was not provided. If this
        // namespace were a view, the collection UUID mismatch check would have failed above.
        if ((_view = catalog->lookupView(opCtx, _resolvedNss))) {
            uassert(ErrorCodes::CommandNotSupportedOnView,
                    str::stream() << "Taking " << _resolvedNss.toStringForErrorMsg()
                                  << " lock for timeseries is not allowed",
                    viewMode == auto_get_collection::ViewMode::kViewsPermitted ||
                        !_view->timeseries());

            uassert(ErrorCodes::CommandNotSupportedOnView,
                    str::stream() << "Namespace " << _resolvedNss.toStringForErrorMsg()
                                  << " is a view, not a collection",
                    viewMode == auto_get_collection::ViewMode::kViewsPermitted);

            uassert(StaleConfigInfo(_resolvedNss,
                                    *receivedShardVersion,
                                    ShardVersion::UNSHARDED() /* wantedVersion */,
                                    ShardingState::get(opCtx)->shardId()),
                    str::stream() << "Namespace " << _resolvedNss.toStringForErrorMsg()
                                  << " is a view therefore the shard "
                                  << "version attached to the request must be unset or UNSHARDED",
                    !receivedShardVersion || *receivedShardVersion == ShardVersion::UNSHARDED());
            return;
        }
    }

    // There is neither a collection nor a view for the namespace, so if we reached to this point
    // there are the following possibilities depending on the received shard version:
    //   1. ShardVersion::UNSHARDED: The request comes from a router and the operation entails the
    //      implicit creation of an unsharded collection. We can continue.
    //   2. ChunkVersion::IGNORED: The request comes from a router that broadcasted the same to all
    //      shards, but this shard doesn't own any chunks for the collection. We can continue.
    //   3. boost::none: The request comes from client directly connected to the shard. We can
    //      continue.
    //   4. Any other value: The request comes from a stale router on a collection or a view which
    //      was deleted time ago (or the user manually deleted it from from underneath of sharding).
    //      We return a stale config error so that the router recovers.

    uassert(StaleConfigInfo(_resolvedNss,
                            *receivedShardVersion,
                            boost::none /* wantedVersion */,
                            ShardingState::get(opCtx)->shardId()),
            str::stream() << "No metadata for namespace " << _resolvedNss.toStringForErrorMsg()
                          << " therefore the shard "
                          << "version attached to the request must be unset, UNSHARDED or IGNORED",
            !receivedShardVersion || *receivedShardVersion == ShardVersion::UNSHARDED() ||
                ShardVersion::isPlacementVersionIgnored(*receivedShardVersion));

    checkCollectionUUIDMismatch(opCtx, *catalog, _resolvedNss, _coll, options._expectedUUID);
}

Collection* AutoGetCollection::getWritableCollection(OperationContext* opCtx) {
    invariant(_collLocks.size() == 1);

    // Acquire writable instance if not already available
    if (!_writableColl) {
        auto catalog = CollectionCatalog::get(opCtx);
        _writableColl = catalog->lookupCollectionByNamespaceForMetadataWrite(opCtx, _resolvedNss);
        // Makes the internal CollectionPtr Yieldable and resets the writable Collection when
        // the write unit of work finishes so we re-fetches and re-clones the Collection if a
        // new write unit of work is opened.
        shard_role_details::getRecoveryUnit(opCtx)->registerChange(
            [this](OperationContext* opCtx, boost::optional<Timestamp> commitTime) {
                _coll = CollectionPtr(_coll.get());
                _coll.makeYieldable(opCtx, LockedCollectionYieldRestore(opCtx, _coll));
                _writableColl = nullptr;
            },
            [this, originalCollection = _coll.get()](OperationContext* opCtx) {
                _coll = CollectionPtr(originalCollection);
                _coll.makeYieldable(opCtx, LockedCollectionYieldRestore(opCtx, _coll));
                _writableColl = nullptr;
            });

        // Set to writable collection. We are no longer yieldable.
        _coll = CollectionPtr(_writableColl);
    }
    return _writableColl;
}

struct CollectionWriter::SharedImpl {
    SharedImpl(CollectionWriter* parent) : _parent(parent) {}

    CollectionWriter* _parent;
    std::function<Collection*()> _writableCollectionInitializer;
};

CollectionWriter::CollectionWriter(OperationContext* opCtx, CollectionAcquisition* acquisition)
    : _acquisition(acquisition),
      _collection(&_storedCollection),
      _managed(true),
      _sharedImpl(std::make_shared<SharedImpl>(this)) {

    _storedCollection = CollectionPtr(
        CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(opCtx, _acquisition->nss()));
    _storedCollection.makeYieldable(opCtx, LockedCollectionYieldRestore(opCtx, _storedCollection));

    _sharedImpl->_writableCollectionInitializer = [this, opCtx]() mutable {
        if (!_fence) {
            _fence = std::make_unique<ScopedLocalCatalogWriteFence>(opCtx, _acquisition);
        }

        return CollectionCatalog::get(opCtx)->lookupCollectionByNamespaceForMetadataWrite(
            opCtx, _acquisition->nss());
    };
}

CollectionWriter::CollectionWriter(OperationContext* opCtx, const UUID& uuid)
    : _collection(&_storedCollection),
      _managed(true),
      _sharedImpl(std::make_shared<SharedImpl>(this)) {

    _storedCollection =
        CollectionPtr(CollectionCatalog::get(opCtx)->lookupCollectionByUUID(opCtx, uuid));
    _storedCollection.makeYieldable(opCtx, LockedCollectionYieldRestore(opCtx, _storedCollection));

    _sharedImpl->_writableCollectionInitializer = [opCtx, uuid]() {
        return CollectionCatalog::get(opCtx)->lookupCollectionByUUIDForMetadataWrite(opCtx, uuid);
    };
}

CollectionWriter::CollectionWriter(OperationContext* opCtx, const NamespaceString& nss)
    : _collection(&_storedCollection),
      _managed(true),
      _sharedImpl(std::make_shared<SharedImpl>(this)) {

    _storedCollection =
        CollectionPtr(CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(opCtx, nss));
    _storedCollection.makeYieldable(opCtx, LockedCollectionYieldRestore(opCtx, _storedCollection));

    _sharedImpl->_writableCollectionInitializer = [opCtx, nss]() {
        return CollectionCatalog::get(opCtx)->lookupCollectionByNamespaceForMetadataWrite(opCtx,
                                                                                          nss);
    };
}

CollectionWriter::CollectionWriter(OperationContext* opCtx, AutoGetCollection& autoCollection)
    : _collection(&autoCollection.getCollection()),
      _managed(true),
      _sharedImpl(std::make_shared<SharedImpl>(this)) {

    _sharedImpl->_writableCollectionInitializer = [&autoCollection, opCtx]() {
        return autoCollection.getWritableCollection(opCtx);
    };
}

CollectionWriter::CollectionWriter(Collection* writableCollection)
    : _collection(&_storedCollection),
      _storedCollection(writableCollection),
      _writableCollection(writableCollection),
      _managed(false) {}

CollectionWriter::~CollectionWriter() {
    // Notify shared state that this instance is destroyed
    if (_sharedImpl) {
        _sharedImpl->_parent = nullptr;
    }
}

Collection* CollectionWriter::getWritableCollection(OperationContext* opCtx) {
    // Acquire writable instance lazily if not already available
    if (!_writableCollection) {
        _writableCollection = _sharedImpl->_writableCollectionInitializer();

        // If we are using our stored Collection then we are not managed by an AutoGetCollection
        // and we need to manage lifetime here.
        if (_managed) {
            bool usingStoredCollection = *_collection == _storedCollection;
            auto rollbackCollection =
                usingStoredCollection ? std::move(_storedCollection) : CollectionPtr();

            // Resets the writable Collection when the write unit of work finishes so we re-fetch
            // and re-clone the Collection if a new write unit of work is opened. Holds the back
            // pointer to the CollectionWriter explicitly so we can detect if the instance is
            // already destroyed.
            shard_role_details::getRecoveryUnit(opCtx)->registerChange(
                [shared = _sharedImpl](OperationContext* opCtx, boost::optional<Timestamp>) {
                    if (shared->_parent) {
                        shared->_parent->_writableCollection = nullptr;

                        // Make the stored collection yieldable again as we now operate with the
                        // same instance as is in the catalog.
                        CollectionPtr& coll = shared->_parent->_storedCollection;
                        coll.makeYieldable(opCtx, LockedCollectionYieldRestore(opCtx, coll));
                    }
                },
                [shared = _sharedImpl, rollbackCollection = std::move(rollbackCollection)](
                    OperationContext* opCtx) mutable {
                    if (shared->_parent) {
                        shared->_parent->_writableCollection = nullptr;

                        // Restore stored collection to its previous state. The rollback
                        // instance is already yieldable.
                        shared->_parent->_storedCollection = std::move(rollbackCollection);
                    }
                });

            if (usingStoredCollection) {
                _storedCollection = CollectionPtr(_writableCollection);
            }
        }
    }
    return _writableCollection;
}

LockMode fixLockModeForSystemDotViewsChanges(const NamespaceString& nss, LockMode mode) {
    return nss.isSystemDotViews() ? MODE_X : mode;
}

ReadSourceScope::ReadSourceScope(OperationContext* opCtx,
                                 RecoveryUnit::ReadSource readSource,
                                 boost::optional<Timestamp> provided)
    : _opCtx(opCtx),
      _originalReadSource(shard_role_details::getRecoveryUnit(opCtx)->getTimestampReadSource()) {
    // Abandoning the snapshot is unsafe when the snapshot is managed by a lock free read
    // helper.
    invariant(!_opCtx->isLockFreeReadsOp());

    if (_originalReadSource == RecoveryUnit::ReadSource::kProvided) {
        _originalReadTimestamp =
            *shard_role_details::getRecoveryUnit(_opCtx)->getPointInTimeReadTimestamp(_opCtx);
    }

    shard_role_details::getRecoveryUnit(_opCtx)->abandonSnapshot();
    shard_role_details::getRecoveryUnit(_opCtx)->setTimestampReadSource(readSource, provided);
}

ReadSourceScope::~ReadSourceScope() {
    // Abandoning the snapshot is unsafe when the snapshot is managed by a lock free read
    // helper.
    invariant(!_opCtx->isLockFreeReadsOp());

    shard_role_details::getRecoveryUnit(_opCtx)->abandonSnapshot();
    if (_originalReadSource == RecoveryUnit::ReadSource::kProvided) {
        shard_role_details::getRecoveryUnit(_opCtx)->setTimestampReadSource(_originalReadSource,
                                                                            _originalReadTimestamp);
    } else {
        shard_role_details::getRecoveryUnit(_opCtx)->setTimestampReadSource(_originalReadSource);
    }
}

AutoGetOplog::AutoGetOplog(OperationContext* opCtx,
                           OplogAccessMode mode,
                           Date_t deadline,
                           const AutoGetOplogOptions& options) {
    auto lockMode = (mode == OplogAccessMode::kRead) ? MODE_IS : MODE_IX;
    if (mode == OplogAccessMode::kLogOp) {
        // Invariant that global lock is already held for kLogOp mode.
        invariant(shard_role_details::getLocker(opCtx)->isWriteLocked());
    } else {
        _globalLock.emplace(opCtx,
                            lockMode,
                            deadline,
                            Lock::InterruptBehavior::kThrow,
                            Lock::GlobalLockSkipOptions{.skipRSTLLock = options.skipRSTLLock});
    }

    _oplogInfo = LocalOplogInfo::get(opCtx);
    _oplog = CollectionPtr(_oplogInfo->getCollection());
    _oplog.makeYieldable(opCtx, LockedCollectionYieldRestore(opCtx, _oplog));
}

AutoGetChangeCollection::AutoGetChangeCollection(OperationContext* opCtx,
                                                 AutoGetChangeCollection::AccessMode mode,
                                                 const TenantId& tenantId,
                                                 Date_t deadline) {
    const auto changeCollectionNamespaceString = NamespaceString::makeChangeCollectionNSS(tenantId);
    if (AccessMode::kRead == mode || AccessMode::kWrite == mode) {
        // Treat this as a regular AutoGetCollection.
        _coll.emplace(opCtx,
                      changeCollectionNamespaceString,
                      mode == AccessMode::kRead ? MODE_IS : MODE_IX,
                      AutoGetCollection::Options{}.deadline(deadline));
        return;
    }
    tassert(6671506, "Invalid lock mode", AccessMode::kWriteInOplogContext == mode);

    // When writing to the change collection as part of normal operation, we avoid taking any new
    // locks. The caller must already have the tenant lock that protects the tenant specific change
    // stream collection from being dropped. That's sufficient for acquiring a raw collection
    // pointer.
    tassert(6671500,
            str::stream() << "Lock not held in IX mode for the tenant " << tenantId,
            shard_role_details::getLocker(opCtx)->isLockHeldForMode(
                ResourceId(ResourceType::RESOURCE_TENANT, tenantId), LockMode::MODE_IX));
    auto changeCollectionPtr = CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(
        opCtx, changeCollectionNamespaceString);
    _changeCollection = CollectionPtr(changeCollectionPtr);
    _changeCollection.makeYieldable(opCtx, LockedCollectionYieldRestore(opCtx, _changeCollection));
}

const Collection* AutoGetChangeCollection::operator->() const {
    return (**this).get();
}

const CollectionPtr& AutoGetChangeCollection::operator*() const {
    return (_coll) ? *(*_coll) : _changeCollection;
}

AutoGetChangeCollection::operator bool() const {
    return static_cast<bool>(**this);
}

}  // namespace mongo
