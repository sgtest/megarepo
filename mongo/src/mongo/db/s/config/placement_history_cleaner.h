/**
 *    Copyright (C) 2023-present MongoDB, Inc.
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

#include <cstddef>
#include <string>

#include "mongo/db/client.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/repl/replica_set_aware_service.h"
#include "mongo/db/service_context.h"
#include "mongo/platform/mutex.h"
#include "mongo/util/periodic_runner.h"

namespace mongo {

/**
 * Background service that launches a periodic job to assess whether there are documents that can be
 * removed from config.placementHistory.
 */
class PlacementHistoryCleaner : public ReplicaSetAwareServiceConfigSvr<PlacementHistoryCleaner> {
public:
    PlacementHistoryCleaner() = default;

    /**
     * Obtains the service-wide instance.
     */
    static PlacementHistoryCleaner* get(ServiceContext* serviceContext);
    static PlacementHistoryCleaner* get(OperationContext* opCtx);

    void pause();
    void resume(OperationContext* opCtx);

private:
    PlacementHistoryCleaner(const PlacementHistoryCleaner&) = delete;
    PlacementHistoryCleaner& operator=(const PlacementHistoryCleaner&) = delete;

    static void runOnce(Client* opCtx, size_t minPlacementHistoryEntries);

    void _start(OperationContext* opCtx, bool steppingUp);

    void _stop(bool steppingDown);

    /**
     * ReplicaSetAwareService entry points.
     */
    void onStartup(OperationContext* opCtx) final {}

    void onSetCurrentConfig(OperationContext* opCtx) final {}

    void onInitialDataAvailable(OperationContext* opCtx, bool isMajorityDataAvailable) final {}

    void onStepUpBegin(OperationContext* opCtx, long long term) final {}

    void onStepUpComplete(OperationContext* opCtx, long long term) final;

    void onStepDown() final;

    void onRollback() final {}

    void onShutdown() final {}

    void onBecomeArbiter() final {}

    inline std::string getServiceName() const override final {
        return "PlacementHistoryCleaner";
    }

    Mutex _mutex = MONGO_MAKE_LATCH("PlacementHistoryCleaner::_mutex");

    PeriodicJobAnchor _anchor;

    bool _runningAsPrimary = false;
};
}  // namespace mongo
