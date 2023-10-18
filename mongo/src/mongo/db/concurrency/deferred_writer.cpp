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

#include "mongo/db/concurrency/deferred_writer.h"

#include <boost/move/utility_core.hpp>
#include <compare>
#include <functional>
#include <mutex>
#include <utility>

#include <boost/none.hpp>

#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/catalog/create_collection.h"
#include "mongo/db/client.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/dbhelpers.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/repl/read_concern_args.h"
#include "mongo/db/shard_role.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/db/transaction_resources.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/s/database_version.h"
#include "mongo/s/shard_version.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/concurrency/thread_pool.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kWrite

namespace mongo {

namespace {
auto kLogInterval = stdx::chrono::minutes(1);
}

void DeferredWriter::_logFailure(const Status& status) {
    if (TimePoint::clock::now() - _lastLogged > kLogInterval) {
        LOGV2(20516, "Unable to write to collection", logAttrs(_nss), "error"_attr = status);
        _lastLogged = stdx::chrono::system_clock::now();
    }
}

void DeferredWriter::_logDroppedEntry() {
    _droppedEntries += 1;
    if (TimePoint::clock::now() - _lastLoggedDrop > kLogInterval) {
        LOGV2(20517,
              "Deferred write buffer for {namespace} is full. {droppedEntries} entries have been "
              "dropped.",
              logAttrs(_nss),
              "droppedEntries"_attr = _droppedEntries);
        _lastLoggedDrop = stdx::chrono::system_clock::now();
        _droppedEntries = 0;
    }
}

Status DeferredWriter::_makeCollection(OperationContext* opCtx) {
    BSONObjBuilder builder;
    builder.append("create", _nss.coll());
    builder.appendElements(_collectionOptions.toBSON());
    try {
        return createCollection(opCtx, _nss.dbName(), builder.obj().getOwned());
    } catch (const DBException& exception) {
        return exception.toStatus();
    }
}

StatusWith<CollectionAcquisition> DeferredWriter::_getCollection(OperationContext* opCtx) {
    while (true) {
        {
            auto collection =
                acquireCollection(opCtx,
                                  CollectionAcquisitionRequest(
                                      _nss,
                                      PlacementConcern{boost::none, ShardVersion::UNSHARDED()},
                                      repl::ReadConcernArgs::get(opCtx),
                                      AcquisitionPrerequisites::kWrite),
                                  MODE_IX);
            if (collection.exists()) {
                return std::move(collection);
            }
        }

        // Release the lockS before trying to rebuild the collection.
        Status status = _makeCollection(opCtx);
        if (!status.isOK()) {
            return status;
        }
    }
}

Status DeferredWriter::_worker(BSONObj doc) noexcept try {
    auto uniqueOpCtx = Client::getCurrent()->makeOperationContext();
    OperationContext* opCtx = uniqueOpCtx.get();
    auto result = _getCollection(opCtx);

    if (!result.isOK()) {
        return result.getStatus();
    }

    const auto collection = std::move(result.getValue());

    Status status = writeConflictRetry(opCtx, "deferred insert", _nss, [&] {
        WriteUnitOfWork wuow(opCtx);
        Status status = Helpers::insert(opCtx, collection, doc);
        if (!status.isOK()) {
            return status;
        }

        wuow.commit();
        return Status::OK();
    });

    stdx::lock_guard<Latch> lock(_mutex);

    _numBytes -= doc.objsize();
    return status;
} catch (const DBException& e) {
    return e.toStatus();
}

DeferredWriter::DeferredWriter(NamespaceString nss, CollectionOptions opts, int64_t maxSize)
    : _collectionOptions(opts),
      _maxNumBytes(maxSize),
      _nss(nss),
      _numBytes(0),
      _droppedEntries(0),
      _lastLogged(TimePoint::clock::now() - kLogInterval) {}

DeferredWriter::~DeferredWriter() {}

void DeferredWriter::startup(std::string workerName) {
    // We should only start up once.
    invariant(!_pool);
    ThreadPool::Options options;
    options.poolName = "deferred writer pool";
    options.threadNamePrefix = workerName;
    options.minThreads = 0;
    options.maxThreads = 1;
    options.onCreateThread = [](const std::string& name) {
        Client::initThread(name, getGlobalServiceContext()->getService(ClusterRole::ShardServer));
    };
    _pool = std::make_unique<ThreadPool>(options);
    _pool->startup();
}

void DeferredWriter::shutdown(void) {
    // If we never allocated the pool, no cleanup is necessary.
    if (!_pool) {
        return;
    }

    _pool->waitForIdle();
    _pool->shutdown();
    _pool->join();
}

bool DeferredWriter::insertDocument(BSONObj obj) {
    // We can't insert documents if we haven't been started up.
    invariant(_pool);

    stdx::lock_guard<Latch> lock(_mutex);

    // Check if we're allowed to insert this object.
    if (_numBytes + obj.objsize() >= _maxNumBytes) {
        // If not, drop it.  We always drop new entries rather than old ones; that way the
        // caller knows at the time of the call that the entry was dropped.
        _logDroppedEntry();
        return false;
    }

    // Add the object to the buffer.
    _numBytes += obj.objsize();
    _pool->schedule([this, obj](auto status) {
        fassert(40588, status);

        auto workerStatus = _worker(obj.getOwned());
        if (!workerStatus.isOK()) {
            _logFailure(workerStatus);
        }
    });
    return true;
}

int64_t DeferredWriter::getDroppedEntries() {
    return _droppedEntries;
}


}  // namespace mongo
