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

#include <absl/container/node_hash_map.h>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional.hpp>
#include <memory>
#include <set>

#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/bson/oid.h"
#include "mongo/db/auth/action_type.h"
#include "mongo/db/auth/auth_name.h"
#include "mongo/db/auth/authorization_manager.h"
#include "mongo/db/auth/authorization_manager_impl.h"
#include "mongo/db/auth/authorization_session.h"
#include "mongo/db/auth/authorization_session_impl.h"
#include "mongo/db/auth/authz_manager_external_state_mock.h"
#include "mongo/db/auth/privilege.h"
#include "mongo/db/auth/resource_pattern.h"
#include "mongo/db/auth/role_name.h"
#include "mongo/db/auth/user.h"
#include "mongo/db/auth/validated_tenancy_scope_factory.h"
#include "mongo/db/client.h"
#include "mongo/db/service_context.h"
#include "mongo/db/service_context_test_fixture.h"
#include "mongo/idl/server_parameter_test_util.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/assert_util.h"

namespace mongo {

class AuthorizationSessionImplTestHelper {
public:
    /**
     * Synthesize a user with the useTenant privilege and add them to the authorization session.
     */
    static void grantUseTenant(Client& client) {
        User user(UserRequest(UserName("useTenant"_sd, "admin"_sd), boost::none));
        user.setPrivileges(
            {Privilege(ResourcePattern::forClusterResource(boost::none), ActionType::useTenant)});
        auto* as = dynamic_cast<AuthorizationSessionImpl*>(AuthorizationSession::get(client));
        if (as->_authenticatedUser != boost::none) {
            as->logoutAllDatabases(&client, "AuthorizationSessionImplTestHelper"_sd);
        }
        as->_authenticatedUser = std::move(user);
        as->_authenticationMode = AuthorizationSession::AuthenticationMode::kConnection;
        as->_updateInternalAuthorizationState();
    }
};

namespace auth {
namespace {

class ValidatedTenancyScopeTestFixture : public mongo::ScopedGlobalServiceContextForTest,
                                         public unittest::Test {
protected:
    void setUp() final {
        auto authzManagerState = std::make_unique<AuthzManagerExternalStateMock>();
        auto authzManager = std::make_unique<AuthorizationManagerImpl>(
            getServiceContext(), std::move(authzManagerState));
        authzManager->setAuthEnabled(true);
        AuthorizationManager::set(getServiceContext(), std::move(authzManager));

        client = getServiceContext()->getService()->makeClient("test");
    }

    std::string makeSecurityToken(const UserName& userName,
                                  ValidatedTenancyScope::TenantProtocol protocol =
                                      ValidatedTenancyScope::TenantProtocol::kDefault) {
        return auth::ValidatedTenancyScopeFactory::create(
                   userName,
                   "secret"_sd,
                   protocol,
                   auth::ValidatedTenancyScopeFactory::TokenForTestingTag{})
            .getOriginalToken()
            .toString();
    }

    ServiceContext::UniqueClient client;
};

void assertIdenticalVTS(const ValidatedTenancyScope& a, const ValidatedTenancyScope& b) {
    ASSERT_EQ(a.getOriginalToken(), b.getOriginalToken());
    // Generally the following MUST be equal if the above is equal, else the VTS ctor has gone
    // deeply wrong.
    ASSERT_EQ(a.hasAuthenticatedUser(), b.hasAuthenticatedUser());
    if (a.hasAuthenticatedUser()) {
        auto aUser = a.authenticatedUser().toBSON(true);
        auto bUser = b.authenticatedUser().toBSON(true);
        ASSERT_BSONOBJ_EQ(aUser, bUser);
    }
    ASSERT_EQ(a.hasTenantId(), b.hasTenantId());
    if (a.hasTenantId()) {
        ASSERT_EQ(a.tenantId().toString(), b.tenantId().toString());
    }
    ASSERT_EQ(a.getExpiration().toString(), b.getExpiration().toString());
    ASSERT_EQ(a.isFromAtlasProxy(), b.isFromAtlasProxy());
}

TEST_F(ValidatedTenancyScopeTestFixture, MultitenancySupportOffWithoutTenantOK) {
    RAIIServerParameterControllerForTest multitenancyController("multitenancySupport", false);
    auto body = BSON("$db"
                     << "foo");

    auto validated = ValidatedTenancyScopeFactory::parse(client.get(), body, {});
    ASSERT_TRUE(validated == boost::none);
}

TEST_F(ValidatedTenancyScopeTestFixture, MultitenancySupportWithSecurityTokenOK) {
    RAIIServerParameterControllerForTest multitenancyController("multitenancySupport", true);
    RAIIServerParameterControllerForTest securityTokenController("featureFlagSecurityToken", true);
    RAIIServerParameterControllerForTest secretController("testOnlyValidatedTenancyScopeKey",
                                                          "secret");

    const TenantId kTenantId(OID::gen());
    auto body = BSON("ping" << 1);
    UserName user("user", "admin", kTenantId);
    auto token = makeSecurityToken(user);

    auto validated = ValidatedTenancyScopeFactory::parse(client.get(), body, token);
    ASSERT_TRUE(validated != boost::none);
    ASSERT_TRUE(validated->hasTenantId());
    ASSERT_TRUE(validated->tenantId() == kTenantId);
    ASSERT_TRUE(validated->hasAuthenticatedUser());
    ASSERT_TRUE(validated->authenticatedUser() == user);
}

// TODO SERVER-66822: Re-enable this test case.
// TEST_F(ValidatedTenancyScopeTestFixture, MultitenancySupportWithoutTenantAndSecurityTokenNOK) {
//     RAIIServerParameterControllerForTest multitenancyController("multitenancySupport", true);
//     auto body = BSON("ping" << 1);
//     AuthorizationSessionImplTestHelper::grantUseTenant(*(client.get()));
//     ASSERT_THROWS_CODE(ValidatedTenancyScopeFactory::parse(client.get(), body, {}), DBException,
//     ErrorCodes::Unauthorized);
// }

TEST_F(ValidatedTenancyScopeTestFixture, NoScopeKey) {
    RAIIServerParameterControllerForTest multitenancyController("multitenancySupport", true);
    RAIIServerParameterControllerForTest securityTokenController("featureFlagSecurityToken", true);

    UserName user("user", "admin", TenantId(OID::gen()));
    auto token = makeSecurityToken(user);
    ASSERT_THROWS_CODE_AND_WHAT(
        ValidatedTenancyScopeFactory::parse(client.get(), {}, token),
        DBException,
        ErrorCodes::OperationFailed,
        "Unable to validate test tokens when testOnlyValidatedTenancyScopeKey is not provided");
}

TEST_F(ValidatedTenancyScopeTestFixture, WrongScopeKey) {
    RAIIServerParameterControllerForTest multitenancyController("multitenancySupport", true);
    RAIIServerParameterControllerForTest securityTokenController("featureFlagSecurityToken", true);
    RAIIServerParameterControllerForTest secretController("testOnlyValidatedTenancyScopeKey",
                                                          "password");  // != "secret"

    UserName user("user", "admin", TenantId(OID::gen()));
    auto token = makeSecurityToken(user);
    ASSERT_THROWS_CODE_AND_WHAT(ValidatedTenancyScopeFactory::parse(client.get(), {}, token),
                                DBException,
                                ErrorCodes::Unauthorized,
                                "Token signature invalid");
}

TEST_F(ValidatedTenancyScopeTestFixture, SecurityTokenDoesNotExpectPrefix) {
    RAIIServerParameterControllerForTest multitenancyController("multitenancySupport", true);
    RAIIServerParameterControllerForTest securityTokenController("featureFlagSecurityToken", true);
    RAIIServerParameterControllerForTest secretController("testOnlyValidatedTenancyScopeKey",
                                                          "secret");

    auto kOid = OID::gen();
    const TenantId kTenantId(kOid);
    auto body = BSON("ping" << 1);
    UserName user("user", "admin", kTenantId);
    auto token = makeSecurityToken(user, ValidatedTenancyScope::TenantProtocol::kDefault);
    auto validated = ValidatedTenancyScopeFactory::parse(client.get(), body, token);

    ASSERT_TRUE(validated->tenantId() == kTenantId);
    ASSERT_FALSE(validated->isFromAtlasProxy());

    token = makeSecurityToken(user, ValidatedTenancyScope::TenantProtocol::kAtlasProxy);
    ASSERT_THROWS_CODE(
        ValidatedTenancyScopeFactory::parse(client.get(), body, token), DBException, 8154400);
}

TEST_F(ValidatedTenancyScopeTestFixture, SecurityTokenHasPrefixExpectPrefix) {
    RAIIServerParameterControllerForTest multitenancyController("multitenancySupport", true);
    RAIIServerParameterControllerForTest securityTokenController("featureFlagSecurityToken", true);
    RAIIServerParameterControllerForTest secretController("testOnlyValidatedTenancyScopeKey",
                                                          "secret");

    auto kOid = OID::gen();
    const TenantId kTenantId(kOid);
    auto body = BSON("ping" << 1);
    UserName user("user", "admin", kTenantId);
    auto token = makeSecurityToken(user, ValidatedTenancyScope::TenantProtocol::kAtlasProxy);
    auto validated = ValidatedTenancyScopeFactory::parse(client.get(), body, token);

    ASSERT_TRUE(validated->tenantId() == kTenantId);
    ASSERT_TRUE(validated->isFromAtlasProxy());

    token = makeSecurityToken(user, ValidatedTenancyScope::TenantProtocol::kDefault);
    ASSERT_THROWS_CODE(
        ValidatedTenancyScopeFactory::parse(client.get(), body, token), DBException, 8154400);
}

TEST_F(ValidatedTenancyScopeTestFixture, VTSCreateFromOriginalToken) {
    RAIIServerParameterControllerForTest multitenancyController("multitenancySupport", true);
    RAIIServerParameterControllerForTest securityTokenController("featureFlagSecurityToken", true);
    RAIIServerParameterControllerForTest secretController("testOnlyValidatedTenancyScopeKey",
                                                          "secret");

    const TenantId kTenantId(OID::gen());
    const auto body = BSON("ping" << 1);
    UserName user("user", "admin", kTenantId);
    const auto token = makeSecurityToken(user, ValidatedTenancyScope::TenantProtocol::kAtlasProxy);
    const auto vts = ValidatedTenancyScopeFactory::parse(client.get(), body, token);

    // check that a VTS created from another VTS token are equal.
    auto copyVts = ValidatedTenancyScopeFactory::parse(client.get(), {}, vts->getOriginalToken());
    assertIdenticalVTS(*vts, *copyVts);
}

}  // namespace
}  // namespace auth
}  // namespace mongo
