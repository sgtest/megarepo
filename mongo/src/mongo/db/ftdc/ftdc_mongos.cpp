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

#include <functional>
#include <memory>
#include <string>

#include <boost/filesystem/operations.hpp>
#include <boost/filesystem/path.hpp>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/client/connpool.h"
#include "mongo/client/dbclient_connection.h"
#include "mongo/client/global_conn_pool.h"
#include "mongo/client/replica_set_monitor_manager.h"
#include "mongo/db/ftdc/collector.h"
#include "mongo/db/ftdc/ftdc_mongos.h"
#include "mongo/db/ftdc/ftdc_server.h"
#include "mongo/db/ftdc/util.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/server_options.h"
#include "mongo/executor/connection_pool_stats.h"
#include "mongo/executor/task_executor.h"
#include "mongo/executor/task_executor_pool.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_component.h"
#include "mongo/s/grid.h"
#include "mongo/s/sharding_feature_flags_gen.h"
#include "mongo/transport/transport_layer_ftdc_collector.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kFTDC


namespace mongo {

class ConnPoolStatsCollector : public FTDCCollectorInterface {
public:
    void collect(OperationContext* opCtx, BSONObjBuilder& builder) override {
        executor::ConnectionPoolStats stats{};

        // Global connection pool connections.
        globalConnPool.appendConnectionStats(&stats);

        // Sharding connections.
        {
            auto const grid = Grid::get(opCtx);
            if (grid->getExecutorPool()) {
                grid->getExecutorPool()->appendConnectionStats(&stats);
            }

            auto const customConnPoolStatsFn = grid->getCustomConnectionPoolStatsFn();
            if (customConnPoolStatsFn) {
                customConnPoolStatsFn(&stats);
            }
        }

        // Output to a BSON object.
        builder.appendNumber("numClientConnections", DBClientConnection::getNumConnections());
        builder.appendNumber("numAScopedConnections", AScopedConnection::getNumConnections());
        stats.appendToBSON(builder, true /* forFTDC */);

        // All replica sets being tracked.
        ReplicaSetMonitorManager::get()->report(&builder, true /* forFTDC */);
    }

    std::string name() const override {
        return "connPoolStats";
    }
};

class NetworkInterfaceStatsCollector final : public FTDCCollectorInterface {
public:
    void collect(OperationContext* opCtx, BSONObjBuilder& builder) override {
        auto const grid = Grid::get(opCtx);
        if (auto executorPool = grid->getExecutorPool()) {
            executorPool->appendNetworkInterfaceStats(builder);
        }

        if (auto executor = ReplicaSetMonitorManager::get()->getExecutor()) {
            executor->appendNetworkInterfaceStats(builder);
        }
    }

    std::string name() const override {
        return "networkInterfaceStats";
    }
};

void registerMongoSCollectors(FTDCController* controller) {
    registerServerCollectorsForRole(controller, ClusterRole::RouterServer);

    // PoolStats
    controller->addPeriodicCollector(std::make_unique<ConnPoolStatsCollector>(),
                                     ClusterRole::RouterServer);

    controller->addPeriodicCollector(std::make_unique<NetworkInterfaceStatsCollector>(),
                                     ClusterRole::RouterServer);

    controller->addPeriodicCollector(std::make_unique<transport::TransportLayerFTDCCollector>(),
                                     ClusterRole::RouterServer);
}

void startMongoSFTDC(ServiceContext* serviceContext) {
    // Get the path to use for FTDC:
    // 1. Check if the user set one.
    // 2. If not, check if the user has a logpath and derive one.
    // 3. Otherwise, tell the user FTDC cannot run.

    // Only attempt to enable FTDC if we have a path to log files to.
    FTDCStartMode startMode = FTDCStartMode::kStart;
    auto directory = getFTDCDirectoryPathParameter();

    if (directory.empty()) {
        if (serverGlobalParams.logpath.empty()) {
            LOGV2_WARNING(23911,
                          "FTDC is disabled because neither '--logpath' nor set parameter "
                          "'diagnosticDataCollectionDirectoryPath' are specified.");
            startMode = FTDCStartMode::kSkipStart;
        } else {
            directory = boost::filesystem::absolute(
                FTDCUtil::getMongoSPath(serverGlobalParams.logpath), serverGlobalParams.cwd);

            // Note: If the computed FTDC directory conflicts with an existing file, then FTDC will
            // warn about the conflict, and not startup. It will not terminate MongoS in this
            // situation.
        }
    }

    // TODO (SERVER-87249): remove after the internal router is enabled by default (hardcode to
    // `true`).
    // (Ignore FCV check): This code is only executed in mongoS, and they're not FCV-gated anyway.
    const UseMultiserviceSchema multiserviceSchema{
        feature_flags::gMultiserviceFTDCSchema.isEnabledAndIgnoreFCVUnsafe() &&
        feature_flags::gEmbeddedRouter.isEnabledAndIgnoreFCVUnsafe()};

    startFTDC(serviceContext, directory, startMode, {registerMongoSCollectors}, multiserviceSchema);
}

void stopMongoSFTDC() {
    stopFTDC();
}

}  // namespace mongo
