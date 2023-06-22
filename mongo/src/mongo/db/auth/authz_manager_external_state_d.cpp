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
#include <utility>

#include "mongo/base/error_codes.h"
#include "mongo/base/shim.h"
#include "mongo/base/status.h"
#include "mongo/db/auth/authz_manager_external_state_d.h"
#include "mongo/db/auth/authz_session_external_state.h"
#include "mongo/db/auth/authz_session_external_state_d.h"
#include "mongo/db/db_raii.h"
#include "mongo/db/dbdirectclient.h"
#include "mongo/db/dbhelpers.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/query/find_command.h"
#include "mongo/db/record_id.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/str.h"

namespace mongo {

AuthzManagerExternalStateMongod::AuthzManagerExternalStateMongod() = default;
AuthzManagerExternalStateMongod::~AuthzManagerExternalStateMongod() = default;

std::unique_ptr<AuthzSessionExternalState>
AuthzManagerExternalStateMongod::makeAuthzSessionExternalState(AuthorizationManager* authzManager) {
    return std::make_unique<AuthzSessionExternalStateMongod>(authzManager);
}
Status AuthzManagerExternalStateMongod::query(
    OperationContext* opCtx,
    const NamespaceString& collectionName,
    const BSONObj& filter,
    const BSONObj& projection,
    const std::function<void(const BSONObj&)>& resultProcessor) {
    try {
        DBDirectClient client(opCtx);
        FindCommandRequest findRequest{collectionName};
        findRequest.setFilter(filter);
        findRequest.setProjection(projection);
        client.find(std::move(findRequest), resultProcessor);
        return Status::OK();
    } catch (const DBException& e) {
        return e.toStatus();
    }
}

Status AuthzManagerExternalStateMongod::findOne(OperationContext* opCtx,
                                                const NamespaceString& nss,
                                                const BSONObj& query,
                                                BSONObj* result) {
    AutoGetCollectionForReadCommandMaybeLockFree ctx(opCtx, nss);

    BSONObj found;
    if (Helpers::findOne(opCtx, ctx.getCollection(), query, found)) {
        *result = found.getOwned();
        return Status::OK();
    }
    return {ErrorCodes::NoMatchingDocument,
            str::stream() << "No document in " << nss.toStringForErrorMsg() << " matches "
                          << query};
}

bool AuthzManagerExternalStateMongod::hasOne(OperationContext* opCtx,
                                             const NamespaceString& nss,
                                             const BSONObj& query) {
    AutoGetCollectionForReadCommandMaybeLockFree ctx(opCtx, nss);
    return !Helpers::findOne(opCtx, ctx.getCollection(), query).isNull();
}

namespace {

std::unique_ptr<AuthzManagerExternalState> authzManagerExternalStateCreateImpl() {
    return std::make_unique<AuthzManagerExternalStateMongod>();
}

auto authzManagerExternalStateCreateRegistration = MONGO_WEAK_FUNCTION_REGISTRATION(
    AuthzManagerExternalState::create, authzManagerExternalStateCreateImpl);

}  // namespace

}  // namespace mongo
