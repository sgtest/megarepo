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

#pragma once

#include <boost/optional/optional.hpp>
#include <memory>
#include <string>

#include "mongo/base/status.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/query/partitioned_cache.h"
#include "mongo/db/query/util/memory_util.h"
#include "mongo/db/service_context.h"
#include "mongo/db/tenant_id.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/decorable.h"


namespace mongo::query_stats_util {

Status onQueryStatsStoreSizeUpdate(const std::string& str);


Status validateQueryStatsStoreSize(const std::string& str, const boost::optional<TenantId>&);

Status onQueryStatsSamplingRateUpdate(int samplingRate);

/**
 *  An interface used to modify the queryStats store when query setParameters are modified. This is
 *  done via an interface decorating the 'ServiceContext' in order to avoid a link-time dependency
 *  of the query knobs library on the queryStats code.
 */
class OnParamChangeUpdater {
public:
    virtual ~OnParamChangeUpdater() = default;

    /**
     * Resizes the queryStats store decorating 'serviceCtx' to the new size given by 'memSize'. If
     * the new size is smaller than the old, cache entries are evicted in order to ensure the
     * cache fits within the new size bound.
     */
    virtual void updateCacheSize(ServiceContext* serviceCtx, memory_util::MemorySize memSize) = 0;

    /**
     * Updates the sampling rate for the queryStats rate limiter.
     */
    virtual void updateSamplingRate(ServiceContext* serviceCtx, int samplingRate) = 0;
};

/**
 * A stub implementation that does not allow changing any parameters - to be used if the queryStats
 * store is disabled and cannot be re-enabled without restarting, as with a feature flag.
 */
class NoChangesAllowedTelemetryParamUpdater : public OnParamChangeUpdater {
public:
    void updateCacheSize(ServiceContext* serviceCtx, memory_util::MemorySize memSize) final {
        uasserted(7373500,
                  "Cannot configure queryStats store - it is currently disabled and a restart is "
                  "required to activate.");
    }

    void updateSamplingRate(ServiceContext* serviceCtx, int samplingRate) {
        uasserted(7506200,
                  "Cannot configure queryStats store - it is currently disabled and a restart is "
                  "required to activate.");
    }
};

/**
 * Decorated accessor to the 'OnParamChangeUpdater' stored in 'ServiceContext'.
 */
extern const Decorable<ServiceContext>::Decoration<std::unique_ptr<OnParamChangeUpdater>>
    queryStatsStoreOnParamChangeUpdater;
}  // namespace mongo::query_stats_util
