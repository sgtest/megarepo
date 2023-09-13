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


#include <algorithm>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <chrono>
#include <compare>
#include <cstdint>
#include <limits>
#include <memory>
#include <mutex>
#include <ratio>
#include <string>
#include <utility>
#include <vector>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/bson/timestamp.h"
#include "mongo/db/auth/action_type.h"
#include "mongo/db/auth/authorization_session.h"
#include "mongo/db/auth/resource_pattern.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/catalog/collection_catalog_helper.h"
#include "mongo/db/catalog/health_log_gen.h"
#include "mongo/db/catalog/health_log_interface.h"
#include "mongo/db/catalog/index_catalog_entry.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/client.h"
#include "mongo/db/commands.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/curop.h"
#include "mongo/db/database_name.h"
#include "mongo/db/db_raii.h"
#include "mongo/db/dbhelpers.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/index/index_access_method.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/repl/dbcheck.h"
#include "mongo/db/repl/dbcheck_gen.h"
#include "mongo/db/repl/dbcheck_idl.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/oplog_entry.h"
#include "mongo/db/repl/oplog_entry_gen.h"
#include "mongo/db/repl/optime.h"
#include "mongo/db/repl/repl_server_parameters_gen.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/server_options.h"
#include "mongo/db/service_context.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/storage/sorted_data_interface.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/db/write_concern.h"
#include "mongo/db/write_concern_options.h"
#include "mongo/idl/command_generic_argument.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/stdx/thread.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/background.h"
#include "mongo/util/clock_source.h"
#include "mongo/util/debug_util.h"
#include "mongo/util/duration.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/namespace_string_util.h"
#include "mongo/util/progress_meter.h"
#include "mongo/util/time_support.h"
#include "mongo/util/uuid.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kCommand
MONGO_FAIL_POINT_DEFINE(hangBeforeReverseLookupCatalogSnapshot);
MONGO_FAIL_POINT_DEFINE(hangAfterReverseLookupCatalogSnapshot);

namespace mongo {

namespace {
MONGO_FAIL_POINT_DEFINE(hangBeforeProcessingDbCheckRun);
MONGO_FAIL_POINT_DEFINE(hangBeforeProcessingFirstBatch);

repl::OpTime _logOp(OperationContext* opCtx,
                    const NamespaceString& nss,
                    const boost::optional<UUID>& uuid,
                    const BSONObj& obj) {
    repl::MutableOplogEntry oplogEntry;
    oplogEntry.setOpType(repl::OpTypeEnum::kCommand);
    oplogEntry.setNss(nss);
    oplogEntry.setTid(nss.tenantId());
    oplogEntry.setUuid(uuid);
    oplogEntry.setObject(obj);
    AutoGetOplog oplogWrite(opCtx, OplogAccessMode::kWrite);
    return writeConflictRetry(
        opCtx, "dbCheck oplog entry", NamespaceString::kRsOplogNamespace, [&] {
            auto const clockSource = opCtx->getServiceContext()->getFastClockSource();
            oplogEntry.setWallClockTime(clockSource->now());

            WriteUnitOfWork uow(opCtx);
            repl::OpTime result = repl::logOp(opCtx, &oplogEntry);
            uow.commit();
            return result;
        });
}

/**
 * All the information needed to run dbCheck on a single collection.
 */
struct DbCheckCollectionInfo {
    NamespaceString nss;
    UUID uuid;
    BSONKey start;
    BSONKey end;
    int64_t maxCount;
    int64_t maxSize;
    int64_t maxRate;
    int64_t maxDocsPerBatch;
    int64_t maxBytesPerBatch;
    int64_t maxDocsPerSec;
    int64_t maxBytesPerSec;
    int64_t maxBatchTimeMillis;
    WriteConcernOptions writeConcern;
    boost::optional<SecondaryIndexCheckParameters> secondaryIndexCheckParameters;
};

/**
 * RAII-style class, which logs dbCheck start and stop events in the healthlog and replicates them.
 * The parameter info is boost::none when for a fullDatabaseRun where all collections are not
 * replicated.
 */
// TODO SERVER-79132: Remove boost::optional from _info once dbCheck no longer allows for full
// database run
class DbCheckStartAndStopLogger {
    boost::optional<DbCheckCollectionInfo> _info;

public:
    DbCheckStartAndStopLogger(OperationContext* opCtx, boost::optional<DbCheckCollectionInfo> info)
        : _info(info), _opCtx(opCtx) {
        try {
            DbCheckOplogStartStop oplogEntry;
            const auto nss = NamespaceString::kAdminCommandNamespace;
            oplogEntry.setNss(nss);
            oplogEntry.setType(OplogEntriesEnum::Start);

            auto healthLogEntry = dbCheckHealthLogEntry(boost::none /*nss*/,
                                                        boost::none /*collectionUUID*/,
                                                        SeverityEnum::Info,
                                                        "",
                                                        OplogEntriesEnum::Start,
                                                        boost::none /*data*/);
            if (_info && _info.value().secondaryIndexCheckParameters) {
                oplogEntry.setSecondaryIndexCheckParameters(
                    _info.value().secondaryIndexCheckParameters.value());
                healthLogEntry->setData(
                    _info.value().secondaryIndexCheckParameters.value().toBSON());
            }

            HealthLogInterface::get(_opCtx->getServiceContext())->log(*healthLogEntry);
            _logOp(_opCtx, nss, boost::none /*uuid*/, oplogEntry.toBSON());
        } catch (const DBException&) {
            LOGV2(6202200, "Could not log start event");
        }
    }

    ~DbCheckStartAndStopLogger() {
        try {
            DbCheckOplogStartStop oplogEntry;
            const auto nss = NamespaceString::kAdminCommandNamespace;
            oplogEntry.setNss(nss);
            oplogEntry.setType(OplogEntriesEnum::Stop);

            auto healthLogEntry = dbCheckHealthLogEntry(boost::none /*nss*/,
                                                        boost::none /*collectionUUID*/,
                                                        SeverityEnum::Info,
                                                        "",
                                                        OplogEntriesEnum::Stop,
                                                        boost::none /*data*/);
            if (_info && _info.value().secondaryIndexCheckParameters) {
                oplogEntry.setSecondaryIndexCheckParameters(
                    _info.value().secondaryIndexCheckParameters.value());
                healthLogEntry->setData(
                    _info.value().secondaryIndexCheckParameters.value().toBSON());
            }

            _logOp(_opCtx, nss, boost::none /*uuid*/, oplogEntry.toBSON());
            HealthLogInterface::get(_opCtx->getServiceContext())->log(*healthLogEntry);
        } catch (const DBException&) {
            LOGV2(6202201, "Could not log stop event");
        }
    }

private:
    OperationContext* _opCtx;
};

/**
 * A run of dbCheck consists of a series of collections.
 */
using DbCheckRun = std::vector<DbCheckCollectionInfo>;

std::unique_ptr<DbCheckRun> singleCollectionRun(OperationContext* opCtx,
                                                const DatabaseName& dbName,
                                                const DbCheckSingleInvocation& invocation) {
    const auto gSecondaryIndexChecksInDbCheck =
        repl::feature_flags::gSecondaryIndexChecksInDbCheck.isEnabled(
            serverGlobalParams.featureCompatibility);
    if (!gSecondaryIndexChecksInDbCheck) {
        uassert(ErrorCodes::InvalidOptions,
                "When featureFlagSecondaryIndexChecksInDbCheck is not enabled, the validateMode "
                "parameter cannot be set.",
                !invocation.getValidateMode());
    } else {
        if (invocation.getValidateMode() == mongo::DbCheckValidationModeEnum::extraIndexKeysCheck) {
            uassert(ErrorCodes::InvalidOptions,
                    "When validateMode is set to extraIndexKeysCheck, the secondaryIndex parameter "
                    "must be set.",
                    invocation.getSecondaryIndex());
        } else {
            uassert(ErrorCodes::InvalidOptions,
                    "When validateMode is set to dataConsistency or "
                    "dataConsistencyAndMissingIndexKeysCheck, the secondaryIndex parameter cannot "
                    "be set.",
                    !invocation.getSecondaryIndex());
            uassert(ErrorCodes::InvalidOptions,
                    "When validateMode is set to dataConsistency or "
                    "dataConsistencyAndMissingIndexKeysCheck, the skipLookupForExtraKeys parameter "
                    "cannot be set.",
                    !invocation.getSkipLookupForExtraKeys());
        }
    }
    NamespaceString nss(NamespaceStringUtil::deserialize(dbName, invocation.getColl()));

    boost::optional<UUID> uuid;
    try {
        AutoGetCollectionForRead agc(opCtx, nss);
        uassert(ErrorCodes::NamespaceNotFound,
                "Collection " + invocation.getColl() + " not found",
                agc.getCollection());
        uuid = agc->uuid();
    } catch (const DBException& ex) {
        // 'AutoGetCollectionForRead' fails with 'CommandNotSupportedOnView' if the namespace is
        // referring to a view.
        uassert(ErrorCodes::CommandNotSupportedOnView,
                invocation.getColl() + " is a view hence 'dbcheck' is not supported.",
                ex.code() != ErrorCodes::CommandNotSupportedOnView);
        throw;
    }

    uassert(40619,
            "Cannot run dbCheck on " + nss.toStringForErrorMsg() + " because it is not replicated",
            nss.isReplicated());

    uassert(6769500, "dbCheck no longer supports snapshotRead:false", invocation.getSnapshotRead());

    const auto start = invocation.getMinKey();
    const auto end = invocation.getMaxKey();
    const auto maxCount = invocation.getMaxCount();
    const auto maxSize = invocation.getMaxSize();
    const auto maxRate = invocation.getMaxCountPerSecond();
    const auto maxDocsPerBatch = invocation.getMaxDocsPerBatch();
    const auto maxBytesPerBatch = invocation.getMaxBytesPerBatch();
    const auto maxDocsPerSec = invocation.getMaxDocsPerSec();
    const auto maxBytesPerSec = invocation.getMaxBytesPerSec();
    const auto maxBatchTimeMillis = invocation.getMaxBatchTimeMillis();
    boost::optional<SecondaryIndexCheckParameters> secondaryIndexCheckParameters = boost::none;
    if (gSecondaryIndexChecksInDbCheck) {
        secondaryIndexCheckParameters = SecondaryIndexCheckParameters();
        secondaryIndexCheckParameters->setSkipLookupForExtraKeys(
            invocation.getSkipLookupForExtraKeys());
        if (invocation.getValidateMode()) {
            secondaryIndexCheckParameters->setValidateMode(invocation.getValidateMode().value());
        }
        if (invocation.getSecondaryIndex()) {
            secondaryIndexCheckParameters->setSecondaryIndex(
                invocation.getSecondaryIndex().value());
        }
    }
    const auto info = DbCheckCollectionInfo{nss,
                                            uuid.get(),
                                            start,
                                            end,
                                            maxCount,
                                            maxSize,
                                            maxRate,
                                            maxDocsPerBatch,
                                            maxBytesPerBatch,
                                            maxDocsPerSec,
                                            maxBytesPerSec,
                                            maxBatchTimeMillis,
                                            invocation.getBatchWriteConcern(),
                                            secondaryIndexCheckParameters};
    auto result = std::make_unique<DbCheckRun>();
    result->push_back(info);
    return result;
}

std::unique_ptr<DbCheckRun> fullDatabaseRun(OperationContext* opCtx,
                                            const DatabaseName& dbName,
                                            const DbCheckAllInvocation& invocation) {
    uassert(
        ErrorCodes::InvalidNamespace, "Cannot run dbCheck on local database", !dbName.isLocalDB());

    AutoGetDb agd(opCtx, dbName, MODE_IS);
    uassert(ErrorCodes::NamespaceNotFound,
            "Database " + dbName.toStringForErrorMsg() + " not found",
            agd.getDb());

    uassert(6769501, "dbCheck no longer supports snapshotRead:false", invocation.getSnapshotRead());

    const int64_t max = std::numeric_limits<int64_t>::max();
    const auto rate = invocation.getMaxCountPerSecond();
    const auto maxDocsPerBatch = invocation.getMaxDocsPerBatch();
    const auto maxBytesPerBatch = invocation.getMaxBytesPerBatch();
    const auto maxBatchTimeMillis = invocation.getMaxBatchTimeMillis();
    const auto maxDocsPerSec = invocation.getMaxDocsPerSec();
    const auto maxBytesPerSec = invocation.getMaxBytesPerSec();
    auto result = std::make_unique<DbCheckRun>();
    auto perCollectionWork = [&](const Collection* coll) {
        if (!coll->ns().isReplicated()) {
            return true;
        }
        DbCheckCollectionInfo info{coll->ns(),
                                   coll->uuid(),
                                   BSONKey::min(),
                                   BSONKey::max(),
                                   max,
                                   max,
                                   rate,
                                   maxDocsPerBatch,
                                   maxBytesPerBatch,
                                   maxDocsPerSec,
                                   maxBytesPerSec,
                                   maxBatchTimeMillis,
                                   invocation.getBatchWriteConcern(),
                                   boost::none};
        result->push_back(info);
        return true;
    };
    mongo::catalog::forEachCollectionFromDb(opCtx, dbName, MODE_IS, perCollectionWork);

    return result;
}


/**
 * Factory function for producing DbCheckRun's from command objects.
 */
std::unique_ptr<DbCheckRun> getRun(OperationContext* opCtx,
                                   const DatabaseName& dbName,
                                   const BSONObj& obj) {
    BSONObjBuilder builder;

    // Get rid of generic command fields.
    for (const auto& elem : obj) {
        const auto& fieldName = elem.fieldNameStringData();
        if (!isGenericArgument(fieldName)) {
            builder.append(elem);
        }
    }

    BSONObj toParse = builder.obj();

    // If the dbCheck argument is a string, this is the per-collection form.
    if (toParse["dbCheck"].type() == BSONType::String) {
        return singleCollectionRun(
            opCtx,
            dbName,
            DbCheckSingleInvocation::parse(
                IDLParserContext("", false /*apiStrict*/, dbName.tenantId()), toParse));
    } else {
        // Otherwise, it's the database-wide form.
        return fullDatabaseRun(
            opCtx,
            dbName,
            DbCheckAllInvocation::parse(
                IDLParserContext("", false /*apiStrict*/, dbName.tenantId()), toParse));
    }
}

std::shared_ptr<const CollectionCatalog> getConsistentCatalogAndSnapshot(OperationContext* opCtx) {
    // Loop until we get a consistent catalog and snapshot
    while (true) {
        auto catalogBeforeSnapshot = CollectionCatalog::get(opCtx);
        opCtx->recoveryUnit()->preallocateSnapshot();
        const auto catalogAfterSnapshot = CollectionCatalog::get(opCtx);
        if (catalogBeforeSnapshot == catalogAfterSnapshot) {
            return catalogBeforeSnapshot;
        }
        opCtx->recoveryUnit()->abandonSnapshot();
    }
}

/**
 * The BackgroundJob in which dbCheck actually executes on the primary.
 */
class DbCheckJob : public BackgroundJob {
public:
    DbCheckJob(Service* service, std::unique_ptr<DbCheckRun> run)
        : BackgroundJob(true), _service(service), _done(false), _run(std::move(run)) {}

protected:
    std::string name() const override {
        return "dbCheck";
    }

    void run() override {
        // Every dbCheck runs in its own client.
        ThreadClient tc(name(), _service);
        auto uniqueOpCtx = tc->makeOperationContext();
        auto opCtx = uniqueOpCtx.get();

        // DbCheckRun will be empty in a fullDatabaseRun where all collections are not replicated.
        // TODO SERVER-79132: Remove this logic once dbCheck no longer allows for a full database
        // run
        boost::optional<DbCheckCollectionInfo> info = boost::none;
        if (!_run->empty()) {
            info = _run->front();
        }
        DbCheckStartAndStopLogger startStop(opCtx, info);

        if (MONGO_unlikely(hangBeforeProcessingDbCheckRun.shouldFail())) {
            LOGV2(7949000, "Hanging dbcheck due to failpoint 'hangBeforeProcessingDbCheckRun'");
            hangBeforeProcessingDbCheckRun.pauseWhileSet();
        }

        for (const auto& coll : *_run) {
            try {
                _doCollection(opCtx, coll);
            } catch (const DBException& e) {
                auto logEntry = dbCheckErrorHealthLogEntry(
                    coll.nss, coll.uuid, "dbCheck failed", OplogEntriesEnum::Batch, e.toStatus());
                HealthLogInterface::get(Client::getCurrent()->getServiceContext())->log(*logEntry);
                return;
            }

            if (_done) {
                LOGV2(20451, "dbCheck terminated due to stepdown");
                return;
            }
        }
    }

private:
    /**
     * For organizing the results of batches for collection-level db check.
     */
    struct DbCheckCollectionBatchStats {
        int64_t nDocs;
        int64_t nBytes;
        BSONKey lastKey;
        std::string md5;
        repl::OpTime time;
        boost::optional<Timestamp> readTimestamp;
    };

    /**
     * For organizing the results of batches for extra index keys check.
     */
    struct DbCheckExtraIndexKeysBatchStats {
        int64_t nDocs;
        int64_t nBytes;
        key_string::Value lastIndexKey;
        key_string::Value nextLookupStart;
        bool finishedIndexBatch;
        bool finishedIndexCheck;
    };

    void _doCollection(OperationContext* opCtx, const DbCheckCollectionInfo& info) {
        if (_done) {
            return;
        }

        // TODO SERVER-78399: Clean up this check once feature flag is removed.
        boost::optional<SecondaryIndexCheckParameters> secondaryIndexCheckParameters =
            info.secondaryIndexCheckParameters;
        if (secondaryIndexCheckParameters) {
            mongo::DbCheckValidationModeEnum validateMode =
                secondaryIndexCheckParameters.get().getValidateMode();
            switch (validateMode) {
                case mongo::DbCheckValidationModeEnum::extraIndexKeysCheck: {
                    _extraIndexKeysCheck(opCtx, info);
                    return;
                }
                case mongo::DbCheckValidationModeEnum::dataConsistencyAndMissingIndexKeysCheck:
                case mongo::DbCheckValidationModeEnum::dataConsistency:
                    // _dataConsistencyCheck will check whether to do missingIndexKeysCheck.
                    _dataConsistencyCheck(opCtx, info);
                    return;
            }
            MONGO_UNREACHABLE;
        } else {
            _dataConsistencyCheck(opCtx, info);
        }
    }

    boost::optional<key_string::Value> getExtraIndexKeysCheckLookupStart(
        OperationContext* opCtx, const DbCheckCollectionInfo& info) {
        StringData indexName = info.secondaryIndexCheckParameters.get().getSecondaryIndex();
        const CollectionAcquisition collAcquisition = acquireCollectionMaybeLockFree(
            opCtx,
            CollectionAcquisitionRequest::fromOpCtx(
                opCtx, info.nss, AcquisitionPrerequisites::OperationType::kRead));
        const CollectionPtr& collection = collAcquisition.getCollectionPtr();
        const IndexDescriptor* index =
            collection.get()->getIndexCatalog()->findIndexByName(opCtx, indexName);

        if (!index) {
            Status status = Status(ErrorCodes::IndexNotFound,
                                   str::stream() << "cannot find index " << indexName << " for ns "
                                                 << info.nss.toStringForErrorMsg() << " and uuid "
                                                 << info.uuid.toString());
            const auto logEntry = dbCheckWarningHealthLogEntry(
                info.nss,
                info.uuid,
                "abandoning dbCheck extra index keys check because index no longer exists",
                OplogEntriesEnum::Batch,
                status);
            HealthLogInterface::get(opCtx)->log(*logEntry);
            return boost::none;
        }

        // TODO SERVER-79846: Add testing for progress meter
        // {
        //     const std::string curOpMessage = "Scanning index " + indexName +
        //         " for namespace " + NamespaceStringUtil::serialize(info.nss);
        //     stdx::unique_lock<Client> lk(*opCtx->getClient());
        //     progress.set(lk,
        //                  CurOp::get(opCtx)->setProgress_inlock(
        //                      StringData(curOpMessage), collection->numRecords(opCtx)),
        //                  opCtx);
        // }

        const IndexCatalogEntry* indexCatalogEntry =
            collection.get()->getIndexCatalog()->getEntry(index);
        auto iam = indexCatalogEntry->accessMethod()->asSortedData();
        const auto ordering = iam->getSortedDataInterface()->getOrdering();
        const key_string::Version version = iam->getSortedDataInterface()->getKeyStringVersion();

        key_string::Builder firstKeyString(
            version, BSONObj(), ordering, key_string::Discriminator::kExclusiveBefore);
        return firstKeyString.getValueCopy();
    }

    void _extraIndexKeysCheck(OperationContext* opCtx, const DbCheckCollectionInfo& info) {
        StringData indexName = info.secondaryIndexCheckParameters.get().getSecondaryIndex();

        // TODO SERVER-79846: Add testing for progress meter
        // ProgressMeterHolder progress;

        // Get catalog snapshot to look up the firstKey in the index.
        boost::optional<key_string::Value> maybeLookupStart =
            getExtraIndexKeysCheckLookupStart(opCtx, info);
        // If no first key was returned that means the index was not found, and we should exit the
        // dbCheck.
        if (!maybeLookupStart) {
            return;
        }
        key_string::Value lookupStart = maybeLookupStart.get();

        bool reachedEnd = false;

        int64_t totalBytesSeen = 0;
        int64_t totalKeysSeen = 0;
        using Clock = stdx::chrono::system_clock;
        using TimePoint = stdx::chrono::time_point<Clock>;
        TimePoint lastStart = Clock::now();
        int64_t docsInCurrentInterval = 0;

        do {
            using namespace std::literals::chrono_literals;

            if (Clock::now() - lastStart > 1s) {
                lastStart = Clock::now();
                docsInCurrentInterval = 0;
            }

            DbCheckExtraIndexKeysBatchStats batchStats = {0};

            // 1. Get batch bounds (stored in batchStats) and run reverse lookup if
            // skipLookupForExtraKeys is not set.
            // TODO SERVER-78449: Revisit case where skipLookupForExtraKeys is true, if we can
            // avoid doing two index walks (one for batching and one for hashing).
            auto batchFirst = lookupStart;
            _getExtraIndexKeysBatchAndRunReverseLookup(
                opCtx, info, indexName, lookupStart, batchStats);


            // 2. Get the last entry processed from reverse lookup.
            auto batchLast = batchStats.lastIndexKey;

            // 3. TODO SERVER-78449: Run hashing algorithm.

            // TODO SERVER-78449: Log batch into health log with range with correct info.
            _batchesProcessed++;
            BSONObjBuilder builder;
            builder.append("success", true);
            auto logEntry = dbCheckHealthLogEntry(info.nss,
                                                  info.uuid,
                                                  SeverityEnum::Info,
                                                  "db check batch",
                                                  OplogEntriesEnum::Batch,
                                                  builder.obj());

            if (kDebugBuild || logEntry->getSeverity() != SeverityEnum::Info ||
                (_batchesProcessed % gDbCheckHealthLogEveryNBatches.load() == 0)) {
                // On debug builds, health-log every batch result; on release builds, health-log
                // every N batches.
                HealthLogInterface::get(opCtx)->log(*logEntry);
            }

            // 4. Update lookupStart to resume the next batch.
            lookupStart = batchStats.nextLookupStart;

            // TODO SERVER-79846: Add testing for progress meter
            // {
            //     stdx::unique_lock<Client> lk(*opCtx->getClient());
            //     progress.get(lk)->hit(batchStats.nDocs);
            // }

            // 5. Check if we've exceeded any limits.
            totalBytesSeen += batchStats.nBytes;
            totalKeysSeen += batchStats.nDocs;
            docsInCurrentInterval += batchStats.nDocs;

            bool tooManyDocs = totalKeysSeen >= info.maxCount;
            bool tooManyBytes = totalBytesSeen >= info.maxSize;
            reachedEnd = batchStats.finishedIndexCheck || tooManyDocs || tooManyBytes;

            if (docsInCurrentInterval > info.maxRate && info.maxRate > 0) {
                // If an extremely low max rate has been set (substantially smaller than the
                // batch size) we might want to sleep for multiple seconds between batches.
                int64_t timesExceeded = docsInCurrentInterval / info.maxRate;

                stdx::this_thread::sleep_for(timesExceeded * 1s - (Clock::now() - lastStart));
            }

        } while (!reachedEnd);

        // TODO SERVER-79846: Add testing for progress meter
        // {
        //     stdx::unique_lock<Client> lk(*opCtx->getClient());
        //     progress.get(lk)->finished();
        // }
    }


    /**
     * Gets batch bounds for extra index keys check and stores the info in batchStats. Runs
     * reverse lookup if skipLookupForExtraKeys is not set.
     */
    void _getExtraIndexKeysBatchAndRunReverseLookup(OperationContext* opCtx,
                                                    const DbCheckCollectionInfo& info,
                                                    const StringData& indexName,
                                                    key_string::Value& lookupStart,
                                                    DbCheckExtraIndexKeysBatchStats& batchStats) {

        bool reachedBatchEnd = false;
        do {
            auto status = _getCatalogSnapshotAndRunReverseLookup(
                opCtx, info, indexName, lookupStart, batchStats);
            if (!status.isOK()) {
                LOGV2_DEBUG(7844807,
                            3,
                            "found one or more index inconsistencies with reverse lookup",
                            "status"_attr = status.reason(),
                            "indexName"_attr = indexName,
                            logAttrs(info.nss),
                            "uuid"_attr = info.uuid);
            }

            if (MONGO_unlikely(hangAfterReverseLookupCatalogSnapshot.shouldFail())) {
                LOGV2_DEBUG(
                    7844810, 3, "Hanging due to hangAfterReverseLookupCatalogSnapshot failpoint");
                hangAfterReverseLookupCatalogSnapshot.pauseWhileSet(opCtx);
            }

            reachedBatchEnd = batchStats.finishedIndexBatch;
            lookupStart = batchStats.nextLookupStart;
        } while (!reachedBatchEnd && !batchStats.finishedIndexCheck);
    }


    /**
     * Acquires a consistent catalog snapshot and iterates through the secondary index in order
     * to get the batch bounds. Runs reverse lookup if skipLookupForExtraKeys is not set.
     *
     * We release the snapshot by exiting the function. This occurs when we've either finished
     * the whole extra index keys check, finished one batch, or the number of keys we've looked
     * at has met or exceeded dbCheckMaxExtraIndexKeysReverseLookupPerSnapshot.
     */
    Status _getCatalogSnapshotAndRunReverseLookup(OperationContext* opCtx,
                                                  const DbCheckCollectionInfo& info,
                                                  const StringData& indexName,
                                                  const key_string::Value& lookupStart,
                                                  DbCheckExtraIndexKeysBatchStats& batchStats) {
        if (MONGO_unlikely(hangBeforeReverseLookupCatalogSnapshot.shouldFail())) {
            LOGV2_DEBUG(
                7844804, 3, "Hanging due to hangBeforeReverseLookupCatalogSnapshot failpoint");
            hangBeforeReverseLookupCatalogSnapshot.pauseWhileSet(opCtx);
        }

        Status status = Status::OK();
        const CollectionAcquisition collAcquisition = acquireCollectionMaybeLockFree(
            opCtx,
            CollectionAcquisitionRequest::fromOpCtx(
                opCtx, info.nss, AcquisitionPrerequisites::OperationType::kRead));
        const CollectionPtr& collection = collAcquisition.getCollectionPtr();
        const IndexDescriptor* index =
            collection.get()->getIndexCatalog()->findIndexByName(opCtx, indexName);

        if (!index) {
            status = Status(ErrorCodes::IndexNotFound,
                            str::stream() << "cannot find index " << indexName << " for ns "
                                          << info.nss.toStringForErrorMsg() << " and uuid "
                                          << info.uuid.toString());
            const auto logEntry = dbCheckWarningHealthLogEntry(
                info.nss,
                info.uuid,
                "abandoning dbCheck extra index keys check because index no longer exists",
                OplogEntriesEnum::Batch,
                status);
            HealthLogInterface::get(opCtx)->log(*logEntry);
            batchStats.finishedIndexBatch = true;
            batchStats.finishedIndexCheck = true;

            return status;
        }

        const IndexCatalogEntry* indexCatalogEntry =
            collection.get()->getIndexCatalog()->getEntry(index);
        auto iam = indexCatalogEntry->accessMethod()->asSortedData();
        const auto ordering = iam->getSortedDataInterface()->getOrdering();


        std::unique_ptr<SortedDataInterface::Cursor> indexCursor =
            iam->newCursor(opCtx, true /* forward */);

        // TODO SERVER-80158: Handle when user specifies a maxKey for extra index key check.

        // Creates a key greater than all other keys to set as the index cursor's end position.
        BSONObjBuilder builder;
        builder.appendMaxKey("");
        auto maxKey = Helpers::toKeyFormat(builder.obj());
        indexCursor->setEndPosition(maxKey, true /*inclusive*/);
        int64_t numKeys = 0;
        int64_t numBytes = 0;


        LOGV2_DEBUG(7844800,
                    3,
                    "starting extra index keys batch at",
                    "lookupStartKeyStringBson"_attr =
                        key_string::toBsonSafe(lookupStart.getBuffer(),
                                               lookupStart.getSize(),
                                               ordering,
                                               lookupStart.getTypeBits()),
                    "indexName"_attr = indexName,
                    logAttrs(info.nss),
                    "uuid"_attr = info.uuid);

        auto currIndexKey = indexCursor->seekForKeyString(lookupStart);

        // Note that if we can't find lookupStart (e.g. it was deleted in between snapshots),
        // seekForKeyString will automatically return the next adjacent keystring in the storage
        // engine. It will only return a null entry if there are no entries at all in the index.
        // Log for debug/testing purposes.
        if (!currIndexKey) {
            LOGV2_DEBUG(7844803,
                        3,
                        "could not find lookupStartKeyStringBson in index",
                        "lookupStartKeyStringBson"_attr =
                            key_string::toBsonSafe(lookupStart.getBuffer(),
                                                   lookupStart.getSize(),
                                                   ordering,
                                                   lookupStart.getTypeBits()),
                        "indexName"_attr = indexName,
                        logAttrs(info.nss),
                        "uuid"_attr = info.uuid);
        }

        while (currIndexKey) {
            const auto keyString = currIndexKey.get().keyString;
            const BSONObj keyStringBson = key_string::toBsonSafe(
                keyString.getBuffer(), keyString.getSize(), ordering, keyString.getTypeBits());

            if (!info.secondaryIndexCheckParameters.get().getSkipLookupForExtraKeys()) {
                status = _reverseLookup(opCtx,
                                        info,
                                        indexName,
                                        batchStats,
                                        collection,
                                        keyString,
                                        keyStringBson,
                                        iam,
                                        indexCatalogEntry);
            }

            batchStats.lastIndexKey = keyString;
            numBytes += keyString.getSize();
            numKeys++;
            batchStats.nBytes += keyString.getSize();
            batchStats.nDocs++;

            currIndexKey = indexCursor->nextKeyString();

            // Set nextLookupStart.
            if (currIndexKey) {
                batchStats.nextLookupStart = currIndexKey.get().keyString;
            }

            // TODO SERVER-79800: Fix handling of identical index keys.
            // If the next key is the same value as this one, we must look at them in the same
            // snapshot/batch, so skip this check.
            if (!(currIndexKey && (keyString == currIndexKey.get().keyString))) {
                // Check if we should finish this batch.
                if (batchStats.nBytes >= info.maxBytesPerBatch ||
                    batchStats.nDocs >= info.maxDocsPerBatch) {
                    batchStats.finishedIndexBatch = true;
                    break;
                }
                // Check if we should release snapshot.
                if (numKeys >= repl::dbCheckMaxExtraIndexKeysReverseLookupPerSnapshot.load()) {
                    break;
                }
            }
        }


        batchStats.finishedIndexCheck = !currIndexKey.is_initialized();
        LOGV2_DEBUG(7844808,
                    3,
                    "Catalog snapshot for extra index keys check ending",
                    "numKeys"_attr = numKeys,
                    "numBytes"_attr = numBytes,
                    "finishedIndexCheck"_attr = batchStats.finishedIndexCheck,
                    "finishedIndexBatch"_attr = batchStats.finishedIndexBatch,
                    logAttrs(info.nss),
                    "uuid"_attr = info.uuid);
        return status;
    }


    Status _reverseLookup(OperationContext* opCtx,
                          const DbCheckCollectionInfo& info,
                          const StringData& indexName,
                          DbCheckExtraIndexKeysBatchStats& batchStats,
                          const CollectionPtr& collection,
                          const key_string::Value& keyString,
                          const BSONObj& keyStringBson,
                          const SortedDataIndexAccessMethod* iam,
                          const IndexCatalogEntry* indexCatalogEntry) {
        // Check that the recordId exists in the record store.
        auto recordId = [&] {
            switch (collection->getRecordStore()->keyFormat()) {
                case KeyFormat::Long:
                    return key_string::decodeRecordIdLongAtEnd(keyString.getBuffer(),
                                                               keyString.getSize());
                case KeyFormat::String:
                    return key_string::decodeRecordIdStrAtEnd(keyString.getBuffer(),
                                                              keyString.getSize());
            }
            MONGO_UNREACHABLE;
        }();
        RecordData record;
        bool res = collection->getRecordStore()->findRecord(opCtx, recordId, &record);
        if (!res) {
            LOGV2_DEBUG(7844802,
                        3,
                        "reverse lookup failed to find record data",
                        "recordId"_attr = recordId.toStringHumanReadable(),
                        "keyString"_attr = keyStringBson,
                        "indexName"_attr = indexName,
                        logAttrs(info.nss),
                        "uuid"_attr = info.uuid);

            Status status =
                Status(ErrorCodes::KeyNotFound,
                       str::stream() << "cannot find document from recordId "
                                     << recordId.toStringHumanReadable() << " from index "
                                     << indexName << " for ns " << info.nss.toStringForErrorMsg());
            BSONObjBuilder context;
            context.append("indexName", indexName);
            context.append("keyString", keyStringBson);
            context.append("recordId", recordId.toStringHumanReadable());

            // TODO SERVER-79301: Update scope enums for health log entries.
            auto logEntry = dbCheckErrorHealthLogEntry(
                info.nss,
                info.uuid,
                "found extra index key entry without corresponding document",
                OplogEntriesEnum::Batch,
                status,
                context.done());
            HealthLogInterface::get(opCtx)->log(*logEntry);
            return status;
        }

        // Found record in record store.
        auto recordBson = record.toBson();

        // Generate the set of keys for the record data and check that it includes the
        // index key.
        // TODO SERVER-80278: Make sure wildcard/multikey indexes are handled correctly here.
        KeyStringSet foundKeys;
        KeyStringSet multikeyMetadataKeys;
        MultikeyPaths multikeyPaths;
        SharedBufferFragmentBuilder pool(key_string::HeapBuilder::kHeapAllocatorDefaultBytes);

        // A potential inefficiency with getKeys is that it generates all of the index keys
        // for this record for this secondary index, which means that if this index is a
        // multikey index, it could potentially be inefficient to generate all of them and only
        // check that it includes one specific keystring.
        iam->getKeys(opCtx,
                     collection,
                     indexCatalogEntry,
                     pool,
                     recordBson,
                     InsertDeleteOptions::ConstraintEnforcementMode::kEnforceConstraints,
                     SortedDataIndexAccessMethod::GetKeysContext::kValidatingKeys,
                     &foundKeys,
                     &multikeyMetadataKeys,
                     &multikeyPaths,
                     recordId);

        LOGV2_DEBUG(7844801,
                    3,
                    "reverse lookup found record data",
                    "recordData"_attr = recordBson,
                    "recordId"_attr = recordId.toStringHumanReadable(),
                    "expectedKeyString"_attr = keyStringBson,
                    "indexName"_attr = indexName,
                    logAttrs(info.nss),
                    "uuid"_attr = info.uuid);

        if (foundKeys.contains(keyString)) {
            return Status::OK();
        }

        LOGV2_DEBUG(7844809,
                    3,
                    "found index key entry with corresponding document/keystring set that "
                    "does not contain expected keystring",
                    "recordData"_attr = recordBson,
                    "recordId"_attr = recordId.toStringHumanReadable(),
                    "expectedKeyString"_attr = keyStringBson,
                    "indexName"_attr = indexName,
                    logAttrs(info.nss),
                    "uuid"_attr = info.uuid);
        Status status =
            Status(ErrorCodes::KeyNotFound,
                   str::stream() << "found index key entry with corresponding document and "
                                    "key string set that does not contain expected keystring "
                                 << keyStringBson << " from index " << indexName << " for ns "
                                 << info.nss.toStringForErrorMsg());
        BSONObjBuilder context;
        context.append("indexName", indexName);
        context.append("expectedKeyString", keyStringBson);
        context.append("recordId", recordId.toStringHumanReadable());
        context.append("recordData", recordBson);

        // TODO SERVER-79301: Update scope enums for health log entries.
        auto logEntry = dbCheckErrorHealthLogEntry(info.nss,
                                                   info.uuid,
                                                   "found index key entry with corresponding "
                                                   "document/keystring set that does not "
                                                   "contain the expected key string",
                                                   OplogEntriesEnum::Batch,
                                                   status,
                                                   context.done());
        HealthLogInterface::get(opCtx)->log(*logEntry);
        return status;
    }


    void _dataConsistencyCheck(OperationContext* opCtx, const DbCheckCollectionInfo& info) {
        const std::string curOpMessage =
            "Scanning namespace " + NamespaceStringUtil::serialize(info.nss);
        ProgressMeterHolder progress;
        {
            bool collectionFound = false;
            std::string collNotFoundMsg = "Collection under dbCheck no longer exists";
            try {
                AutoGetCollection coll(opCtx, info.nss, MODE_IS);
                if (coll) {
                    stdx::unique_lock<Client> lk(*opCtx->getClient());
                    progress.set(lk,
                                 CurOp::get(opCtx)->setProgress_inlock(StringData(curOpMessage),
                                                                       coll->numRecords(opCtx)),
                                 opCtx);
                    collectionFound = true;
                }
            } catch (const DBException& ex) {
                // 'AutoGetCollection' fails with 'CommandNotSupportedOnView' if the namespace is
                // referring to a view. This case can happen if the collection got dropped and then
                // a view got created with the same name before calling 'AutoGetCollection'.
                if (ex.code() != ErrorCodes::CommandNotSupportedOnView) {
                    throw;
                }
                collNotFoundMsg += ", but there is a view with the identical name";
            }

            if (!collectionFound) {
                const auto entry = dbCheckWarningHealthLogEntry(
                    info.nss,
                    info.uuid,
                    "abandoning dbCheck batch because collection no longer exists",
                    OplogEntriesEnum::Batch,
                    Status(ErrorCodes::NamespaceNotFound, collNotFoundMsg));
                HealthLogInterface::get(Client::getCurrent()->getServiceContext())->log(*entry);
                return;
            }
        }

        if (MONGO_unlikely(hangBeforeProcessingFirstBatch.shouldFail())) {
            LOGV2(7949001, "Hanging dbcheck due to failpoint 'hangBeforeProcessingFirstBatch'");
            hangBeforeProcessingFirstBatch.pauseWhileSet();
        }

        // Parameters for the hasher.
        auto start = info.start;
        bool reachedEnd = false;

        // Make sure the totals over all of our batches don't exceed the provided limits.
        int64_t totalBytesSeen = 0;
        int64_t totalDocsSeen = 0;

        // Limit the rate of the check.
        using Clock = stdx::chrono::system_clock;
        using TimePoint = stdx::chrono::time_point<Clock>;
        TimePoint lastStart = Clock::now();
        int64_t docsInCurrentInterval = 0;

        do {
            using namespace std::literals::chrono_literals;

            if (Clock::now() - lastStart > 1s) {
                lastStart = Clock::now();
                docsInCurrentInterval = 0;
            }

            auto result =
                _runBatch(opCtx, info, start, info.maxDocsPerBatch, info.maxBytesPerBatch);

            if (_done) {
                return;
            }


            if (!result.isOK()) {
                bool retryable = false;
                std::unique_ptr<HealthLogEntry> entry;

                const auto code = result.getStatus().code();
                if (code == ErrorCodes::LockTimeout) {
                    retryable = true;
                    entry = dbCheckWarningHealthLogEntry(
                        info.nss,
                        info.uuid,
                        "retrying dbCheck batch after timeout due to lock unavailability",
                        OplogEntriesEnum::Batch,
                        result.getStatus());
                } else if (code == ErrorCodes::SnapshotUnavailable) {
                    retryable = true;
                    entry = dbCheckWarningHealthLogEntry(
                        info.nss,
                        info.uuid,
                        "retrying dbCheck batch after conflict with pending catalog operation",
                        OplogEntriesEnum::Batch,
                        result.getStatus());
                } else if (code == ErrorCodes::NamespaceNotFound) {
                    entry = dbCheckWarningHealthLogEntry(
                        info.nss,
                        info.uuid,
                        "abandoning dbCheck batch because collection no longer exists",
                        OplogEntriesEnum::Batch,
                        result.getStatus());
                } else if (code == ErrorCodes::IndexNotFound) {
                    entry = dbCheckWarningHealthLogEntry(
                        info.nss,
                        info.uuid,
                        "skipping dbCheck on collection because it is missing an _id index",
                        OplogEntriesEnum::Batch,
                        result.getStatus());
                } else if (ErrorCodes::isA<ErrorCategory::NotPrimaryError>(code)) {
                    entry = dbCheckWarningHealthLogEntry(
                        info.nss,
                        info.uuid,
                        "stopping dbCheck because node is no longer primary",
                        OplogEntriesEnum::Batch,
                        result.getStatus());
                } else {
                    entry = dbCheckErrorHealthLogEntry(info.nss,
                                                       info.uuid,
                                                       "dbCheck batch failed",
                                                       OplogEntriesEnum::Batch,
                                                       result.getStatus());
                }
                HealthLogInterface::get(opCtx)->log(*entry);
                if (retryable) {
                    continue;
                }
                return;
            }

            const auto stats = result.getValue();

            _batchesProcessed++;
            auto entry = dbCheckBatchEntry(info.nss,
                                           info.uuid,
                                           stats.nDocs,
                                           stats.nBytes,
                                           stats.md5,
                                           stats.md5,
                                           start,
                                           stats.lastKey,
                                           stats.readTimestamp,
                                           stats.time);
            if (kDebugBuild || entry->getSeverity() != SeverityEnum::Info ||
                (_batchesProcessed % gDbCheckHealthLogEveryNBatches.load() == 0)) {
                // On debug builds, health-log every batch result; on release builds, health-log
                // every N batches.
                HealthLogInterface::get(opCtx)->log(*entry);
            }

            WriteConcernResult unused;
            auto status = waitForWriteConcern(opCtx, stats.time, info.writeConcern, &unused);
            if (!status.isOK()) {
                auto entry = dbCheckWarningHealthLogEntry(info.nss,
                                                          info.uuid,
                                                          "dbCheck failed waiting for writeConcern",
                                                          OplogEntriesEnum::Batch,
                                                          status);
                HealthLogInterface::get(opCtx)->log(*entry);
            }

            start = stats.lastKey;

            // Update our running totals.
            totalDocsSeen += stats.nDocs;
            totalBytesSeen += stats.nBytes;
            docsInCurrentInterval += stats.nDocs;
            {
                stdx::unique_lock<Client> lk(*opCtx->getClient());
                progress.get(lk)->hit(stats.nDocs);
            }

            // Check if we've exceeded any limits.
            bool reachedLast = stats.lastKey >= info.end;
            bool tooManyDocs = totalDocsSeen >= info.maxCount;
            bool tooManyBytes = totalBytesSeen >= info.maxSize;
            reachedEnd = reachedLast || tooManyDocs || tooManyBytes;

            if (docsInCurrentInterval > info.maxRate && info.maxRate > 0) {
                // If an extremely low max rate has been set (substantially smaller than the
                // batch size) we might want to sleep for multiple seconds between batches.
                int64_t timesExceeded = docsInCurrentInterval / info.maxRate;

                stdx::this_thread::sleep_for(timesExceeded * 1s - (Clock::now() - lastStart));
            }
        } while (!reachedEnd);

        {
            stdx::unique_lock<Client> lk(*opCtx->getClient());
            progress.get(lk)->finished();
        }
    }

    StatusWith<DbCheckCollectionBatchStats> _runBatch(OperationContext* opCtx,
                                                      const DbCheckCollectionInfo& info,
                                                      const BSONKey& first,
                                                      int64_t batchDocs,
                                                      int64_t batchBytes) {
        // Each batch will read at the latest no-overlap point, which is the all_durable
        // timestamp on primaries. We assume that the history window on secondaries is always
        // longer than the time it takes between starting and replicating a batch on the
        // primary. Otherwise, the readTimestamp will not be available on a secondary by the
        // time it processes the oplog entry.
        opCtx->recoveryUnit()->setTimestampReadSource(RecoveryUnit::ReadSource::kNoOverlap);

        // dbCheck writes to the oplog, so we need to take an IX lock. We don't need to write to
        // the collection, however, so we only take an intent lock on it.
        Lock::GlobalLock glob(opCtx, MODE_IX);

        // The CollectionCatalog to use for lock-free reads with point-in-time catalog lookups.
        std::shared_ptr<const CollectionCatalog> catalog = getConsistentCatalogAndSnapshot(opCtx);
        const Collection* collection = catalog->establishConsistentCollection(
            opCtx,
            {info.nss.dbName(), info.uuid},
            opCtx->recoveryUnit()->getPointInTimeReadTimestamp(opCtx));

        if (_stepdownHasOccurred(opCtx, info.nss)) {
            _done = true;
            return Status(ErrorCodes::PrimarySteppedDown, "dbCheck terminated due to stepdown");
        }

        if (!collection) {
            const auto msg = "Collection under dbCheck no longer exists";
            return {ErrorCodes::NamespaceNotFound, msg};
        }

        auto readTimestamp = opCtx->recoveryUnit()->getPointInTimeReadTimestamp(opCtx);
        uassert(ErrorCodes::SnapshotUnavailable,
                "No snapshot available yet for dbCheck",
                readTimestamp);

        // The CollectionPtr needs to outlive the DbCheckHasher as it's used internally.
        const CollectionPtr collectionPtr(collection);

        boost::optional<DbCheckHasher> hasher;
        try {
            hasher.emplace(opCtx,
                           collectionPtr,
                           first,
                           info.end,
                           std::min(batchDocs, info.maxCount),
                           std::min(batchBytes, info.maxSize));
        } catch (const DBException& e) {
            return e.toStatus();
        }

        const auto batchDeadline = Date_t::now() + Milliseconds(info.maxBatchTimeMillis);
        Status status = hasher->hashAll(opCtx, batchDeadline);

        if (!status.isOK()) {
            return status;
        }

        std::string md5 = hasher->total();

        DbCheckOplogBatch batch;
        batch.setType(OplogEntriesEnum::Batch);
        batch.setNss(info.nss);
        batch.setMd5(md5);
        batch.setMinKey(first);
        batch.setMaxKey(BSONKey(hasher->lastKey()));
        batch.setReadTimestamp(*readTimestamp);
        if (info.secondaryIndexCheckParameters) {
            batch.setSecondaryIndexCheckParameters(info.secondaryIndexCheckParameters);
        }

        // Send information on this batch over the oplog.
        DbCheckCollectionBatchStats result;
        result.time = _logOp(opCtx, info.nss, collection->uuid(), batch.toBSON());
        result.readTimestamp = readTimestamp;

        result.nDocs = hasher->docsSeen();
        result.nBytes = hasher->bytesSeen();
        result.lastKey = hasher->lastKey();
        result.md5 = md5;
        return result;
    }

    /**
     * Return `true` iff the primary the check is running on has stepped down.
     */
    bool _stepdownHasOccurred(OperationContext* opCtx, const NamespaceString& nss) {
        Status status = opCtx->checkForInterruptNoAssert();

        if (!status.isOK()) {
            return true;
        }

        auto coord = repl::ReplicationCoordinator::get(opCtx);

        if (!coord->canAcceptWritesFor(opCtx, nss)) {
            return true;
        }

        return false;
    }

    Service* _service;
    bool _done;  // Set if the job cannot proceed.
    std::unique_ptr<DbCheckRun> _run;

    // Cumulative number of batches processed. Can wrap around; it's not guaranteed to be in
    // lockstep with other replica set members.
    unsigned int _batchesProcessed = 0;
};

/**
 * The command, as run on the primary.
 */
class DbCheckCmd : public BasicCommand {
public:
    DbCheckCmd() : BasicCommand("dbCheck") {}

    AllowedOnSecondary secondaryAllowed(ServiceContext*) const override {
        return AllowedOnSecondary::kNever;
    }

    bool maintenanceOk() const override {
        return false;
    }

    virtual bool adminOnly() const {
        return false;
    }

    virtual bool supportsWriteConcern(const BSONObj& cmd) const override {
        return false;
    }

    std::string help() const override {
        return "Validate replica set consistency.\n"
               "Invoke with { dbCheck: <collection name/uuid>,\n"
               "              minKey: <first key, exclusive>,\n"
               "              maxKey: <last key, inclusive>,\n"
               "              maxCount: <try to keep a batch within maxCount number of docs>,\n"
               "              maxSize: <try to keep a batch withing maxSize of docs (bytes)>,\n"
               "              maxCountPerSecond: <max rate in docs/sec>\n"
               "              maxDocsPerBatch: <max number of docs/batch>\n"
               "              maxBytesPerBatch: <try to keep a batch within max bytes/batch>\n"
               "              maxBatchTimeMillis: <max time processing a batch in "
               "milliseconds>\n"
               "to check a collection.\n"
               "Invoke with {dbCheck: 1} to check all collections in the database.";
    }

    Status checkAuthForOperation(OperationContext* opCtx,
                                 const DatabaseName& dbName,
                                 const BSONObj&) const override {
        const bool isAuthorized =
            AuthorizationSession::get(opCtx->getClient())
                ->isAuthorizedForActionsOnResource(
                    ResourcePattern::forAnyResource(dbName.tenantId()), ActionType::dbCheck);
        return isAuthorized ? Status::OK() : Status(ErrorCodes::Unauthorized, "Unauthorized");
    }

    virtual bool run(OperationContext* opCtx,
                     const DatabaseName& dbName,
                     const BSONObj& cmdObj,
                     BSONObjBuilder& result) {
        auto job = getRun(opCtx, dbName, cmdObj);
        (new DbCheckJob(opCtx->getService(), std::move(job)))->go();
        return true;
    }
};
MONGO_REGISTER_COMMAND(DbCheckCmd);

}  // namespace
}  // namespace mongo
