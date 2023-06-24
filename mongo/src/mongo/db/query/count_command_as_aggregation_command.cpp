/**
 *    Copyright (C) 2019-present MongoDB, Inc.
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

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/query/count_command_as_aggregation_command.h"
#include "mongo/db/query/query_request_helper.h"

namespace mongo {
namespace {

const char kQueryField[] = "query";
const char kLimitField[] = "limit";
const char kSkipField[] = "skip";
const char kHintField[] = "hint";
const char kCollationField[] = "collation";
const char kExplainField[] = "explain";
const char kMaxTimeMSField[] = "maxTimeMS";
const char kReadConcernField[] = "readConcern";
}  // namespace

StatusWith<BSONObj> countCommandAsAggregationCommand(const CountCommandRequest& cmd,
                                                     const NamespaceString& nss) {
    BSONObjBuilder aggregationBuilder;
    aggregationBuilder.append("aggregate", nss.coll());

    // Build an aggregation pipeline that performs the counting. We add stages that satisfy the
    // query, skip and limit before finishing with the actual $count stage.
    BSONArrayBuilder pipelineBuilder(aggregationBuilder.subarrayStart("pipeline"));

    auto queryObj = cmd.getQuery();
    if (!queryObj.isEmpty()) {
        BSONObjBuilder matchBuilder(pipelineBuilder.subobjStart());
        matchBuilder.append("$match", queryObj);
        matchBuilder.doneFast();
    }

    if (auto skip = cmd.getSkip()) {
        BSONObjBuilder skipBuilder(pipelineBuilder.subobjStart());
        skipBuilder.append("$skip", skip.value());
        skipBuilder.doneFast();
    }

    if (auto limit = cmd.getLimit()) {
        BSONObjBuilder limitBuilder(pipelineBuilder.subobjStart());
        limitBuilder.append("$limit", limit.value());
        limitBuilder.doneFast();
    }

    BSONObjBuilder countBuilder(pipelineBuilder.subobjStart());
    countBuilder.append("$count", "count");
    countBuilder.doneFast();
    pipelineBuilder.doneFast();

    // Complete the command by appending the other options to the aggregate command.
    if (auto collation = cmd.getCollation()) {
        aggregationBuilder.append(kCollationField, collation.value());
    }

    aggregationBuilder.append(kHintField, cmd.getHint());

    if (auto maxTime = cmd.getMaxTimeMS()) {
        if (maxTime.value() > 0) {
            aggregationBuilder.append(kMaxTimeMSField, maxTime.value());
        }
    }

    if (auto readConcern = cmd.getReadConcern()) {
        if (!readConcern->isEmpty()) {
            aggregationBuilder.append(kReadConcernField, readConcern.value());
        }
    }

    if (auto unwrapped = cmd.getQueryOptions()) {
        if (!unwrapped->isEmpty()) {
            aggregationBuilder.append(query_request_helper::kUnwrappedReadPrefField,
                                      unwrapped.value());
        }
    }

    // The 'cursor' option is always specified so that aggregation uses the cursor interface.
    aggregationBuilder.append("cursor", BSONObj());

    return aggregationBuilder.obj();
}


}  // namespace mongo
