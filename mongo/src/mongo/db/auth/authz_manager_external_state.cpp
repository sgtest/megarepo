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

#include <string>
#include <utility>

#include <absl/container/node_hash_set.h>
#include <boost/preprocessor/control/iif.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/shim.h"
#include "mongo/bson/util/builder.h"
#include "mongo/bson/util/builder_fwd.h"
#include "mongo/config.h"  // IWYU pragma: keep
#include "mongo/db/auth/auth_name.h"
#include "mongo/db/auth/authz_manager_external_state.h"
#include "mongo/util/assert_util_core.h"

namespace mongo {
namespace {
using UniqueExternalState = AuthzManagerExternalState::UniqueExternalState;
using ShimFn = AuthzManagerExternalState::ShimFn;
std::vector<ShimFn> shimFunctions;
}  // namespace

UniqueExternalState AuthzManagerExternalState::create() {
    static auto w = MONGO_WEAK_FUNCTION_DEFINITION(AuthzManagerExternalState::create);

    UniqueExternalState externalState = w();
    for (const auto& shim : shimFunctions) {
        externalState = shim(std::move(externalState));
    }
    return externalState;
}

void AuthzManagerExternalState::prependShim(ShimFn&& shim) {
    shimFunctions.insert(shimFunctions.begin(), std::move(shim));
}

void AuthzManagerExternalState::appendShim(ShimFn&& shim) {
    shimFunctions.push_back(std::move(shim));
}

AuthzManagerExternalState::AuthzManagerExternalState() = default;
AuthzManagerExternalState::~AuthzManagerExternalState() = default;

Status AuthzManagerExternalState::makeRoleNotFoundStatus(
    const stdx::unordered_set<RoleName>& unknownRoles) {
    dassert(unknownRoles.size());

    char delim = ':';
    StringBuilder sb;
    sb << "Could not find role";
    if (unknownRoles.size() > 1) {
        sb << 's';
    }
    for (const auto& unknownRole : unknownRoles) {
        sb << delim << ' ' << unknownRole;
        delim = ',';
    }
    return {ErrorCodes::RoleNotFound, sb.str()};
}

}  // namespace mongo
