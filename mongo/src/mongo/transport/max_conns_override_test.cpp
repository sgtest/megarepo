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
#include <variant>
#include <vector>

#include "mongo/base/string_data.h"
#include "mongo/transport/mock_session.h"
#include "mongo/transport/service_entry_point_impl.h"
#include "mongo/transport/session.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/net/cidr.h"
#include "mongo/util/net/hostandport.h"
#include "mongo/util/net/sockaddr.h"

namespace mongo {
namespace {

using ExemptionVector = std::vector<stdx::variant<CIDR, std::string>>;

template <typename T>
stdx::variant<CIDR, std::string> makeExemption(T exemption) {
    auto swCIDR = CIDR::parse(exemption);
    if (swCIDR.isOK()) {
        return swCIDR.getValue();
    } else {
        return std::string{exemption};
    }
}

std::shared_ptr<transport::Session> makeIPSession(StringData ip) {
    return transport::MockSession::create(HostAndPort(ip.toString(), 27017),
                                          HostAndPort(),
                                          SockAddr::create(ip, 27017, AF_INET),
                                          SockAddr(),
                                          nullptr);
}

#ifndef _WIN32
std::shared_ptr<transport::Session> makeUNIXSession(StringData path) {
    return transport::MockSession::create(HostAndPort(""_sd.toString(), -1),
                                          HostAndPort(path.toString(), -1),
                                          SockAddr::create(""_sd, -1, AF_UNIX),
                                          SockAddr::create(path, -1, AF_UNIX),

                                          nullptr);
}
#endif

TEST(MaxConnsOverride, NormalCIDR) {
    ExemptionVector cidrOnly{makeExemption("127.0.0.1"), makeExemption("10.0.0.0/24")};

    ASSERT_TRUE(shouldOverrideMaxConns(makeIPSession("127.0.0.1"), cidrOnly));
    ASSERT_TRUE(shouldOverrideMaxConns(makeIPSession("10.0.0.35"), cidrOnly));
    ASSERT_FALSE(shouldOverrideMaxConns(makeIPSession("192.168.0.53"), cidrOnly));
}

#ifndef _WIN32
TEST(MaxConnsOverride, UNIXPaths) {
    ExemptionVector mixed{makeExemption("127.0.0.1"),
                          makeExemption("10.0.0.0/24"),
                          makeExemption("/tmp/mongod.sock")};

    ASSERT_TRUE(shouldOverrideMaxConns(makeIPSession("127.0.0.1"), mixed));
    ASSERT_TRUE(shouldOverrideMaxConns(makeIPSession("10.0.0.35"), mixed));
    ASSERT_FALSE(shouldOverrideMaxConns(makeIPSession("192.168.0.53"), mixed));
    ASSERT_TRUE(shouldOverrideMaxConns(makeUNIXSession("/tmp/mongod.sock"), mixed));
    ASSERT_FALSE(shouldOverrideMaxConns(makeUNIXSession("/tmp/other-mongod.sock"), mixed));
}
#endif

}  // namespace
}  // namespace mongo
