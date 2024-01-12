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


#include "mongo/s/client_transport_observer_mongos.h"

#include <boost/optional.hpp>

#include "mongo/db/cursor_id.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/request_execution_context.h"
#include "mongo/db/session/session.h"
#include "mongo/db/session/session_catalog.h"
#include "mongo/s/grid.h"
#include "mongo/s/load_balancer_support.h"
#include "mongo/s/query/cluster_cursor_manager.h"
#include "mongo/s/transaction_router.h"

namespace mongo {

void ClientTransportObserverMongos::onClientConnect(Client* client) {
    if (load_balancer_support::isFromLoadBalancer(client)) {
        _loadBalancedConnections.increment();
    }
}

void ClientTransportObserverMongos::onClientDisconnect(Client* client) {
    if (!load_balancer_support::isFromLoadBalancer(client)) {
        return;
    }

    _loadBalancedConnections.decrement();

    auto killerOperationContext = client->makeOperationContext();

    // Kill any cursors opened by the given Client.
    auto ccm = Grid::get(client->getServiceContext())->getCursorManager();
    ccm->killCursorsSatisfying(killerOperationContext.get(),
                               [&](CursorId, const ClusterCursorManager::CursorEntry& entry) {
                                   return entry.originatingClientUuid() == client->getUUID();
                               });

    // Kill any in-progress transactions over this Client connection.
    auto lsid = load_balancer_support::getMruSession(client);

    auto killToken = [&]() -> boost::optional<SessionCatalog::KillToken> {
        try {
            return SessionCatalog::get(killerOperationContext.get())->killSession(lsid);
        } catch (const ExceptionFor<ErrorCodes::NoSuchSession>&) {
            return boost::none;
        }
    }();
    if (!killToken) {
        // There was no entry in the SessionCatalog for the session most recently used by the
        // disconnecting client, so we have no transaction state to clean up.
        return;
    }
    OperationContextSession sessionCtx(killerOperationContext.get(), std::move(*killToken));
    invariant(lsid == OperationContextSession::get(killerOperationContext.get())->getSessionId());

    auto txnRouter = TransactionRouter::get(killerOperationContext.get());
    if (txnRouter && txnRouter.isInitialized() && !txnRouter.isTrackingOver()) {
        txnRouter.implicitlyAbortTransaction(
            killerOperationContext.get(),
            {ErrorCodes::Interrupted,
             "aborting in-progress transaction because load-balanced client disconnected"});
    }
}

void ClientTransportObserverMongos::appendTransportServerStats(BSONObjBuilder* bob) {
    if (load_balancer_support::isEnabled()) {
        bob->append("loadBalanced", _loadBalancedConnections);
    }
}

}  // namespace mongo
