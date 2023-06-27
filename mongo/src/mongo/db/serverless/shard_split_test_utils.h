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

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>
#include <string>
#include <vector>

#include "mongo/db/tenant_id.h"
#include "mongo/util/net/hostandport.h"
#include "mongo/util/uuid.h"

namespace mongo {
class OperationContext;
class ServiceContext;

namespace test {
namespace shard_split {

// Scoped guard to ensure tenant blockers are removed in case a test case fails and throws an
// exception. If we do not remove the blockers, it triggers an invariant upon destruction of the
// test fixture, which introduces additional errors in the test and makes debugging harder.
class ScopedTenantAccessBlocker {
public:
    ScopedTenantAccessBlocker(const UUID& uuid, OperationContext* opCtx);
    ~ScopedTenantAccessBlocker();

    void dismiss();

private:
    boost::optional<UUID> _uuid;
    OperationContext* _opCtx;
};

/**
 * Build a new configuration with tagged nodes, and install it in ReplicationCoordinatorMock
 */
void reconfigToAddRecipientNodes(ServiceContext* serviceContext,
                                 const std::string& recipientTagName,
                                 const std::vector<HostAndPort>& donorNodes,
                                 const std::vector<HostAndPort>& recipientNodes);

}  // namespace shard_split
}  // namespace test
}  // namespace mongo
