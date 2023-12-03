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

#include "mongo/db/s/global_index/global_index_metrics.h"

#include <absl/container/node_hash_map.h>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <fmt/format.h>
#include <utility>
#include <vector>

#include <boost/optional/optional.hpp>

#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/create_indexes_gen.h"
#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/s/global_index/global_index_cloner_gen.h"
#include "mongo/util/namespace_string_util.h"

namespace mongo {
namespace global_index {
namespace {

using TimedPhase = GlobalIndexMetrics::TimedPhase;
const auto kTimedPhaseNamesMap = [] {
    return GlobalIndexMetrics::TimedPhaseNameMap{
        {TimedPhase::kCloning, "totalCopyTimeElapsedSecs"}};
}();


inline GlobalIndexMetrics::State getDefaultState(GlobalIndexMetrics::Role role) {
    using Role = GlobalIndexMetrics::Role;
    switch (role) {
        case Role::kCoordinator:
            return GlobalIndexCoordinatorStateEnumPlaceholder::kUnused;
        case Role::kRecipient:
            return GlobalIndexClonerStateEnum::kUnused;
        case Role::kDonor:
            MONGO_UNREACHABLE;
    }
    MONGO_UNREACHABLE;
}

// Returns the originalCommand with the createIndexes, key and unique fields added.
BSONObj createOriginalCommand(const NamespaceString& nss, BSONObj keyPattern, bool unique) {

    using Doc = Document;
    using Arr = std::vector<Value>;
    using V = Value;

    return Doc{{"originatingCommand",
                V{Doc{{"createIndexes",
                       V{StringData{NamespaceStringUtil::serialize(
                           nss, SerializationContext::stateDefault())}}},
                      {"key", std::move(keyPattern)},
                      {"unique", V{unique}}}}}}
        .toBson();
}
}  // namespace

GlobalIndexMetrics::GlobalIndexMetrics(UUID instanceId,
                                       BSONObj originatingCommand,
                                       NamespaceString nss,
                                       Role role,
                                       Date_t startTime,
                                       ClockSource* clockSource,
                                       ShardingDataTransformCumulativeMetrics* cumulativeMetrics)
    : Base{std::move(instanceId),
           std::move(originatingCommand),
           std::move(nss),
           role,
           startTime,
           clockSource,
           cumulativeMetrics,
           std::make_unique<GlobalIndexMetricsFieldNameProvider>()},
      _stateHolder{getGlobalIndexCumulativeMetrics(), getDefaultState(role)},
      _scopedObserver(registerInstanceMetrics()),
      _globalIndexFieldNames{static_cast<GlobalIndexMetricsFieldNameProvider*>(_fieldNames.get())} {
}

GlobalIndexMetrics::~GlobalIndexMetrics() {
    // Deregister the observer first to ensure that the observer will no longer be able to reach
    // this object while destructor is running.
    _scopedObserver.reset();
}

std::string GlobalIndexMetrics::createOperationDescription() const noexcept {
    return fmt::format("GlobalIndexMetrics{}Service {}",
                       ShardingDataTransformMetrics::getRoleName(_role),
                       _instanceId.toString());
}

GlobalIndexCumulativeMetrics* GlobalIndexMetrics::getGlobalIndexCumulativeMetrics() {
    return dynamic_cast<GlobalIndexCumulativeMetrics*>(getCumulativeMetrics());
}

boost::optional<Milliseconds> GlobalIndexMetrics::getRecipientHighEstimateRemainingTimeMillis()
    const {
    return boost::none;
}

BSONObj GlobalIndexMetrics::getOriginalCommand(const CommonGlobalIndexMetadata& metadata) {
    CreateIndexesCommand cmd(metadata.getNss(), {metadata.getIndexSpec().toBSON()});
    return cmd.toBSON({});
}

StringData GlobalIndexMetrics::getStateString() const noexcept {
    return visit(OverloadedVisitor{
                     [](GlobalIndexCoordinatorStateEnumPlaceholder state) { return "TODO"_sd; },
                     [](GlobalIndexClonerStateEnum state) {
                         return GlobalIndexClonerState_serializer(state);
                     }},
                 _stateHolder.getState());
}

BSONObj GlobalIndexMetrics::reportForCurrentOp() const noexcept {
    BSONObjBuilder builder;
    reportDurationsForAllPhases<Seconds>(kTimedPhaseNamesMap, getClockSource(), &builder);
    builder.appendElementsUnique(ShardingDataTransformInstanceMetrics::reportForCurrentOp());
    return builder.obj();
}

}  // namespace global_index
}  // namespace mongo
