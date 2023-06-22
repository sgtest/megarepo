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

#include "mongo/db/auth/validated_tenancy_scope.h"

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <string>

#include <boost/optional/optional.hpp>

#include "mongo/base/data_range.h"
#include "mongo/base/error_codes.h"
#include "mongo/base/init.h"  // IWYU pragma: keep
#include "mongo/base/initializer.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/crypto/hash_block.h"
#include "mongo/crypto/sha256_block.h"
#include "mongo/db/auth/action_type.h"
#include "mongo/db/auth/authorization_session.h"
#include "mongo/db/auth/resource_pattern.h"
#include "mongo/db/auth/security_token_gen.h"
#include "mongo/db/client.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/multitenancy_gen.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/server_feature_flags_gen.h"
#include "mongo/db/server_options.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_component.h"
#include "mongo/logv2/log_detail.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/decorable.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kAccessControl

namespace mongo::auth {
namespace {
const auto validatedTenancyScopeDecoration =
    OperationContext::declareDecoration<boost::optional<ValidatedTenancyScope>>();
MONGO_INITIALIZER(SecurityTokenOptionValidate)(InitializerContext*) {
    if (gMultitenancySupport) {
        logv2::detail::setGetTenantIDCallback([]() -> std::string {
            auto* client = Client::getCurrent();
            if (!client) {
                return std::string();
            }

            if (auto* opCtx = client->getOperationContext()) {
                if (auto token = ValidatedTenancyScope::get(opCtx)) {
                    return token->tenantId().toString();
                }
            }

            return std::string();
        });
    }

    if (gFeatureFlagSecurityToken.isEnabledAndIgnoreFCVUnsafeAtStartup()) {
        LOGV2_WARNING(
            7539600,
            "featureFlagSecurityToken is enabled.  This flag MUST NOT be enabled in production");
    }
}
}  // namespace

ValidatedTenancyScope::ValidatedTenancyScope(BSONObj obj, InitTag tag) : _originalToken(obj) {
    const bool enabled = gMultitenancySupport &&
        gFeatureFlagSecurityToken.isEnabled(serverGlobalParams.featureCompatibility);

    uassert(ErrorCodes::InvalidOptions,
            "Multitenancy not enabled, refusing to accept securityToken",
            enabled || (tag == InitTag::kInitForShell));

    auto token = SecurityToken::parse(IDLParserContext{"Security Token"}, obj);
    auto authenticatedUser = token.getAuthenticatedUser();
    uassert(ErrorCodes::BadValue,
            "Security token authenticated user requires a valid Tenant ID",
            authenticatedUser.getTenant());

    // Use actual authenticatedUser object as passed to preserve hash input.
    auto authUserObj = obj[SecurityToken::kAuthenticatedUserFieldName].Obj();
    ConstDataRange authUserCDR(authUserObj.objdata(), authUserObj.objsize());

    // Placeholder algorithm.
    auto computed = SHA256Block::computeHash({authUserCDR});

    uassert(ErrorCodes::Unauthorized, "Token signature invalid", computed == token.getSig());

    _tenantOrUser = std::move(authenticatedUser);
}

ValidatedTenancyScope::ValidatedTenancyScope(Client* client, TenantId tenant)
    : _tenantOrUser(std::move(tenant)) {
    uassert(ErrorCodes::InvalidOptions,
            "Multitenancy not enabled, refusing to accept $tenant parameter",
            gMultitenancySupport);

    uassert(ErrorCodes::Unauthorized,
            "'$tenant' may only be specified with the useTenant action type",
            client &&
                AuthorizationSession::get(client)->isAuthorizedForActionsOnResource(
                    ResourcePattern::forClusterResource(), ActionType::useTenant));
}

boost::optional<ValidatedTenancyScope> ValidatedTenancyScope::create(Client* client,
                                                                     BSONObj body,
                                                                     BSONObj securityToken) {
    if (!gMultitenancySupport) {
        return boost::none;
    }

    auto dollarTenantElem = body["$tenant"_sd];
    const bool hasToken = securityToken.nFields() > 0;

    uassert(6545800,
            "Cannot pass $tenant id if also passing securityToken",
            dollarTenantElem.eoo() || !hasToken);
    uassert(ErrorCodes::OperationFailed,
            "Cannot process $tenant id when no client is available",
            dollarTenantElem.eoo() || client);

    // TODO SERVER-66822: Re-enable this uassert.
    // uassert(ErrorCodes::Unauthorized,
    //         "Multitenancy is enabled, $tenant id or securityToken is required.",
    //         dollarTenantElem || opMsg.securityToken.nFields() > 0);

    if (dollarTenantElem) {
        return ValidatedTenancyScope(client, TenantId::parseFromBSON(dollarTenantElem));
    } else if (hasToken) {
        return ValidatedTenancyScope(securityToken);
    } else {
        return boost::none;
    }
}

bool ValidatedTenancyScope::hasAuthenticatedUser() const {
    return stdx::holds_alternative<UserName>(_tenantOrUser);
}

const UserName& ValidatedTenancyScope::authenticatedUser() const {
    invariant(hasAuthenticatedUser());
    return stdx::get<UserName>(_tenantOrUser);
}

const boost::optional<ValidatedTenancyScope>& ValidatedTenancyScope::get(OperationContext* opCtx) {
    return validatedTenancyScopeDecoration(opCtx);
}

void ValidatedTenancyScope::set(OperationContext* opCtx,
                                boost::optional<ValidatedTenancyScope> token) {
    validatedTenancyScopeDecoration(opCtx) = std::move(token);
}

ValidatedTenancyScope::ValidatedTenancyScope(BSONObj obj, TokenForTestingTag) {
    auto authUserElem = obj[SecurityToken::kAuthenticatedUserFieldName];
    uassert(ErrorCodes::BadValue,
            "Invalid field(s) in token being signed",
            (authUserElem.type() == Object) && (obj.nFields() == 1));

    auto authUserObj = authUserElem.Obj();
    ConstDataRange authUserCDR(authUserObj.objdata(), authUserObj.objsize());

    // Placeholder algorithm.
    auto sig = SHA256Block::computeHash({authUserCDR});

    BSONObjBuilder signedToken(obj);
    signedToken.appendBinData(SecurityToken::kSigFieldName, sig.size(), BinDataGeneral, sig.data());
    _originalToken = signedToken.obj();
    _tenantOrUser = UserName::parseFromBSONObj(authUserObj);
}

}  // namespace mongo::auth
