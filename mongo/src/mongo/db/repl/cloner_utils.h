/**
 *    Copyright (C) 2020-present MongoDB, Inc.
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

#include <boost/optional/optional.hpp>
#include <string>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/timestamp.h"
#include "mongo/db/database_name.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/serverless/serverless_types_gen.h"
#include "mongo/db/tenant_id.h"

namespace mongo {
namespace repl {

/**
 * Contains a variety of static helper methods used by any set of the replication cloners.
 */
class ClonerUtils {
    ClonerUtils(const ClonerUtils&) = delete;
    ClonerUtils& operator=(const ClonerUtils&) = delete;

public:
    /**
     * Builds a regex that matches database names prefixed with a specific tenantId.
     */
    static BSONObj makeTenantDatabaseRegex(StringData prefix);

    /**
     * Builds a filter that matches database names prefixed with a specific tenantId.
     */
    static BSONObj makeTenantDatabaseFilter(StringData prefix);

    /**
     * Assembles a majority read using the operationTime specified as the afterClusterTime.
     */
    static BSONObj buildMajorityWaitRequest(Timestamp operationTime);

    /**
     * Checks if the database belongs to the given tenant.
     */
    static bool isDatabaseForTenant(const DatabaseName& db,
                                    const boost::optional<TenantId>& prefix,
                                    MigrationProtocolEnum protocol);
};


}  // namespace repl
}  // namespace mongo
