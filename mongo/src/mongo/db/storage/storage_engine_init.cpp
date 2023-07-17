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

#include "mongo/db/storage/storage_engine_init.h"

#include <exception>
#include <map>
#include <string>
#include <utility>

#include <boost/filesystem/path.hpp>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/init.h"  // IWYU pragma: keep
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/storage/control/storage_control.h"
#include "mongo/db/storage/execution_control/concurrency_adjustment_parameters_gen.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/storage/recovery_unit_noop.h"
#include "mongo/db/storage/storage_engine_change_context.h"
#include "mongo/db/storage/storage_engine_feature_flags_gen.h"
#include "mongo/db/storage/storage_engine_lock_file.h"
#include "mongo/db/storage/storage_engine_metadata.h"
#include "mongo/db/storage/storage_engine_parameters_gen.h"
#include "mongo/db/storage/storage_options.h"
#include "mongo/db/storage/storage_parameters_gen.h"
#include "mongo/db/storage/storage_repair_observer.h"
#include "mongo/db/storage/ticketholder_manager.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/concurrency/priority_ticketholder.h"
#include "mongo/util/concurrency/semaphore_ticketholder.h"  // IWYU pragma: keep
#include "mongo/util/decorable.h"
#include "mongo/util/scopeguard.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kStorage

namespace mongo {
namespace {
/**
 * Creates the lock file used to prevent concurrent processes from accessing the data files,
 * as appropriate.
 */
void createLockFile(ServiceContext* service);
}  // namespace

StorageEngine::LastShutdownState initializeStorageEngine(OperationContext* opCtx,
                                                         const StorageEngineInitFlags initFlags) {
    ServiceContext* service = opCtx->getServiceContext();

    if (storageGlobalParams.restore) {
        uassert(6260400,
                "Cannot use --restore when the 'featureFlagSelectiveBackup' is disabled",
                feature_flags::gSelectiveBackup.isEnabledAndIgnoreFCVUnsafeAtStartup());
    }

    // This should be set once.
    if ((initFlags & StorageEngineInitFlags::kForRestart) == StorageEngineInitFlags{})
        invariant(!service->getStorageEngine());

    if ((initFlags & StorageEngineInitFlags::kAllowNoLockFile) == StorageEngineInitFlags{}) {
        createLockFile(service);
    }

    const std::string dbpath = storageGlobalParams.dbpath;

    StorageRepairObserver::set(service, std::make_unique<StorageRepairObserver>(dbpath));
    auto repairObserver = StorageRepairObserver::get(service);

    if (storageGlobalParams.repair) {
        repairObserver->onRepairStarted();
    } else if (repairObserver->isIncomplete()) {
        LOGV2_FATAL_NOTRACE(50922,
                            "An incomplete repair has been detected! This is likely because a "
                            "repair operation unexpectedly failed before completing. MongoDB will "
                            "not start up again without --repair.");
    }

    if (auto existingStorageEngine = StorageEngineMetadata::getStorageEngineForPath(dbpath)) {
        if (storageGlobalParams.engineSetByUser) {
            // Verify that the name of the user-supplied storage engine matches the contents of
            // the metadata file.
            const StorageEngine::Factory* factory =
                getFactoryForStorageEngine(service, storageGlobalParams.engine);
            if (factory) {
                uassert(28662,
                        str::stream()
                            << "Cannot start server. Detected data files in " << dbpath
                            << " created by"
                            << " the '" << *existingStorageEngine << "' storage engine, but the"
                            << " specified storage engine was '" << factory->getCanonicalName()
                            << "'.",
                        factory->getCanonicalName() == *existingStorageEngine);
            }
        } else {
            // Otherwise set the active storage engine as the contents of the metadata file.
            LOGV2(22270,
                  "Storage engine to use detected by data files",
                  "dbpath"_attr = boost::filesystem::path(dbpath).generic_string(),
                  "storageEngine"_attr = *existingStorageEngine);
            storageGlobalParams.engine = *existingStorageEngine;
        }
    }

    const StorageEngine::Factory* factory =
        getFactoryForStorageEngine(service, storageGlobalParams.engine);

    uassert(18656,
            str::stream() << "Cannot start server with an unknown storage engine: "
                          << storageGlobalParams.engine,
            factory);

    if (storageGlobalParams.queryableBackupMode) {
        uassert(34368,
                str::stream() << "Server was started in queryable backup mode, but the configured "
                              << "storage engine, " << storageGlobalParams.engine
                              << ", does not support queryable backup mode",
                factory->supportsQueryableBackupMode());
    }

    std::unique_ptr<StorageEngineMetadata> metadata;
    if ((initFlags & StorageEngineInitFlags::kSkipMetadataFile) == StorageEngineInitFlags{}) {
        metadata = StorageEngineMetadata::forPath(dbpath);
    }

    // Validate options in metadata against current startup options.
    if (metadata.get()) {
        uassertStatusOK(factory->validateMetadata(*metadata, storageGlobalParams));
    }

    // This should be set once during startup.
    if ((initFlags & StorageEngineInitFlags::kForRestart) == StorageEngineInitFlags{}) {
        auto readTransactions = gConcurrentReadTransactions.load();
        auto writeTransactions = gConcurrentWriteTransactions.load();
        static constexpr auto DEFAULT_TICKETS_VALUE = 128;
        bool userSetConcurrency = false;

        userSetConcurrency = readTransactions != 0 || writeTransactions != 0;
        readTransactions = readTransactions == 0 ? DEFAULT_TICKETS_VALUE : readTransactions;
        writeTransactions = writeTransactions == 0 ? DEFAULT_TICKETS_VALUE : writeTransactions;

        if (userSetConcurrency) {
            // If the user manually set concurrency limits, then disable execution control
            // implicitly.
            gStorageEngineConcurrencyAdjustmentAlgorithm = "fixedConcurrentTransactions";
        }

        auto svcCtx = opCtx->getServiceContext();
        if (feature_flags::gFeatureFlagDeprioritizeLowPriorityOperations
                .isEnabledAndIgnoreFCVUnsafeAtStartup()) {
            std::unique_ptr<TicketHolderManager> ticketHolderManager;
#ifdef __linux__
            LOGV2_DEBUG(6902900, 1, "Using Priority Queue-based ticketing scheduler");

            auto lowPriorityBypassThreshold = gLowPriorityAdmissionBypassThreshold.load();
            ticketHolderManager = std::make_unique<TicketHolderManager>(
                svcCtx,
                std::make_unique<PriorityTicketHolder>(
                    readTransactions, lowPriorityBypassThreshold, svcCtx),
                std::make_unique<PriorityTicketHolder>(
                    writeTransactions, lowPriorityBypassThreshold, svcCtx));
#else
            LOGV2_DEBUG(7207201, 1, "Using semaphore-based ticketing scheduler");

            // PriorityTicketHolder is implemented using an equivalent mechanism to
            // std::atomic::wait which isn't available until C++20. We've implemented it in Linux
            // using futex calls. As this hasn't been implemented in non-Linux platforms we fallback
            // to the existing semaphore implementation even if the feature flag is enabled.
            //
            // TODO SERVER-72616: Remove the ifdefs once TicketPool is implemented with atomic
            // wait.
            ticketHolderManager = std::make_unique<TicketHolderManager>(
                svcCtx,
                std::make_unique<SemaphoreTicketHolder>(readTransactions, svcCtx),
                std::make_unique<SemaphoreTicketHolder>(writeTransactions, svcCtx));
#endif
            TicketHolderManager::use(svcCtx, std::move(ticketHolderManager));
        } else {
            auto ticketHolderManager = std::make_unique<TicketHolderManager>(
                svcCtx,
                std::make_unique<SemaphoreTicketHolder>(readTransactions, svcCtx),
                std::make_unique<SemaphoreTicketHolder>(writeTransactions, svcCtx));
            TicketHolderManager::use(svcCtx, std::move(ticketHolderManager));
        }
    }

    ScopeGuard guard([&] {
        auto& lockFile = StorageEngineLockFile::get(service);
        if (lockFile) {
            lockFile->close();
        }
    });

    auto& lockFile = StorageEngineLockFile::get(service);
    if ((initFlags & StorageEngineInitFlags::kForRestart) == StorageEngineInitFlags{}) {
        auto storageEngine = std::unique_ptr<StorageEngine>(
            factory->create(opCtx, storageGlobalParams, lockFile ? &*lockFile : nullptr));
        service->setStorageEngine(std::move(storageEngine));
    } else {
        auto storageEngineChangeContext = StorageEngineChangeContext::get(service);
        auto token = storageEngineChangeContext->killOpsForStorageEngineChange(service);
        auto storageEngine = std::unique_ptr<StorageEngine>(
            factory->create(opCtx, storageGlobalParams, lockFile ? &*lockFile : nullptr));
        storageEngineChangeContext->changeStorageEngine(
            service, std::move(token), std::move(storageEngine));
    }

    if (lockFile) {
        uassertStatusOK(lockFile->writePid());
    }

    // Write a new metadata file if it is not present.
    if (!metadata.get() &&
        (initFlags & StorageEngineInitFlags::kSkipMetadataFile) == StorageEngineInitFlags{}) {
        metadata.reset(new StorageEngineMetadata(storageGlobalParams.dbpath));
        metadata->setStorageEngine(factory->getCanonicalName().toString());
        metadata->setStorageEngineOptions(factory->createMetadataOptions(storageGlobalParams));
        uassertStatusOK(metadata->write());
    }

    guard.dismiss();

    if (lockFile && lockFile->createdByUncleanShutdown()) {
        return StorageEngine::LastShutdownState::kUnclean;
    } else {
        return StorageEngine::LastShutdownState::kClean;
    }
}

namespace {
void shutdownGlobalStorageEngineCleanly(ServiceContext* service,
                                        Status errorToReport,
                                        bool forRestart) {
    auto storageEngine = service->getStorageEngine();
    invariant(storageEngine);
    // We always use 'forRestart' = false here because 'forRestart' = true is only appropriate if
    // we're going to restart controls on the same storage engine, which we are not here because we
    // are shutting the storage engine down. Additionally, we need to terminate any background
    // threads as they may be holding onto an OperationContext, as opposed to pausing them.
    StorageControl::stopStorageControls(service, errorToReport, /*forRestart=*/false);
    storageEngine->cleanShutdown(service);
    auto& lockFile = StorageEngineLockFile::get(service);
    if (lockFile) {
        lockFile->clearPidAndUnlock();
        lockFile = boost::none;
    }
}
} /* namespace */

void shutdownGlobalStorageEngineCleanly(ServiceContext* service) {
    shutdownGlobalStorageEngineCleanly(
        service,
        {ErrorCodes::ShutdownInProgress, "The storage catalog is being closed."},
        /*forRestart=*/false);
}

StorageEngine::LastShutdownState reinitializeStorageEngine(
    OperationContext* opCtx,
    StorageEngineInitFlags initFlags,
    std::function<void()> changeConfigurationCallback) {
    auto service = opCtx->getServiceContext();
    opCtx->recoveryUnit()->abandonSnapshot();
    shutdownGlobalStorageEngineCleanly(
        service,
        {ErrorCodes::InterruptedDueToStorageChange, "The storage engine is being reinitialized."},
        /*forRestart=*/true);
    opCtx->setRecoveryUnit(std::make_unique<RecoveryUnitNoop>(),
                           WriteUnitOfWork::RecoveryUnitState::kNotInUnitOfWork);
    changeConfigurationCallback();
    auto lastShutdownState =
        initializeStorageEngine(opCtx, initFlags | StorageEngineInitFlags::kForRestart);
    StorageControl::startStorageControls(service);
    return lastShutdownState;
}

namespace {

void createLockFile(ServiceContext* service) {
    auto& lockFile = StorageEngineLockFile::get(service);
    try {
        lockFile.emplace(storageGlobalParams.dbpath);
    } catch (const std::exception& ex) {
        uassert(28596,
                str::stream() << "Unable to determine status of lock file in the data directory "
                              << storageGlobalParams.dbpath << ": " << ex.what(),
                false);
    }
    const bool wasUnclean = lockFile->createdByUncleanShutdown();
    const auto openStatus = lockFile->open();
    if (openStatus == ErrorCodes::IllegalOperation) {
        lockFile = boost::none;
    } else {
        uassertStatusOK(openStatus);
    }

    if (wasUnclean) {
        LOGV2_WARNING(22271,
                      "Detected unclean shutdown - Lock file is not empty",
                      "lockFile"_attr = lockFile->getFilespec());
    }
}

using FactoryMap = std::map<std::string, std::unique_ptr<StorageEngine::Factory>>;

auto storageFactories = ServiceContext::declareDecoration<FactoryMap>();

}  // namespace

void registerStorageEngine(ServiceContext* service,
                           std::unique_ptr<StorageEngine::Factory> factory) {
    // No double-registering.
    invariant(!getFactoryForStorageEngine(service, factory->getCanonicalName()));

    // Some sanity checks: the factory must exist,
    invariant(factory);

    // and all factories should be added before we pick a storage engine.
    invariant(!service->getStorageEngine());

    auto name = factory->getCanonicalName().toString();
    storageFactories(service).emplace(name, std::move(factory));
}

bool isRegisteredStorageEngine(ServiceContext* service, StringData name) {
    return getFactoryForStorageEngine(service, name);
}

StorageEngine::Factory* getFactoryForStorageEngine(ServiceContext* service, StringData name) {
    const auto result = storageFactories(service).find(name.toString());
    if (result == storageFactories(service).end()) {
        return nullptr;
    }
    return result->second.get();
}

Status validateStorageOptions(
    ServiceContext* service,
    const BSONObj& storageEngineOptions,
    std::function<Status(const StorageEngine::Factory* const, const BSONObj&)> validateFunc) {

    BSONObjIterator storageIt(storageEngineOptions);
    while (storageIt.more()) {
        BSONElement storageElement = storageIt.next();
        StringData storageEngineName = storageElement.fieldNameStringData();
        if (storageElement.type() != mongo::Object) {
            return Status(ErrorCodes::BadValue,
                          str::stream() << "'storageEngine." << storageElement.fieldNameStringData()
                                        << "' has to be an embedded document.");
        }

        if (auto factory = getFactoryForStorageEngine(service, storageEngineName)) {
            Status status = validateFunc(factory, storageElement.Obj());
            if (!status.isOK()) {
                return status;
            }
        } else {
            return Status(ErrorCodes::InvalidOptions,
                          str::stream() << storageEngineName
                                        << " is not a registered storage engine for this server");
        }
    }
    return Status::OK();
}

namespace {
BSONArray storageEngineList(ServiceContext* service) {
    if (!service)
        return BSONArray();

    BSONArrayBuilder engineArrayBuilder;

    for (const auto& nameAndFactory : storageFactories(service)) {
        engineArrayBuilder.append(nameAndFactory.first);
    }

    return engineArrayBuilder.arr();
}
}  // namespace

void appendStorageEngineList(ServiceContext* service, BSONObjBuilder* result) {
    result->append("storageEngines", storageEngineList(service));
}

}  // namespace mongo
