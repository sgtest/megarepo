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

#pragma once

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/bson/bsonobj.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/s/collection_sharding_state.h"
#include "mongo/db/s/scoped_collection_metadata.h"
#include "mongo/db/shard_id.h"
#include "mongo/s/catalog_cache.h"
#include "mongo/s/chunk_manager.h"
#include "mongo/s/shard_key_pattern.h"

namespace mongo {

class ShardingWriteRouter {
public:
    ShardingWriteRouter(OperationContext* opCtx, const NamespaceString& nss);

    CollectionShardingState* getCss() const {
        return _scopedCss ? &(**_scopedCss) : nullptr;
    }

    const boost::optional<ScopedCollectionDescription>& getCollDesc() const {
        return _collDesc;
    }

    boost::optional<ShardId> getReshardingDestinedRecipient(const BSONObj& fullDocument) const;

private:
    boost::optional<CollectionShardingState::ScopedCollectionShardingState> _scopedCss;
    boost::optional<ScopedCollectionDescription> _collDesc;

    boost::optional<ScopedCollectionFilter> _ownershipFilter;

    boost::optional<ShardKeyPattern> _reshardingKeyPattern;
    boost::optional<ChunkManager> _reshardingChunkMgr;
};

}  // namespace mongo
