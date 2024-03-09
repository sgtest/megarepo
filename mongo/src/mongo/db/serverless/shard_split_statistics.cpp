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

#include "mongo/db/serverless/shard_split_statistics.h"

#include <memory>
#include <utility>

#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/db/commands/server_status.h"
#include "mongo/db/operation_context.h"
#include "mongo/util/decorable.h"

namespace mongo {

const ServiceContext::Decoration<ShardSplitStatistics> statisticsDecoration =
    ServiceContext::declareDecoration<ShardSplitStatistics>();

ShardSplitStatistics* ShardSplitStatistics::get(ServiceContext* service) {
    return &statisticsDecoration(service);
}

void ShardSplitStatistics::incrementTotalCommitted(Milliseconds durationWithCatchup,
                                                   Milliseconds durationWithoutCatchup) {
    _totalCommitted.fetchAndAddRelaxed(1);
    _totalCommittedDurationMillis.fetchAndAdd(durationCount<Milliseconds>(durationWithCatchup));
    _totalCommittedDurationWithoutCatchupMillis.fetchAndAdd(
        durationCount<Milliseconds>(durationWithoutCatchup));
}

void ShardSplitStatistics::incrementTotalAborted() {
    _totalAborted.fetchAndAddRelaxed(1);
}

void ShardSplitStatistics::appendInfoForServerStatus(BSONObjBuilder* builder) const {
    builder->append("totalCommitted", _totalCommitted.load());
    builder->append("totalCommittedDurationMillis", _totalCommittedDurationMillis.load());
    builder->append("totalCommittedDurationWithoutCatchupMillis",
                    _totalCommittedDurationWithoutCatchupMillis.load());
    builder->append("totalAborted", _totalAborted.load());
}

class ShardSplitServerStatus final : public ServerStatusSection {
public:
    using ServerStatusSection::ServerStatusSection;

    bool includeByDefault() const override {
        return true;
    }

    BSONObj generateSection(OperationContext* opCtx,
                            const BSONElement& configElement) const override {
        BSONObjBuilder result;
        ShardSplitStatistics::get(opCtx->getServiceContext())->appendInfoForServerStatus(&result);
        return result.obj();
    }
};
auto shardSplitServerStatus =
    *ServerStatusSectionBuilder<ShardSplitServerStatus>("shardSplits").forShard();

}  // namespace mongo
