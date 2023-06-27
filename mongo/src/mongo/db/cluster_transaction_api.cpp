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

#include "mongo/db/cluster_transaction_api.h"

#include <fmt/format.h>
#include <string>

#include <absl/container/node_hash_map.h>
#include <absl/meta/type_traits.h>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/s/service_entry_point_mongos.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/string_map.h"

namespace mongo::txn_api::details {

namespace {

StringMap<std::string> clusterCommandTranslations = {
    {"abortTransaction", "clusterAbortTransaction"},
    {"aggregate", "clusterAggregate"},
    {"commitTransaction", "clusterCommitTransaction"},
    {"delete", "clusterDelete"},
    {"find", "clusterFind"},
    {"getMore", "clusterGetMore"},
    {"insert", "clusterInsert"},
    {"update", "clusterUpdate"}};

BSONObj replaceCommandNameWithClusterCommandName(BSONObj cmdObj) {
    auto cmdName = cmdObj.firstElement().fieldNameStringData();
    auto newNameIt = clusterCommandTranslations.find(cmdName);
    uassert(6349501,
            "Cannot use unsupported command {} with cluster transaction API"_format(cmdName),
            newNameIt != clusterCommandTranslations.end());

    return cmdObj.replaceFieldNames(BSON(newNameIt->second << 1));
}

}  // namespace

BSONObj ClusterSEPTransactionClientBehaviors::maybeModifyCommand(BSONObj cmdObj) const {
    return replaceCommandNameWithClusterCommandName(cmdObj);
}

Future<DbResponse> ClusterSEPTransactionClientBehaviors::handleRequest(
    OperationContext* opCtx, const Message& request) const {
    return ServiceEntryPointMongos::handleRequestImpl(opCtx, request);
}

}  // namespace mongo::txn_api::details
