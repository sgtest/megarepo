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

#include <memory>
#include <string>

#include <boost/preprocessor/control/iif.hpp>

#include "mongo/base/shim.h"
#include "mongo/db/auth/authz_session_external_state.h"
#include "mongo/db/auth/authz_session_external_state_d.h"
#include "mongo/db/client.h"
#include "mongo/db/concurrency/locker.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/repl/member_state.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/service_context.h"
#include "mongo/util/assert_util.h"

namespace mongo {

AuthzSessionExternalStateMongod::AuthzSessionExternalStateMongod(AuthorizationManager* authzManager)
    : AuthzSessionExternalStateServerCommon(authzManager) {}
AuthzSessionExternalStateMongod::~AuthzSessionExternalStateMongod() {}

void AuthzSessionExternalStateMongod::startRequest(OperationContext* opCtx) {
    // No locks should be held as this happens before any database accesses occur
    dassert(!opCtx->lockState()->isLocked());

    _checkShouldAllowLocalhost(opCtx);
}

bool AuthzSessionExternalStateMongod::shouldIgnoreAuthChecks() const {
    // TODO(spencer): get "isInDirectClient" from OperationContext
    return cc().isInDirectClient() ||
        AuthzSessionExternalStateServerCommon::shouldIgnoreAuthChecks();
}

bool AuthzSessionExternalStateMongod::serverIsArbiter() const {
    // Arbiters have access to extra privileges under localhost. See SERVER-5479.
    return (
        repl::ReplicationCoordinator::get(getGlobalServiceContext())->getSettings().isReplSet() &&
        repl::ReplicationCoordinator::get(getGlobalServiceContext())->getMemberState().arbiter());
}

namespace {

std::unique_ptr<AuthzSessionExternalState> authzSessionExternalStateImpl(
    AuthorizationManager* authzManager) {
    return std::make_unique<AuthzSessionExternalStateMongod>(authzManager);
}

auto authzSessionExternalStateRegistration = MONGO_WEAK_FUNCTION_REGISTRATION(
    AuthzSessionExternalState::create, authzSessionExternalStateImpl);

}  // namespace


}  // namespace mongo
