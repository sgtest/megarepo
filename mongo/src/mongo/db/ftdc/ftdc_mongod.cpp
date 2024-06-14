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

#include <boost/filesystem/path.hpp>
#include <fmt/format.h>
#include <memory>

#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/cluster_role.h"
#include "mongo/db/commands.h"
#include "mongo/db/ftdc/collector.h"
#include "mongo/db/ftdc/constants.h"
#include "mongo/db/ftdc/controller.h"
#include "mongo/db/ftdc/ftdc_mongod.h"
#include "mongo/db/ftdc/ftdc_mongod_gen.h"
#include "mongo/db/ftdc/ftdc_mongos.h"
#include "mongo/db/ftdc/ftdc_server.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/storage/storage_options.h"
#include "mongo/rpc/op_msg.h"
#include "mongo/s/sharding_feature_flags_gen.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/synchronized_value.h"

namespace mongo {

Status validateCollectionStatsNamespaces(const std::vector<std::string> value,
                                         const boost::optional<TenantId>& tenantId) {
    try {
        for (const auto& nsStr : value) {
            const auto ns = NamespaceStringUtil::deserialize(
                tenantId, nsStr, SerializationContext::stateDefault());

            if (!ns.isValid()) {
                return Status(ErrorCodes::BadValue,
                              fmt::format("'{}' is not a valid namespace", nsStr));
            }
        }
    } catch (...) {
        return exceptionToStatus();
    }

    return Status::OK();
}

namespace {

class FTDCCollectionStatsCollector final : public FTDCCollectorInterface {
public:
    bool hasData() const override {
        return !gDiagnosticDataCollectionStatsNamespaces->empty();
    }

    void collect(OperationContext* opCtx, BSONObjBuilder& builder) override {
        std::vector<std::string> namespaces = gDiagnosticDataCollectionStatsNamespaces.get();

        for (const auto& nsStr : namespaces) {

            try {
                // TODO SERVER-74464 tenantId needs to be passed.
                const auto ns =
                    NamespaceStringUtil::parseFromStringExpectTenantIdInMultitenancyMode(nsStr);
                auto result = CommandHelpers::runCommandDirectly(
                    opCtx,
                    OpMsgRequestBuilder::create(
                        auth::ValidatedTenancyScope::get(opCtx),
                        ns.dbName(),
                        BSON("aggregate" << ns.coll() << "cursor" << BSONObj{} << "pipeline"
                                         << BSON_ARRAY(BSON("$collStats" << BSON(
                                                                "storageStats" << BSON(
                                                                    "waitForLock" << false)))))));
                builder.append(nsStr, result["cursor"]["firstBatch"]["0"].Obj());

            } catch (...) {
                Status s = exceptionToStatus();
                builder.append("error", s.toString());
            }
        }
    }

    std::string name() const override {
        return "collectionStats";
    }
};

static const BSONArray pipelineObj =
    BSONArrayBuilder{}
        .append(BSONObjBuilder{}
                    .append("$collStats",
                            BSONObjBuilder{}
                                .append("storageStats",
                                        BSONObjBuilder{}
                                            .append("waitForLock", false)
                                            .append("numericOnly", true)
                                            .obj())
                                .obj())
                    .obj())
        .arr();

static const BSONObj getParameterQueryObj =
    BSONObjBuilder{}
        .append("getParameter",
                BSONObjBuilder{}.append("allParameters", true).append("setAt", "runtime").obj())
        .obj();

static const BSONObj getClusterParameterQueryObj =
    BSONObjBuilder{}.append("getClusterParameter", "*").append("omitInFTDC", true).obj();

static const BSONObj replSetGetStatusObj =
    BSONObjBuilder{}.append("replSetGetStatus", 1).append("initialSync", 0).obj();

repl::ReplicationCoordinator* getGlobalRC() {
    return repl::ReplicationCoordinator::get(getGlobalServiceContext());
}

bool isRepl(const repl::ReplicationCoordinator& rc) {
    return rc.getSettings().isReplSet();
}

bool isArbiter(const repl::ReplicationCoordinator& rc) {
    return isRepl(rc) && rc.getMemberState().arbiter();
}

bool isDataStoringNode() {
    auto rc = getGlobalRC();
    return !(rc && isArbiter(*rc));
}

std::unique_ptr<FTDCCollectorInterface> makeFilteredCollector(
    std::function<bool()> pred, std::unique_ptr<FTDCCollectorInterface> collector) {
    return std::make_unique<FilteredFTDCCollector>(std::move(pred), std::move(collector));
}

void registerShardCollectors(FTDCController* controller) {
    const auto role = ClusterRole::ShardServer;
    registerServerCollectorsForRole(controller, role);

    if (auto rc = getGlobalRC(); rc && isRepl(*rc)) {
        // CmdReplSetGetStatus
        controller->addPeriodicCollector(
            std::make_unique<FTDCSimpleInternalCommandCollector>(
                "replSetGetStatus", "replSetGetStatus", DatabaseName::kEmpty, replSetGetStatusObj),
            role);


        // CollectionStats
        const struct {
            StringData stat;
            StringData coll;
            const DatabaseName& db;
        } specs[]{
            {"local.oplog.rs.stats"_sd, "oplog.rs"_sd, DatabaseName::kLocal},
            {"config.transactions.stats"_sd, "transactions"_sd, DatabaseName::kConfig},
            {"config.image_collection.stats"_sd, "image_collection"_sd, DatabaseName::kConfig},
        };

        for (const auto& spec : specs) {
            controller->addPeriodicCollector(
                makeFilteredCollector(isDataStoringNode,
                                      std::make_unique<FTDCSimpleInternalCommandCollector>(
                                          "aggregate",
                                          spec.stat,
                                          spec.db,
                                          BSONObjBuilder{}
                                              .append("aggregate", spec.coll)
                                              .append("cursor", BSONObj{})
                                              .append("pipeline", pipelineObj)
                                              .obj())),
                role);
        }
    }

    controller->addPeriodicMetadataCollector(
        std::make_unique<FTDCSimpleInternalCommandCollector>(
            "getParameter", "getParameter", DatabaseName::kEmpty, getParameterQueryObj),
        role);

    controller->addPeriodicMetadataCollector(
        std::make_unique<FTDCSimpleInternalCommandCollector>("getClusterParameter",
                                                             "getClusterParameter",
                                                             DatabaseName::kEmpty,
                                                             getClusterParameterQueryObj),
        role);

    controller->addPeriodicCollector(
        makeFilteredCollector(isDataStoringNode, std::make_unique<FTDCCollectionStatsCollector>()),
        role);
}

}  // namespace

void startMongoDFTDC(ServiceContext* serviceContext) {
    auto dir = getFTDCDirectoryPathParameter();

    if (dir.empty()) {
        dir = storageGlobalParams.dbpath;
        dir /= kFTDCDefaultDirectory.toString();
    }

    std::vector<RegisterCollectorsFunction> registerFns{
        registerShardCollectors,
    };

    // (Ignore FCV check): this feature flag is not FCV-gated.
    const bool multiServiceFTDCSchema =
        feature_flags::gMultiServiceLogAndFTDCFormat.isEnabledAndIgnoreFCVUnsafe();

    const UseMultiServiceSchema multiversionSchema{
        serviceContext->getService(ClusterRole::RouterServer) && multiServiceFTDCSchema};

    if (multiversionSchema) {
        registerFns.emplace_back(registerRouterCollectors);
    }

    startFTDC(
        serviceContext, dir, FTDCStartMode::kStart, std::move(registerFns), multiversionSchema);
}

void stopMongoDFTDC() {
    stopFTDC();
}

}  // namespace mongo
