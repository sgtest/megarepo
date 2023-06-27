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


#include <memory>
#include <string>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>

#include "mongo/bson/bsonobj.h"
#include "mongo/db/basic_types.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/commands.h"
#include "mongo/db/db_raii.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/s/shard_key_util.h"
#include "mongo/db/s/sharding_state.h"
#include "mongo/db/service_context.h"
#include "mongo/rpc/op_msg.h"
#include "mongo/s/catalog_cache.h"
#include "mongo/s/chunk_manager.h"
#include "mongo/s/grid.h"
#include "mongo/s/request_types/sharded_ddl_commands_gen.h"
#include "mongo/s/shard_key_pattern.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/uuid.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kSharding

namespace mongo {
namespace {

class ShardsvrValidateShardKeyCandidateCommand final
    : public TypedCommand<ShardsvrValidateShardKeyCandidateCommand> {
public:
    AllowedOnSecondary secondaryAllowed(ServiceContext*) const override {
        return Command::AllowedOnSecondary::kNever;
    }

    bool skipApiVersionCheck() const override {
        // Internal command (server to server).
        return true;
    }

    std::string help() const override {
        return "Internal command, which is exported by the primary sharding server. Do not call "
               "directly. Validates a collection shard key candidate.";
    }

    using Request = ShardsvrValidateShardKeyCandidate;

    class Invocation final : public InvocationBase {
    public:
        using InvocationBase::InvocationBase;

        void typedRun(OperationContext* opCtx) {

            const ShardKeyPattern keyPattern(request().getKey());
            uassertStatusOK(ShardingState::get(opCtx)->canAcceptShardedCommands());

            const auto [cm, _] = uassertStatusOK(
                Grid::get(opCtx)->catalogCache()->getCollectionRoutingInfo(opCtx, ns()));
            {
                AutoGetCollectionForReadCommandMaybeLockFree coll{
                    opCtx,
                    ns(),
                    AutoGetCollection::Options{}.viewMode(
                        auto_get_collection::ViewMode::kViewsForbidden)};

                shardkeyutil::validateShardKeyIndexExistsOrCreateIfPossible(
                    opCtx,
                    ns(),
                    keyPattern,
                    boost::none,
                    cm.isUnique(),
                    request().getEnforceUniquenessCheck().value_or(true),
                    shardkeyutil::ValidationBehaviorsLocalRefineShardKey(opCtx,
                                                                         coll.getCollection()));
            }
            shardkeyutil::validateShardKeyIsNotEncrypted(opCtx, ns(), keyPattern);
        }

        bool supportsWriteConcern() const override {
            return false;
        }

        void doCheckAuthorization(OperationContext*) const override {}

        /**
         * The ns() for when Request's IDL specifies "namespace: concatenate_with_db".
         */
        NamespaceString ns() const override {
            return request().getCommandParameter();
        }
    };

} shardsvrValidateShardKeyCandidateCommand;

}  // namespace
}  // namespace mongo
