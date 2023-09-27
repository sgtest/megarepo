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

#include "mongo/db/s/resharding/resharding_cumulative_metrics.h"

#include <absl/container/node_hash_map.h>
#include <array>
#include <boost/move/utility_core.hpp>
#include <memory>
#include <utility>
#include <variant>

#include <boost/optional/optional.hpp>

#include "mongo/s/resharding/resharding_feature_flag_gen.h"

namespace mongo {

namespace {

constexpr auto kResharding = "resharding";

const auto kReportedStateFieldNamesMap = [] {
    return ReshardingCumulativeMetrics::StateFieldNameMap{
        {CoordinatorStateEnum::kInitializing, "countInstancesInCoordinatorState1Initializing"},
        {CoordinatorStateEnum::kPreparingToDonate,
         "countInstancesInCoordinatorState2PreparingToDonate"},
        {CoordinatorStateEnum::kCloning, "countInstancesInCoordinatorState3Cloning"},
        {CoordinatorStateEnum::kApplying, "countInstancesInCoordinatorState4Applying"},
        {CoordinatorStateEnum::kBlockingWrites, "countInstancesInCoordinatorState5BlockingWrites"},
        {CoordinatorStateEnum::kAborting, "countInstancesInCoordinatorState6Aborting"},
        {CoordinatorStateEnum::kCommitting, "countInstancesInCoordinatorState7Committing"},
        {DonorStateEnum::kPreparingToDonate, "countInstancesInDonorState1PreparingToDonate"},
        {DonorStateEnum::kDonatingInitialData, "countInstancesInDonorState2DonatingInitialData"},
        {DonorStateEnum::kDonatingOplogEntries, "countInstancesInDonorState3DonatingOplogEntries"},
        {DonorStateEnum::kPreparingToBlockWrites,
         "countInstancesInDonorState4PreparingToBlockWrites"},
        {DonorStateEnum::kError, "countInstancesInDonorState5Error"},
        {DonorStateEnum::kBlockingWrites, "countInstancesInDonorState6BlockingWrites"},
        {DonorStateEnum::kDone, "countInstancesInDonorState7Done"},
        {RecipientStateEnum::kAwaitingFetchTimestamp,
         "kCountInstancesInRecipientState1AwaitingFetchTimestamp"},
        {RecipientStateEnum::kCreatingCollection,
         "countInstancesInRecipientState2CreatingCollection"},
        {RecipientStateEnum::kCloning, "countInstancesInRecipientState3Cloning"},
        {RecipientStateEnum::kBuildingIndex, "countInstancesInRecipientState4BuildingIndex"},
        {RecipientStateEnum::kApplying, "countInstancesInRecipientState5Applying"},
        {RecipientStateEnum::kError, "countInstancesInRecipientState6Error"},
        {RecipientStateEnum::kStrictConsistency,
         "countInstancesInRecipientState7StrictConsistency"},
        {RecipientStateEnum::kDone, "countInstancesInRecipientState8Done"},
    };
}();

}  // namespace

boost::optional<StringData> ReshardingCumulativeMetrics::fieldNameFor(AnyState state) {
    return getNameFor(state, kReportedStateFieldNamesMap);
}

ReshardingCumulativeMetrics::ReshardingCumulativeMetrics()
    : ReshardingCumulativeMetrics(kResharding) {}

ReshardingCumulativeMetrics::ReshardingCumulativeMetrics(const std::string& rootName)
    : resharding_cumulative_metrics::Base(
          rootName, std::make_unique<ReshardingCumulativeMetricsFieldNameProvider>()),
      _fieldNames(
          static_cast<const ReshardingCumulativeMetricsFieldNameProvider*>(getFieldNames())) {}

void ReshardingCumulativeMetrics::reportActive(BSONObjBuilder* bob) const {
    ShardingDataTransformCumulativeMetrics::reportActive(bob);
    reportOplogApplicationCountMetrics(_fieldNames, bob);
}

void ReshardingCumulativeMetrics::reportLatencies(BSONObjBuilder* bob) const {
    ShardingDataTransformCumulativeMetrics::reportLatencies(bob);
    reportOplogApplicationLatencyMetrics(_fieldNames, bob);
}

void ReshardingCumulativeMetrics::reportCurrentInSteps(BSONObjBuilder* bob) const {
    ShardingDataTransformCumulativeMetrics::reportCurrentInSteps(bob);
    reportCountsForAllStates(kReportedStateFieldNamesMap, bob);
}

void ReshardingCumulativeMetrics::reportForServerStatus(BSONObjBuilder* bob) const {
    if (!_operationWasAttempted.load()) {
        return;
    }

    BSONObjBuilder root(bob->subobjStart(_rootSectionName));
    if (_rootSectionName == kResharding &&
        resharding::gFeatureFlagReshardingImprovements.isEnabledAndIgnoreFCVUnsafeAtStartup()) {
        root.append(_fieldNames->getForCountSameKeyStarted(), _countSameKeyStarted.load());
        root.append(_fieldNames->getForCountSameKeySucceeded(), _countSameKeySucceeded.load());
        root.append(_fieldNames->getForCountSameKeyFailed(), _countSameKeyFailed.load());
        root.append(_fieldNames->getForCountSameKeyCanceled(), _countSameKeyCancelled.load());
    }
    {
        BSONObjBuilder builder;
        Base::reportForServerStatus(&builder);
        root.appendElementsUnique(builder.obj().getObjectField(_rootSectionName));
    }
}

void ReshardingCumulativeMetrics::onStarted(bool isSameKeyResharding) {
    if (_rootSectionName == kResharding && isSameKeyResharding) {
        _countSameKeyStarted.fetchAndAdd(1);
    }
    Base::onStarted();
}

void ReshardingCumulativeMetrics::onSuccess(bool isSameKeyResharding) {
    if (_rootSectionName == kResharding && isSameKeyResharding) {
        _countSameKeySucceeded.fetchAndAdd(1);
    }
    Base::onSuccess();
}

void ReshardingCumulativeMetrics::onFailure(bool isSameKeyResharding) {
    if (_rootSectionName == kResharding && isSameKeyResharding) {
        _countSameKeyFailed.fetchAndAdd(1);
    }
    Base::onFailure();
}

void ReshardingCumulativeMetrics::onCanceled(bool isSameKeyResharding) {
    if (_rootSectionName == kResharding && isSameKeyResharding) {
        _countSameKeyCancelled.fetchAndAdd(1);
    }
    Base::onCanceled();
}

}  // namespace mongo
