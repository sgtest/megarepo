/**
 *    Copyright (C) 2022-present MongoDB, Inc.
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

#include <list>
#include <memory>
#include <utility>
#include <variant>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/db/catalog/collection.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/concurrency/locker.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/repl/read_concern_args.h"
#include "mongo/db/s/scoped_collection_metadata.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/db/views/view.h"
#include "mongo/s/database_version.h"
#include "mongo/s/shard_version.h"
#include "mongo/util/uuid.h"

namespace mongo {

struct PlacementConcern {
    boost::optional<DatabaseVersion> dbVersion;
    boost::optional<ShardVersion> shardVersion;
};

struct AcquisitionPrerequisites {
    // Pretends that the collection is unsharded. Acquisitions with this PlacementConcern will have
    // always have UNSHARDED description and filter, even if they are sharded. Only for use in
    // internal code paths that require it. Possible data loss if used incorrectly!
    static const PlacementConcern kPretendUnsharded;

    enum PlacementConcernPlaceholder {
        /**
         * Special PlacementConcern which mimics direct connection to a shard, causing the
         * acquisition to bypass any sharding checks and acquire just the local catalog portion. Any
         * sharding service values, such as the description or the filter are not allowed to be used
         * (will invariant).
         *
         * Note the *with potential data loss* in the name, which indicates that it allows the
         * caller to operate on a collection which is not even on the local shard, thus if used
         * incorrectly can lead to data loss.
         */
        kLocalCatalogOnlyWithPotentialDataLoss,
    };

    using PlacementConcernVariant = stdx::variant<PlacementConcern, PlacementConcernPlaceholder>;

    enum ViewMode { kMustBeCollection, kCanBeView };

    enum OperationType { kRead, kWrite };

    AcquisitionPrerequisites(NamespaceString nss,
                             boost::optional<UUID> uuid,
                             repl::ReadConcernArgs readConcern,
                             PlacementConcernVariant placementConcern,
                             OperationType operationType,
                             ViewMode viewMode)
        : nss(std::move(nss)),
          uuid(std::move(uuid)),
          readConcern(std::move(readConcern)),
          placementConcern(std::move(placementConcern)),
          operationType(operationType),
          viewMode(viewMode) {}

    NamespaceString nss;
    boost::optional<UUID> uuid;

    repl::ReadConcernArgs readConcern;
    PlacementConcernVariant placementConcern;
    OperationType operationType;
    ViewMode viewMode;
};

namespace shard_role_details {

struct AcquiredCollection {
    AcquiredCollection(AcquisitionPrerequisites prerequisites,
                       std::shared_ptr<Lock::DBLock> dbLock,
                       boost::optional<Lock::CollectionLock> collectionLock,
                       boost::optional<LockFreeReadsBlock> lockFreeReadsBlock,
                       boost::optional<Lock::GlobalLock> globalLock,
                       boost::optional<ScopedCollectionDescription> collectionDescription,
                       boost::optional<ScopedCollectionFilter> ownershipFilter,
                       CollectionPtr collectionPtr)
        : prerequisites(std::move(prerequisites)),
          dbLock(std::move(dbLock)),
          collectionLock(std::move(collectionLock)),
          lockFreeReadsBlock(std::move(lockFreeReadsBlock)),
          globalLock(std::move(globalLock)),
          collectionDescription(std::move(collectionDescription)),
          ownershipFilter(std::move(ownershipFilter)),
          collectionPtr(std::move(collectionPtr)),
          invalidated(false),
          sharedImpl(std::make_shared<SharedImpl>()) {}

    AcquiredCollection(AcquisitionPrerequisites prerequisites,
                       std::shared_ptr<Lock::DBLock> dbLock,
                       boost::optional<Lock::CollectionLock> collectionLock,
                       CollectionPtr collectionPtr)
        : AcquiredCollection(std::move(prerequisites),
                             std::move(dbLock),
                             std::move(collectionLock),
                             boost::none,
                             boost::none,
                             boost::none,
                             boost::none,
                             std::move(collectionPtr)){};

    AcquisitionPrerequisites prerequisites;

    std::shared_ptr<Lock::DBLock> dbLock;
    boost::optional<Lock::CollectionLock> collectionLock;

    boost::optional<LockFreeReadsBlock> lockFreeReadsBlock;
    boost::optional<Lock::GlobalLock> globalLock;

    boost::optional<ScopedCollectionDescription> collectionDescription;
    boost::optional<ScopedCollectionFilter> ownershipFilter;

    CollectionPtr collectionPtr;

    // Indicates whether this acquisition has been invalidated after a ScopedLocalCatalogWriteFence
    // was unable to restore it on rollback.
    bool invalidated;

    // Used by the ScopedLocalCatalogWriteFence to track the lifetime of AcquiredCollection.
    // ScopedLocalCatalogWriteFence will hold a weak_ptr pointing to 'sharedImpl'. The 'onRollback'
    // handler it installs will use that weak_ptr to determine if the AcquiredCollection is still
    // alive.
    // TODO: (jordist) SERVER-XXXXX Rework this.
    struct SharedImpl {};
    std::shared_ptr<SharedImpl> sharedImpl;
};

struct AcquiredView {
    AcquisitionPrerequisites prerequisites;

    std::shared_ptr<Lock::DBLock> dbLock;
    boost::optional<Lock::CollectionLock> collectionLock;

    std::shared_ptr<const ViewDefinition> viewDefinition;
};

/**
 * This class is a container for all the collection resources which are currently acquired by a
 * given operation. Operations consist of one or more transactions, which "acquire" and "release"
 * collections within their lifetime.
 *
 * Transactions start either explicitly (through the construction of a WUOW) or implicitly, from the
 * moment the first collection is acquired. They last until the last collection snapshot is released
 * or the WriteUnitOfWork commits (whichever is longer).
 *
 * Because of the above definition, within a transaction, acquisitions are always 2-phase, meaning
 * that acquiring a collection and then releasing it will defer the release until the transaction
 * actually commits. The boundaries of the transaction are considered to be the WUOW. If there is no
 * WUOW, the transaction ends when the snapshot is released.
 *
 * There are three steps associated with each acquisition:
 *
 *  - Locking: Acquiring the necessary lock manager locks in order to ensure stability of the
 * snapshot for the duration of the acquisition.
 *  - Snapshotting: Taking a consistent snapshot across all the "services" associated with the
 * collection (shard filter, storage catalog, data snapshot).
 *  - Resource reservation: This is service-specific and indicates setting the necessary state so
 * that the snapshot is consistent for the duration of the acquisition. Example of resource
 * acquisition is the RangePreserver, which blocks orphan cleanups.
 *
 * Acquiring a collection performs all three steps: locking, resource reservation and snapshotting.
 *
 * Releasing a collection performs the inverse of acquisition, freeing locks, reservations and the
 * snapshot, such that a new acquire may see newer state (if the readConcern of the transaction
 * permits it).
 *
 * Yielding *all* transaction resources only frees locks and the snapshot, but it keeps the resource
 * reservations.
 *
 * Restoring *all* transaction resources only performs locking and snapshotting (in accordance with
 * the read concern of the operation).
 */
struct TransactionResources {
    TransactionResources();

    TransactionResources(TransactionResources&&) = delete;
    TransactionResources& operator=(TransactionResources&&) = delete;

    TransactionResources(TransactionResources&) = delete;
    TransactionResources& operator=(TransactionResources&) = delete;

    ~TransactionResources();

    AcquiredCollection& addAcquiredCollection(AcquiredCollection&& acquiredCollection);
    const AcquiredView& addAcquiredView(AcquiredView&& acquiredView);

    void releaseAllResourcesOnCommitOrAbort() noexcept;

    /**
     * Asserts that this transaction context is not holding any collection acquisitions (i.e., it is
     * pristine). Used for invarianting in places where we do not expect an existing snapshot to
     * have been acquired because the caller expects to operate on latest.
     */
    void assertNoAcquiredCollections() const;

    // Indicates whether yield has been performed on these resources
    bool yielded{false};

    ////////////////////////////////////////////////////////////////////////////////////////
    // Global resources (cover all collections for the operation)

    // The read concern with which the transaction runs. All acquisitions must match that read
    // concern.
    boost::optional<repl::ReadConcernArgs> readConcern;

    // Set of locks acquired by the operation or nullptr if yielded.
    std::unique_ptr<Locker> locker;

    // If '_locker' has been yielded, contains a snapshot of the locks which have been yielded.
    // Otherwise boost::none.
    boost::optional<Locker::LockSnapshot> yieldedLocker;

    // The storage engine snapshot associated with this transaction (when yielded).
    struct YieldedRecoveryUnit {
        std::unique_ptr<RecoveryUnit> recoveryUnit;
        WriteUnitOfWork::RecoveryUnitState recoveryUnitState;
    };

    boost::optional<YieldedRecoveryUnit> yieldedRecoveryUnit;

    ////////////////////////////////////////////////////////////////////////////////////////
    // Per-collection resources

    // Set of all collections which are currently acquired
    std::list<AcquiredCollection> acquiredCollections;
    std::list<AcquiredView> acquiredViews;
};

}  // namespace shard_role_details
}  // namespace mongo
