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

#include "mongo/db/introspect.h"

#include <memory>
#include <mutex>
#include <ostream>
#include <string>
#include <utility>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/util/builder.h"
#include "mongo/db/auth/authorization_session.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/catalog/collection_options.h"
#include "mongo/db/catalog/collection_write_path.h"
#include "mongo/db/catalog/database.h"
#include "mongo/db/catalog/database_holder.h"
#include "mongo/db/client.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/concurrency/locker.h"
#include "mongo/db/concurrency/locker_impl.h"
#include "mongo/db/curop.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/read_concern_args.h"
#include "mongo/db/service_context.h"
#include "mongo/db/shard_role.h"
#include "mongo/db/stats/resource_consumption_metrics.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/db/transaction_resources.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/logv2/redaction.h"
#include "mongo/rpc/metadata/client_metadata.h"
#include "mongo/s/database_version.h"
#include "mongo/s/shard_version.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/scopeguard.h"
#include "mongo/util/str.h"
#include "mongo/util/time_support.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kDefault

namespace mongo {

using std::endl;
using std::string;
using std::unique_ptr;

void profile(OperationContext* opCtx, NetworkOp op) {
    // Initialize with 1kb at start in order to avoid realloc later
    BufBuilder profileBufBuilder(1024);

    BSONObjBuilder b(profileBufBuilder);

    {
        Locker::LockerInfo lockerInfo;
        opCtx->lockState()->getLockerInfo(&lockerInfo, CurOp::get(opCtx)->getLockStatsBase());
        CurOp::get(opCtx)->debug().append(
            opCtx, lockerInfo.stats, opCtx->lockState()->getFlowControlStats(), b);
    }

    auto& metricsCollector = ResourceConsumption::MetricsCollector::get(opCtx);
    if (metricsCollector.hasCollectedMetrics()) {
        BSONObjBuilder metricsBuilder = b.subobjStart("operationMetrics");
        const auto& metrics = metricsCollector.getMetrics();
        metrics.toBson(&metricsBuilder);
        metricsBuilder.done();
    }

    b.appendDate("ts", jsTime());
    b.append("client", opCtx->getClient()->clientAddress());

    if (auto clientMetadata = ClientMetadata::get(opCtx->getClient())) {
        auto appName = clientMetadata->getApplicationName();
        if (!appName.empty()) {
            b.append("appName", appName);
        }
    }

    AuthorizationSession* authSession = AuthorizationSession::get(opCtx->getClient());
    OpDebug::appendUserInfo(*CurOp::get(opCtx), b, authSession);

    const BSONObj p = b.done();

    const auto ns = CurOp::get(opCtx)->getNSS();

    try {
        // We create a new opCtx so that we aren't interrupted by having the original operation
        // killed or timed out. Those are the case we want to have profiling data.
        auto newClient = opCtx->getServiceContext()->makeClient("profiling");
        auto newCtx = newClient->makeOperationContext();

        // TODO(SERVER-74657): Please revisit if this thread could be made killable.
        {
            stdx::lock_guard<Client> lk(*newClient.get());
            newClient.get()->setSystemOperationUnkillableByStepdown(lk);
        }

        // We swap the lockers as that way we preserve locks held in transactions and any other
        // options set for the locker like maxLockTimeout.
        auto oldLocker = opCtx->getClient()->swapLockState(
            std::make_unique<LockerImpl>(opCtx->getServiceContext()));
        auto emptyLocker = newClient->swapLockState(std::move(oldLocker));
        ON_BLOCK_EXIT([&] {
            auto oldCtxLocker = newClient->swapLockState(std::move(emptyLocker));
            opCtx->getClient()->swapLockState(std::move(oldCtxLocker));
        });
        AlternativeClientRegion acr(newClient);
        const auto dbProfilingNS = NamespaceString::makeSystemDotProfileNamespace(ns.dbName());

        boost::optional<CollectionAcquisition> profileCollection;
        while (true) {
            profileCollection.emplace(
                acquireCollection(newCtx.get(),
                                  CollectionAcquisitionRequest(
                                      dbProfilingNS,
                                      PlacementConcern{boost::none, ShardVersion::UNSHARDED()},
                                      repl::ReadConcernArgs::get(newCtx.get()),
                                      AcquisitionPrerequisites::kWrite),
                                  MODE_IX));

            Database* const db =
                DatabaseHolder::get(newCtx.get())->getDb(newCtx.get(), dbProfilingNS.dbName());
            if (!db) {
                // Database disappeared.
                LOGV2(20700,
                      "note: not profiling because db went away for {namespace}",
                      "note: not profiling because db went away for namespace",
                      logAttrs(ns));
                return;
            }

            if (profileCollection->exists()) {
                break;
            }

            uassertStatusOK(createProfileCollection(newCtx.get(), db));
            profileCollection.reset();
        }

        invariant(profileCollection && profileCollection->exists());

        WriteUnitOfWork wuow(newCtx.get());
        OpDebug* const nullOpDebug = nullptr;
        uassertStatusOK(collection_internal::insertDocument(newCtx.get(),
                                                            profileCollection->getCollectionPtr(),
                                                            InsertStatement(p),
                                                            nullOpDebug,
                                                            false));
        wuow.commit();
    } catch (const AssertionException& assertionEx) {
        LOGV2_WARNING(20703,
                      "Caught Assertion while trying to profile {operation} against "
                      "{namespace}: {assertion}",
                      "Caught Assertion while trying to profile operation",
                      "operation"_attr = networkOpToString(op),
                      logAttrs(ns),
                      "assertion"_attr = redact(assertionEx));
    }
}

Status createProfileCollection(OperationContext* opCtx, Database* db) {
    invariant(opCtx->lockState()->isDbLockedForMode(db->name(), MODE_IX));

    const auto dbProfilingNS = NamespaceString::makeSystemDotProfileNamespace(db->name());

    // Checking the collection exists must also be done in the WCE retry loop. Only retrying
    // collection creation would endlessly throw errors because the collection exists: must check
    // and see the collection exists in order to break free.
    return writeConflictRetry(opCtx, "createProfileCollection", dbProfilingNS, [&] {
        const Collection* collection =
            CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(opCtx, dbProfilingNS);
        if (collection) {
            if (!collection->isCapped()) {
                return Status(ErrorCodes::NamespaceExists,
                              str::stream() << dbProfilingNS.toStringForErrorMsg()
                                            << " exists but isn't capped");
            }

            return Status::OK();
        }

        // system.profile namespace doesn't exist; create it
        LOGV2(20701,
              "Creating profile collection: {namespace}",
              "Creating profile collection",
              logAttrs(dbProfilingNS));

        CollectionOptions collectionOptions;
        collectionOptions.capped = true;
        collectionOptions.cappedSize = 1024 * 1024;

        WriteUnitOfWork wunit(opCtx);
        repl::UnreplicatedWritesBlock uwb(opCtx);
        invariant(db->createCollection(opCtx, dbProfilingNS, collectionOptions));
        wunit.commit();

        return Status::OK();
    });
}

}  // namespace mongo
