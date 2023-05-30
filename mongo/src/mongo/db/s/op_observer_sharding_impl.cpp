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

#include "mongo/db/s/op_observer_sharding_impl.h"

#include "mongo/db/repl/oplog_entry.h"
#include "mongo/db/s/collection_sharding_runtime.h"
#include "mongo/db/s/database_sharding_state.h"
#include "mongo/db/s/migration_chunk_cloner_source.h"
#include "mongo/db/s/migration_chunk_cloner_source_op_observer.h"
#include "mongo/db/s/migration_source_manager.h"
#include "mongo/db/s/sharding_write_router.h"
#include "mongo/logv2/log.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kSharding

namespace mongo {

OpObserverShardingImpl::OpObserverShardingImpl(std::unique_ptr<OplogWriter> oplogWriter)
    : OpObserverImpl(std::move(oplogWriter)) {}

void OpObserverShardingImpl::shardObserveAboutToDelete(OperationContext* opCtx,
                                                       NamespaceString const& nss,
                                                       BSONObj const& docToDelete) {}

void OpObserverShardingImpl::shardObserveInsertsOp(
    OperationContext* opCtx,
    const NamespaceString& nss,
    std::vector<InsertStatement>::const_iterator first,
    std::vector<InsertStatement>::const_iterator last,
    const std::vector<repl::OpTime>& opTimeList,
    const ShardingWriteRouter& shardingWriteRouter,
    const bool fromMigrate,
    const bool inMultiDocumentTransaction) {
    if (nss == NamespaceString::kSessionTransactionsTableNamespace || fromMigrate)
        return;

    auto* const css = shardingWriteRouter.getCss();
    css->checkShardVersionOrThrow(opCtx);
    DatabaseShardingState::assertMatchingDbVersion(opCtx, nss.dbName());

    auto* const csr = checked_cast<CollectionShardingRuntime*>(css);
    auto metadata = csr->getCurrentMetadataIfKnown();
    if (!metadata || !metadata->isSharded()) {
        MigrationChunkClonerSourceOpObserver::assertNoMovePrimaryInProgress(opCtx, nss);
        return;
    }

    int index = 0;
    for (auto it = first; it != last; it++, index++) {
        auto opTime = opTimeList.empty() ? repl::OpTime() : opTimeList[index];

        if (inMultiDocumentTransaction) {
            const auto atClusterTime = repl::ReadConcernArgs::get(opCtx).getArgsAtClusterTime();

            if (atClusterTime) {
                const auto shardKey =
                    metadata->getShardKeyPattern().extractShardKeyFromDocThrows(it->doc);
                MigrationChunkClonerSourceOpObserver::assertIntersectingChunkHasNotMoved(
                    opCtx, *metadata, shardKey, *atClusterTime);
            }

            return;
        }

        auto cloner = MigrationSourceManager::getCurrentCloner(*csr);
        if (cloner) {
            cloner->onInsertOp(opCtx, it->doc, opTime);
        }
    }
}

void OpObserverShardingImpl::shardObserveUpdateOp(OperationContext* opCtx,
                                                  const NamespaceString& nss,
                                                  boost::optional<BSONObj> preImageDoc,
                                                  const BSONObj& postImageDoc,
                                                  const repl::OpTime& opTime,
                                                  const ShardingWriteRouter& shardingWriteRouter,
                                                  const bool inMultiDocumentTransaction) {
    auto* const css = shardingWriteRouter.getCss();
    css->checkShardVersionOrThrow(opCtx);
    DatabaseShardingState::assertMatchingDbVersion(opCtx, nss.dbName());

    auto* const csr = checked_cast<CollectionShardingRuntime*>(css);
    auto metadata = csr->getCurrentMetadataIfKnown();
    if (!metadata || !metadata->isSharded()) {
        MigrationChunkClonerSourceOpObserver::assertNoMovePrimaryInProgress(opCtx, nss);
        return;
    }

    if (inMultiDocumentTransaction) {
        const auto atClusterTime = repl::ReadConcernArgs::get(opCtx).getArgsAtClusterTime();

        if (atClusterTime) {
            const auto shardKey =
                metadata->getShardKeyPattern().extractShardKeyFromDocThrows(postImageDoc);
            MigrationChunkClonerSourceOpObserver::assertIntersectingChunkHasNotMoved(
                opCtx, *metadata, shardKey, *atClusterTime);
        }

        return;
    }

    auto cloner = MigrationSourceManager::getCurrentCloner(*csr);
    if (cloner) {
        cloner->onUpdateOp(opCtx, preImageDoc, postImageDoc, opTime);
    }
}

void OpObserverShardingImpl::shardObserveDeleteOp(OperationContext* opCtx,
                                                  const NamespaceString& nss,
                                                  const BSONObj& documentKey,
                                                  const repl::OpTime& opTime,
                                                  const ShardingWriteRouter& shardingWriteRouter,
                                                  const bool inMultiDocumentTransaction) {}

}  // namespace mongo
