/**
 *    Copyright (C) 2021-present MongoDB, Inc.
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
#include <memory>
#include <utility>
#include <vector>

#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/client/read_preference.h"
#include "mongo/db/commands.h"
#include "mongo/db/database_name.h"
#include "mongo/db/namespace_string.h"
#include "mongo/executor/remote_command_response.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/rpc/get_status_from_command_result.h"
#include "mongo/s/async_requests_sender.h"
#include "mongo/s/catalog/type_database_gen.h"
#include "mongo/s/client/shard.h"
#include "mongo/s/client/shard_registry.h"
#include "mongo/s/cluster_commands_helpers.h"
#include "mongo/s/cluster_ddl.h"
#include "mongo/s/database_version.h"
#include "mongo/s/grid.h"
#include "mongo/s/shard_version.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/read_through_cache.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kSharding


namespace mongo {
namespace cluster {
namespace {

std::vector<AsyncRequestsSender::Request> buildUnshardedRequestsForAllShards(
    OperationContext* opCtx, std::vector<ShardId> shardIds, const BSONObj& cmdObj) {
    auto cmdToSend = cmdObj;
    appendShardVersion(cmdToSend, ShardVersion::UNSHARDED());

    std::vector<AsyncRequestsSender::Request> requests;
    for (auto&& shardId : shardIds)
        requests.emplace_back(std::move(shardId), cmdToSend);

    return requests;
}

AsyncRequestsSender::Response executeCommandAgainstDatabasePrimaryOrFirstShard(
    OperationContext* opCtx,
    const DatabaseName& dbName,
    const CachedDatabaseInfo& dbInfo,
    const BSONObj& cmdObj,
    const ReadPreferenceSetting& readPref,
    Shard::RetryPolicy retryPolicy) {
    ShardId shardId;
    if (dbName == DatabaseName::kConfig) {
        auto shardIds = Grid::get(opCtx)->shardRegistry()->getAllShardIds(opCtx);
        uassert(ErrorCodes::IllegalOperation, "there are no shards to target", !shardIds.empty());
        std::sort(shardIds.begin(), shardIds.end());
        shardId = shardIds[0];
    } else {
        shardId = dbInfo->getPrimary();
    }

    auto responses =
        gatherResponses(opCtx,
                        dbName,
                        readPref,
                        retryPolicy,
                        buildUnshardedRequestsForAllShards(
                            opCtx, {shardId}, appendDbVersionIfPresent(cmdObj, dbInfo)));
    return std::move(responses.front());
}

}  // namespace

CachedDatabaseInfo createDatabase(OperationContext* opCtx,
                                  const DatabaseName& dbName,
                                  const boost::optional<ShardId>& suggestedPrimaryId) {
    auto catalogCache = Grid::get(opCtx)->catalogCache();

    auto dbStatus = catalogCache->getDatabase(opCtx, dbName);

    if (dbStatus == ErrorCodes::NamespaceNotFound) {
        ConfigsvrCreateDatabase request(DatabaseNameUtil::serialize(dbName));
        request.setDbName(DatabaseName::kAdmin);
        if (suggestedPrimaryId)
            request.setPrimaryShardId(*suggestedPrimaryId);

        auto configShard = Grid::get(opCtx)->shardRegistry()->getConfigShard();
        auto response = uassertStatusOK(configShard->runCommandWithFixedRetryAttempts(
            opCtx,
            ReadPreferenceSetting(ReadPreference::PrimaryOnly),
            DatabaseName::kAdmin,
            CommandHelpers::appendMajorityWriteConcern(request.toBSON({})),
            Shard::RetryPolicy::kIdempotent));
        uassertStatusOK(response.writeConcernStatus);
        uassertStatusOKWithContext(response.commandStatus,
                                   str::stream() << "Database " << dbName.toStringForErrorMsg()
                                                 << " could not be created");

        auto createDbResponse = ConfigsvrCreateDatabaseResponse::parse(
            IDLParserContext("configsvrCreateDatabaseResponse"), response.response);
        catalogCache->onStaleDatabaseVersion(dbName, createDbResponse.getDatabaseVersion());

        dbStatus = catalogCache->getDatabase(opCtx, dbName);
    }

    return uassertStatusOK(std::move(dbStatus));
}

void createCollection(OperationContext* opCtx, const ShardsvrCreateCollection& request) {
    const auto& nss = request.getNamespace();
    const auto dbInfo = createDatabase(opCtx, nss.dbName());

    auto cmdResponse = executeCommandAgainstDatabasePrimaryOrFirstShard(
        opCtx,
        nss.dbName(),
        dbInfo,
        CommandHelpers::appendMajorityWriteConcern(request.toBSON({})),
        ReadPreferenceSetting(ReadPreference::PrimaryOnly),
        Shard::RetryPolicy::kIdempotent);

    const auto remoteResponse = uassertStatusOK(cmdResponse.swResponse);
    uassertStatusOK(getStatusFromCommandResult(remoteResponse.data));

    auto createCollResp =
        CreateCollectionResponse::parse(IDLParserContext("createCollection"), remoteResponse.data);

    auto catalogCache = Grid::get(opCtx)->catalogCache();
    catalogCache->invalidateShardOrEntireCollectionEntryForShardedCollection(
        nss, createCollResp.getCollectionVersion(), dbInfo->getPrimary());
}

}  // namespace cluster
}  // namespace mongo
