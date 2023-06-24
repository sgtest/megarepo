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

#include <boost/optional.hpp>
#include <utility>

#include <absl/container/node_hash_map.h>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/oid.h"
#include "mongo/bson/timestamp.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/s/collection_metadata.h"
#include "mongo/db/s/collection_sharding_runtime.h"
#include "mongo/db/s/sharding_state.h"
#include "mongo/db/shard_id.h"
#include "mongo/s/catalog/type_chunk.h"
#include "mongo/s/chunk_version.h"
#include "mongo/s/index_version.h"
#include "mongo/s/shard_version.h"
#include "mongo/s/shard_version_factory.h"
#include "mongo/s/sharding_index_catalog_cache.h"
#include "mongo/s/stale_exception.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/str.h"

namespace mongo {

namespace {

// This shard version is used as the received version in StaleConfigInfo since we do not have
// information about the received version of the operation.
ShardVersion ShardVersionPlacementIgnoredNoIndexes() {
    return ShardVersionFactory::make(ChunkVersion::IGNORED(),
                                     boost::optional<CollectionIndexes>(boost::none));
}

}  // namespace

CollectionPlacementAndIndexInfo checkCollectionIdentity(
    OperationContext* opCtx,
    const NamespaceString& nss,
    const OID& expectedEpoch,
    const boost::optional<Timestamp>& expectedTimestamp) {
    AutoGetCollection collection(opCtx, nss, MODE_IS);

    const auto shardId = ShardingState::get(opCtx)->shardId();
    const auto scopedCsr =
        CollectionShardingRuntime::assertCollectionLockedAndAcquireShared(opCtx, nss);
    auto optMetadata = scopedCsr->getCurrentMetadataIfKnown();
    auto optShardingIndexCatalogInfo = scopedCsr->getIndexes(opCtx);

    uassert(StaleConfigInfo(nss,
                            ShardVersionPlacementIgnoredNoIndexes() /* receivedVersion */,
                            boost::none /* wantedVersion */,
                            shardId),
            str::stream() << "Collection " << nss.toStringForErrorMsg() << " needs to be recovered",
            optMetadata);

    auto metadata = *optMetadata;

    uassert(StaleConfigInfo(nss,
                            ShardVersionPlacementIgnoredNoIndexes() /* receivedVersion */,
                            ShardVersion::UNSHARDED() /* wantedVersion */,
                            shardId),
            str::stream() << "Collection " << nss.toStringForErrorMsg() << " is not sharded",
            metadata.isSharded());

    uassert(ErrorCodes::NamespaceNotFound,
            "The collection was not found locally even though it is marked as sharded.",
            collection);

    const auto placementVersion = metadata.getShardPlacementVersion();
    const auto shardVersion =
        ShardVersionFactory::make(metadata, scopedCsr->getCollectionIndexes(opCtx));

    uassert(StaleConfigInfo(nss,
                            ShardVersionPlacementIgnoredNoIndexes() /* receivedVersion */,
                            shardVersion /* wantedVersion */,
                            shardId),
            str::stream() << "Collection " << nss.toStringForErrorMsg()
                          << " has changed since operation was sent (sent epoch: " << expectedEpoch
                          << ", current epoch: " << placementVersion.epoch() << ")",
            expectedEpoch == placementVersion.epoch() &&
                (!expectedTimestamp || expectedTimestamp == placementVersion.getTimestamp()));

    uassert(StaleConfigInfo(nss,
                            ShardVersionPlacementIgnoredNoIndexes() /* receivedVersion */,
                            shardVersion /* wantedVersion */,
                            shardId),
            str::stream() << "Shard does not contain any chunks for collection.",
            placementVersion.majorVersion() > 0);

    return std::make_pair(metadata, optShardingIndexCatalogInfo);
}

void checkShardKeyPattern(OperationContext* opCtx,
                          const NamespaceString& nss,
                          const CollectionMetadata& metadata,
                          const boost::optional<ShardingIndexesCatalogCache>& shardingIndexesInfo,
                          const ChunkRange& chunkRange) {
    const auto shardId = ShardingState::get(opCtx)->shardId();
    const auto& keyPattern = metadata.getKeyPattern();
    const auto shardVersion = ShardVersionFactory::make(
        metadata,
        shardingIndexesInfo ? boost::make_optional(shardingIndexesInfo->getCollectionIndexes())
                            : boost::none);

    uassert(StaleConfigInfo(nss,
                            ShardVersionPlacementIgnoredNoIndexes() /* receivedVersion */,
                            shardVersion /* wantedVersion */,
                            shardId),
            str::stream() << "The range " << chunkRange.toString()
                          << " is not valid for collection " << nss.toStringForErrorMsg()
                          << " with key pattern " << keyPattern.toString(),
            metadata.isValidKey(chunkRange.getMin()) && metadata.isValidKey(chunkRange.getMax()));
}

void checkChunkMatchesRange(OperationContext* opCtx,
                            const NamespaceString& nss,
                            const CollectionMetadata& metadata,
                            const boost::optional<ShardingIndexesCatalogCache>& shardingIndexesInfo,
                            const ChunkRange& chunkRange) {
    const auto shardId = ShardingState::get(opCtx)->shardId();
    const auto shardVersion = ShardVersionFactory::make(
        metadata,
        shardingIndexesInfo ? boost::make_optional(shardingIndexesInfo->getCollectionIndexes())
                            : boost::none);

    ChunkType existingChunk;
    uassert(StaleConfigInfo(nss,
                            ShardVersionPlacementIgnoredNoIndexes() /* receivedVersion */,
                            shardVersion /* wantedVersion */,
                            shardId),
            str::stream() << "Range with bounds " << chunkRange.toString()
                          << " is not owned by this shard.",
            metadata.getNextChunk(chunkRange.getMin(), &existingChunk) &&
                existingChunk.getMin().woCompare(chunkRange.getMin()) == 0);

    uassert(StaleConfigInfo(nss,
                            ShardVersionPlacementIgnoredNoIndexes() /* receivedVersion */,
                            shardVersion /* wantedVersion */,
                            shardId),
            str::stream() << "Chunk bounds " << chunkRange.toString() << " do not exist.",
            existingChunk.getRange() == chunkRange);
}

void checkRangeWithinChunk(OperationContext* opCtx,
                           const NamespaceString& nss,
                           const CollectionMetadata& metadata,
                           const boost::optional<ShardingIndexesCatalogCache>& shardingIndexesInfo,
                           const ChunkRange& chunkRange) {
    const auto shardId = ShardingState::get(opCtx)->shardId();
    const auto shardVersion = ShardVersionFactory::make(
        metadata,
        shardingIndexesInfo ? boost::make_optional(shardingIndexesInfo->getCollectionIndexes())
                            : boost::none);

    ChunkType existingChunk;
    uassert(StaleConfigInfo(nss,
                            ShardVersionPlacementIgnoredNoIndexes() /* receivedVersion */,
                            shardVersion /* wantedVersion */,
                            shardId),
            str::stream() << "Range with bounds " << chunkRange.toString()
                          << " is not contained within a chunk owned by this shard.",
            metadata.getNextChunk(chunkRange.getMin(), &existingChunk) &&
                existingChunk.getRange().covers(chunkRange));
}

void checkRangeOwnership(OperationContext* opCtx,
                         const NamespaceString& nss,
                         const CollectionMetadata& metadata,
                         const boost::optional<ShardingIndexesCatalogCache>& shardingIndexesInfo,
                         const ChunkRange& chunkRange) {
    const auto shardId = ShardingState::get(opCtx)->shardId();
    const auto shardVersion = ShardVersionFactory::make(
        metadata,
        shardingIndexesInfo ? boost::make_optional(shardingIndexesInfo->getCollectionIndexes())
                            : boost::none);

    ChunkType existingChunk;
    BSONObj minKey = chunkRange.getMin();
    do {
        uassert(StaleConfigInfo(nss,
                                ShardVersionPlacementIgnoredNoIndexes() /* receivedVersion */,
                                shardVersion /* wantedVersion */,
                                shardId),
                str::stream() << "Range with bounds " << chunkRange.toString()
                              << " is not owned by this shard.",
                metadata.getNextChunk(minKey, &existingChunk) &&
                    existingChunk.getMin().woCompare(minKey) == 0);
        minKey = existingChunk.getMax();
    } while (existingChunk.getMax().woCompare(chunkRange.getMax()) < 0);
    uassert(
        StaleConfigInfo(nss,
                        ShardVersionPlacementIgnoredNoIndexes() /* receivedVersion */,
                        shardVersion /* wantedVersion */,
                        shardId),
        str::stream() << "Shard does not contain a sequence of chunks that exactly fills the range "
                      << chunkRange.toString(),
        existingChunk.getMax().woCompare(chunkRange.getMax()) == 0);
}

}  // namespace mongo
