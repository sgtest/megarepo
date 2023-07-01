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

#include "mongo/s/catalog_cache_mock.h"

#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <utility>

#include <boost/move/utility_core.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/db/service_context.h"
#include "mongo/s/catalog_cache_loader_mock.h"
#include "mongo/s/sharding_index_catalog_cache.h"

namespace mongo {

const Status CatalogCacheMock::kChunkManagerInternalErrorStatus = {
    ErrorCodes::InternalError, "Mocked catalog cache received unexpected chunks manager"};


CatalogCacheMock::CatalogCacheMock(ServiceContext* serviceContext, CatalogCacheLoaderMock& loader)
    : CatalogCache(serviceContext, loader) {}

StatusWith<CollectionRoutingInfo> CatalogCacheMock::getCollectionRoutingInfo(
    OperationContext* opCtx, const NamespaceString& nss, bool allowLocks) {
    if (!_swChunkManagerReturnValue.isOK()) {
        return _swChunkManagerReturnValue.getStatus();
    }
    ChunkManager cm = _swChunkManagerReturnValue.getValue();
    return CollectionRoutingInfo(std::move(cm), boost::none);
}

void CatalogCacheMock::setChunkManagerReturnValue(StatusWith<ChunkManager> statusWithChunks) {
    _swChunkManagerReturnValue = statusWithChunks;
}
void CatalogCacheMock::clearChunkManagerReturnValue() {
    _swChunkManagerReturnValue = kChunkManagerInternalErrorStatus;
}

std::unique_ptr<CatalogCacheMock> CatalogCacheMock::make() {
    auto catalogCacheLoader = std::make_unique<CatalogCacheLoaderMock>();
    auto serviceContext = ServiceContext::make();
    return std::make_unique<CatalogCacheMock>(serviceContext.get(), *catalogCacheLoader);
}
}  // namespace mongo
