/**
 *    Copyright (C) 2024-present MongoDB, Inc.
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
#include "mongo/db/auth/validated_tenancy_scope.h"

namespace mongo {

class Client;
class OperationContext;

namespace auth {

class ValidatedTenancyScopeFactory {
public:
    /**
     * Parse the provided command {body} and {securityToken}.
     * 1. If a `"$tenant"` field is found in {body}, and the connection
     * is authorized for cluster{useTenant}, a simple Tenant-only VTS will be returned.
     * 2. If an unsigned {securityToken} is provided, we delegate to parseUnsignedToken().
     * 3. If a signed {securityToken} is provided, we delegate to parseToken().
     *
     * Note that specifying both `"$tenant"` in {body} and a non-empty {securityToken} is an error.
     * If neither is provided, this method returns `boost::none`.
     */
    static boost::optional<ValidatedTenancyScope> parse(Client* client,
                                                        BSONObj body,
                                                        StringData securityToken);

private:
    /**
     * Transitional token mode used to convey TenantId and Protocol ONLY.
     * These tokens do not need to be signed, however, they are only valid
     * when provided by clients who are already authenticated and posess
     * cluster{useTenant} privilege.
     */
    static ValidatedTenancyScope parseUnsignedToken(Client* client, StringData securityToken);

    /**
     * Validates a JWS signature on the provided JWT header and token,
     * then extracts authenticatedUser, TenantId, and/or TenantProtocol.
     */
    static ValidatedTenancyScope parseToken(Client* client, StringData securityToken);
};

}  // namespace auth
}  // namespace mongo
