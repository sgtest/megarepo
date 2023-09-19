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


#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional.hpp>
#include <map>
#include <memory>
#include <string>
#include <tuple>
#include <utility>

#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>

#include "mongo/bson/bsonobj.h"
#include "mongo/db/s/metrics/sharding_data_transform_metrics.h"
#include "mongo/db/s/metrics/sharding_data_transform_metrics_test_fixture.h"
#include "mongo/db/s/resharding/resharding_cumulative_metrics.h"
#include "mongo/idl/server_parameter_test_util.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/assert_util_core.h"
#include "mongo/util/duration.h"
#include "mongo/util/time_support.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kTest

namespace mongo {
namespace {

constexpr auto kResharding = "resharding";

class ReshardingCumulativeMetricsTest : public ShardingDataTransformMetricsTestFixture {
protected:
    void setUp() override {
        ShardingDataTransformMetricsTestFixture::setUp();
        _reshardingCumulativeMetrics =
            static_cast<ReshardingCumulativeMetrics*>(_cumulativeMetrics.get());
        _fieldNames = std::make_unique<ReshardingCumulativeMetricsFieldNameProvider>();
    }

    virtual std::unique_ptr<ShardingDataTransformCumulativeMetrics> initializeCumulativeMetrics()
        override {
        return std::make_unique<ReshardingCumulativeMetrics>();
    }

    virtual StringData getRootSectionName() override {
        return kResharding;
    }

    template <typename T>
    StringData fieldNameFor(T state) {
        auto maybeFieldName = ReshardingCumulativeMetrics::fieldNameFor(state);
        invariant(maybeFieldName.has_value());
        return *maybeFieldName;
    }

    BSONObj getStateSubObj(const ReshardingCumulativeMetrics* metrics) {
        BSONObjBuilder bob;
        metrics->reportForServerStatus(&bob);
        auto report = bob.done();
        return report.getObjectField(kResharding).getObjectField("currentInSteps").getOwned();
    }

    bool checkCoordinateStateField(const ReshardingCumulativeMetrics* metrics,
                                   boost::optional<CoordinatorStateEnum> expectedState) {
        auto serverStatusSubObj = getStateSubObj(metrics);
        std::map<std::string, int> expectedStateFieldCount;

        auto addExpectedField = [&](CoordinatorStateEnum stateToPopulate) {
            expectedStateFieldCount.emplace(
                fieldNameFor(stateToPopulate),
                ((expectedState && (stateToPopulate == expectedState)) ? 1 : 0));
        };

        addExpectedField(CoordinatorStateEnum::kInitializing);
        addExpectedField(CoordinatorStateEnum::kPreparingToDonate);
        addExpectedField(CoordinatorStateEnum::kCloning);
        addExpectedField(CoordinatorStateEnum::kApplying);
        addExpectedField(CoordinatorStateEnum::kBlockingWrites);
        addExpectedField(CoordinatorStateEnum::kAborting);
        addExpectedField(CoordinatorStateEnum::kCommitting);

        for (const auto& fieldNameAndState : expectedStateFieldCount) {
            const auto actualValue = serverStatusSubObj.getIntField(fieldNameAndState.first);
            if (actualValue != fieldNameAndState.second) {
                LOGV2_DEBUG(6438600,
                            0,
                            "coordinator state field value does not match expected value",
                            "field"_attr = fieldNameAndState.first,
                            "serverStatus"_attr = serverStatusSubObj);
                return false;
            }
        }

        return true;
    }

    bool checkDonorStateField(const ReshardingCumulativeMetrics* metrics,
                              boost::optional<DonorStateEnum> expectedState) {
        auto serverStatusSubObj = getStateSubObj(metrics);
        std::map<std::string, int> expectedStateFieldCount;

        auto addExpectedField = [&](DonorStateEnum stateToPopulate) {
            expectedStateFieldCount.emplace(
                fieldNameFor(stateToPopulate),
                ((expectedState && (stateToPopulate == expectedState)) ? 1 : 0));
        };

        addExpectedField(DonorStateEnum::kPreparingToDonate);
        addExpectedField(DonorStateEnum::kDonatingInitialData);
        addExpectedField(DonorStateEnum::kDonatingOplogEntries);
        addExpectedField(DonorStateEnum::kPreparingToBlockWrites);
        addExpectedField(DonorStateEnum::kError);
        addExpectedField(DonorStateEnum::kBlockingWrites);
        addExpectedField(DonorStateEnum::kDone);

        for (const auto& fieldNameAndState : expectedStateFieldCount) {
            const auto actualValue = serverStatusSubObj.getIntField(fieldNameAndState.first);
            if (actualValue != fieldNameAndState.second) {
                LOGV2_DEBUG(6438701,
                            0,
                            "Donor state field value does not match expected value",
                            "field"_attr = fieldNameAndState.first,
                            "serverStatus"_attr = serverStatusSubObj);
                return false;
            }
        }

        return true;
    }

    bool checkRecipientStateField(const ReshardingCumulativeMetrics* metrics,
                                  boost::optional<RecipientStateEnum> expectedState) {
        auto serverStatusSubObj = getStateSubObj(metrics);
        std::map<std::string, int> expectedStateFieldCount;

        auto addExpectedField = [&](RecipientStateEnum stateToPopulate) {
            expectedStateFieldCount.emplace(
                fieldNameFor(stateToPopulate),
                ((expectedState && (stateToPopulate == expectedState)) ? 1 : 0));
        };

        addExpectedField(RecipientStateEnum::kAwaitingFetchTimestamp);
        addExpectedField(RecipientStateEnum::kCreatingCollection);
        addExpectedField(RecipientStateEnum::kCloning);
        addExpectedField(RecipientStateEnum::kBuildingIndex);
        addExpectedField(RecipientStateEnum::kApplying);
        addExpectedField(RecipientStateEnum::kError);
        addExpectedField(RecipientStateEnum::kStrictConsistency);
        addExpectedField(RecipientStateEnum::kDone);

        for (const auto& fieldNameAndState : expectedStateFieldCount) {
            const auto actualValue = serverStatusSubObj.getIntField(fieldNameAndState.first);
            if (actualValue != fieldNameAndState.second) {
                LOGV2_DEBUG(6438901,
                            0,
                            "Recipient state field value does not match expected value",
                            "field"_attr = fieldNameAndState.first,
                            "serverStatus"_attr = serverStatusSubObj);
                return false;
            }
        }

        return true;
    }

    ReshardingCumulativeMetrics* _reshardingCumulativeMetrics;
    std::unique_ptr<ReshardingCumulativeMetricsFieldNameProvider> _fieldNames;
};


TEST_F(ReshardingCumulativeMetricsTest, ReportContainsInsertsDuringFetching) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(100), 100, 100, Role::kRecipient};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&recipient);

    auto latencies = getCumulativeMetricsReportForSection(kLatencies);
    ASSERT_EQ(latencies.getIntField("oplogFetchingTotalLocalInserts"), 0);
    ASSERT_EQ(latencies.getIntField("oplogFetchingTotalLocalInsertTimeMillis"), 0);

    _reshardingCumulativeMetrics->onLocalInsertDuringOplogFetching(Milliseconds(17));

    latencies = getCumulativeMetricsReportForSection(kLatencies);
    ASSERT_EQ(latencies.getIntField("oplogFetchingTotalLocalInserts"), 1);
    ASSERT_EQ(latencies.getIntField("oplogFetchingTotalLocalInsertTimeMillis"), 17);
}


TEST_F(ReshardingCumulativeMetricsTest, ReportContainsBatchRetrievedDuringApplying) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(100), 100, 100, Role::kRecipient};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&recipient);

    auto latencies = getCumulativeMetricsReportForSection(kLatencies);
    ASSERT_EQ(latencies.getIntField("oplogApplyingTotalLocalBatchesRetrieved"), 0);
    ASSERT_EQ(latencies.getIntField("oplogApplyingTotalLocalBatchRetrievalTimeMillis"), 0);

    _reshardingCumulativeMetrics->onBatchRetrievedDuringOplogApplying(Milliseconds(39));

    latencies = getCumulativeMetricsReportForSection(kLatencies);
    ASSERT_EQ(latencies.getIntField("oplogApplyingTotalLocalBatchesRetrieved"), 1);
    ASSERT_EQ(latencies.getIntField("oplogApplyingTotalLocalBatchRetrievalTimeMillis"), 39);
}


TEST_F(ReshardingCumulativeMetricsTest, ReportContainsBatchApplied) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(100), 100, 100, Role::kRecipient};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&recipient);

    auto latencies = getCumulativeMetricsReportForSection(kLatencies);
    ASSERT_EQ(latencies.getIntField("oplogApplyingTotalLocalBatchesApplied"), 0);
    ASSERT_EQ(latencies.getIntField("oplogApplyingTotalLocalBatchApplyTimeMillis"), 0);

    _reshardingCumulativeMetrics->onOplogLocalBatchApplied(Milliseconds(333));

    latencies = getCumulativeMetricsReportForSection(kLatencies);
    ASSERT_EQ(latencies.getIntField("oplogApplyingTotalLocalBatchesApplied"), 1);
    ASSERT_EQ(latencies.getIntField("oplogApplyingTotalLocalBatchApplyTimeMillis"), 333);
}

TEST_F(ReshardingCumulativeMetricsTest, ReportContainsInsertsApplied) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(100), 100, 100, Role::kRecipient};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&recipient);

    auto active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("insertsApplied"), 0);

    _reshardingCumulativeMetrics->onInsertApplied();

    active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("insertsApplied"), 1);
}

TEST_F(ReshardingCumulativeMetricsTest, ReportContainsUpdatesApplied) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(100), 100, 100, Role::kRecipient};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&recipient);

    auto active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("updatesApplied"), 0);

    _reshardingCumulativeMetrics->onUpdateApplied();

    active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("updatesApplied"), 1);
}

TEST_F(ReshardingCumulativeMetricsTest, ReportContainsDeletesApplied) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(100), 100, 100, Role::kRecipient};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&recipient);

    auto active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("deletesApplied"), 0);

    _reshardingCumulativeMetrics->onDeleteApplied();

    active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("deletesApplied"), 1);
}

TEST_F(ReshardingCumulativeMetricsTest, ReportContainsOplogEntriesFetched) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(100), 100, 100, Role::kRecipient};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&recipient);

    auto active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("oplogEntriesFetched"), 0);

    auto latencies = getCumulativeMetricsReportForSection(kLatencies);
    ASSERT_EQ(latencies.getIntField("oplogFetchingTotalRemoteBatchesRetrieved"), 0);
    ASSERT_EQ(latencies.getIntField("oplogFetchingTotalRemoteBatchRetrievalTimeMillis"), 0);

    _reshardingCumulativeMetrics->onOplogEntriesFetched(123);
    _reshardingCumulativeMetrics->onBatchRetrievedDuringOplogFetching(Milliseconds(43));

    active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("oplogEntriesFetched"), 123);

    latencies = getCumulativeMetricsReportForSection(kLatencies);
    ASSERT_EQ(latencies.getIntField("oplogFetchingTotalRemoteBatchesRetrieved"), 1);
    ASSERT_EQ(latencies.getIntField("oplogFetchingTotalRemoteBatchRetrievalTimeMillis"), 43);
}

TEST_F(ReshardingCumulativeMetricsTest, ReportContainsOplogEntriesApplied) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(100), 100, 100, Role::kRecipient};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&recipient);

    auto active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("oplogEntriesApplied"), 0);

    _reshardingCumulativeMetrics->onOplogEntriesApplied(99);

    active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("oplogEntriesApplied"), 99);
}

TEST_F(ReshardingCumulativeMetricsTest,
       SimulatedNormalCoordinatorStateTransitionReportsStateCorrectly) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock coordinator{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kCoordinator};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&coordinator);

    ASSERT(checkCoordinateStateField(_reshardingCumulativeMetrics, CoordinatorStateEnum::kUnused));

    boost::optional<CoordinatorStateEnum> prevState;
    boost::optional<CoordinatorStateEnum> nextState;

    auto simulateTransitionTo = [&](boost::optional<CoordinatorStateEnum> newState) {
        prevState = nextState;
        nextState = newState;
        _reshardingCumulativeMetrics->onStateTransition(prevState, nextState);
        return checkCoordinateStateField(_reshardingCumulativeMetrics, nextState);
    };

    ASSERT(simulateTransitionTo(CoordinatorStateEnum::kUnused));
    ASSERT(simulateTransitionTo(CoordinatorStateEnum::kInitializing));
    ASSERT(simulateTransitionTo(CoordinatorStateEnum::kPreparingToDonate));
    ASSERT(simulateTransitionTo(CoordinatorStateEnum::kCloning));
    ASSERT(simulateTransitionTo(CoordinatorStateEnum::kBlockingWrites));
    ASSERT(simulateTransitionTo(CoordinatorStateEnum::kCommitting));
    ASSERT(simulateTransitionTo(CoordinatorStateEnum::kDone));
    ASSERT(simulateTransitionTo(boost::none));
}

TEST_F(ReshardingCumulativeMetricsTest,
       SimulatedAbortedCoordinatorStateTransitionReportsStateCorrectly) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock coordinator{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kCoordinator};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&coordinator);

    ASSERT(checkCoordinateStateField(_reshardingCumulativeMetrics, CoordinatorStateEnum::kUnused));

    boost::optional<CoordinatorStateEnum> prevState;
    boost::optional<CoordinatorStateEnum> nextState;

    auto simulateTransitionTo = [&](boost::optional<CoordinatorStateEnum> newState) {
        prevState = nextState;
        nextState = newState;
        _reshardingCumulativeMetrics->onStateTransition(prevState, nextState);
        return checkCoordinateStateField(_reshardingCumulativeMetrics, nextState);
    };

    ASSERT(simulateTransitionTo(CoordinatorStateEnum::kUnused));
    ASSERT(simulateTransitionTo(CoordinatorStateEnum::kInitializing));
    ASSERT(simulateTransitionTo(CoordinatorStateEnum::kPreparingToDonate));
    ASSERT(simulateTransitionTo(CoordinatorStateEnum::kAborting));
    ASSERT(simulateTransitionTo(boost::none));
}

TEST_F(ReshardingCumulativeMetricsTest,
       SimulatedSteppedDownCoordinatorStateFromUnusedReportsStateCorrectly) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock coordinator{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kCoordinator};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&coordinator);

    boost::optional<CoordinatorStateEnum> initState = CoordinatorStateEnum::kUnused;
    ASSERT(checkCoordinateStateField(_reshardingCumulativeMetrics, initState));

    _reshardingCumulativeMetrics->onStateTransition(initState, {boost::none});
    ASSERT(checkCoordinateStateField(_reshardingCumulativeMetrics, initState));
}

TEST_F(ReshardingCumulativeMetricsTest,
       SimulatedSteppedDownCoordinatorStateTransitionReportsStateCorrectly) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock coordinator{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kCoordinator};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&coordinator);

    ASSERT(checkCoordinateStateField(_reshardingCumulativeMetrics, CoordinatorStateEnum::kUnused));

    boost::optional<CoordinatorStateEnum> prevState;
    boost::optional<CoordinatorStateEnum> nextState;

    auto simulateTransitionTo = [&](boost::optional<CoordinatorStateEnum> newState) {
        prevState = nextState;
        nextState = newState;
        _reshardingCumulativeMetrics->onStateTransition(prevState, nextState);
        return checkCoordinateStateField(_reshardingCumulativeMetrics, nextState);
    };

    ASSERT(simulateTransitionTo(CoordinatorStateEnum::kUnused));
    ASSERT(simulateTransitionTo(CoordinatorStateEnum::kInitializing));
    ASSERT(simulateTransitionTo(CoordinatorStateEnum::kPreparingToDonate));
    ASSERT(simulateTransitionTo(boost::none));
}

TEST_F(ReshardingCumulativeMetricsTest, SimulatedNormalDonorStateTransitionReportsStateCorrectly) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock donor{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kDonor};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&donor);

    ASSERT(checkDonorStateField(_reshardingCumulativeMetrics, DonorStateEnum::kUnused));

    boost::optional<DonorStateEnum> prevState;
    boost::optional<DonorStateEnum> nextState;

    auto simulateTransitionTo = [&](boost::optional<DonorStateEnum> newState) {
        prevState = nextState;
        nextState = newState;
        _reshardingCumulativeMetrics->onStateTransition(prevState, nextState);
        return checkDonorStateField(_reshardingCumulativeMetrics, nextState);
    };

    ASSERT(simulateTransitionTo(DonorStateEnum::kUnused));
    ASSERT(simulateTransitionTo(DonorStateEnum::kPreparingToDonate));
    ASSERT(simulateTransitionTo(DonorStateEnum::kDonatingInitialData));
    ASSERT(simulateTransitionTo(DonorStateEnum::kDonatingOplogEntries));
    ASSERT(simulateTransitionTo(DonorStateEnum::kPreparingToBlockWrites));
    ASSERT(simulateTransitionTo(DonorStateEnum::kBlockingWrites));
    ASSERT(simulateTransitionTo(DonorStateEnum::kDone));
    ASSERT(simulateTransitionTo(boost::none));
}

TEST_F(ReshardingCumulativeMetricsTest, SimulatedAbortedDonorStateTransitionReportsStateCorrectly) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock donor{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kDonor};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&donor);

    ASSERT(checkDonorStateField(_reshardingCumulativeMetrics, DonorStateEnum::kUnused));

    boost::optional<DonorStateEnum> prevState;
    boost::optional<DonorStateEnum> nextState;

    auto simulateTransitionTo = [&](boost::optional<DonorStateEnum> newState) {
        prevState = nextState;
        nextState = newState;
        _reshardingCumulativeMetrics->onStateTransition(prevState, nextState);
        return checkDonorStateField(_reshardingCumulativeMetrics, nextState);
    };

    ASSERT(simulateTransitionTo(DonorStateEnum::kUnused));
    ASSERT(simulateTransitionTo(DonorStateEnum::kPreparingToDonate));
    ASSERT(simulateTransitionTo(DonorStateEnum::kError));
    ASSERT(simulateTransitionTo(DonorStateEnum::kDone));
    ASSERT(simulateTransitionTo(boost::none));
}

TEST_F(ReshardingCumulativeMetricsTest,
       SimulatedSteppedDownDonorStateFromUnusedReportsStateCorrectly) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock donor{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kDonor};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&donor);

    boost::optional<DonorStateEnum> initState = DonorStateEnum::kUnused;
    ASSERT(checkDonorStateField(_reshardingCumulativeMetrics, initState));

    _reshardingCumulativeMetrics->onStateTransition(initState, {boost::none});
    ASSERT(checkDonorStateField(_reshardingCumulativeMetrics, initState));
}

TEST_F(ReshardingCumulativeMetricsTest,
       SimulatedSteppedDownDonorStateTransitionReportsStateCorrectly) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock donor{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kDonor};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&donor);

    ASSERT(checkDonorStateField(_reshardingCumulativeMetrics, DonorStateEnum::kUnused));

    boost::optional<DonorStateEnum> prevState;
    boost::optional<DonorStateEnum> nextState;

    auto simulateTransitionTo = [&](boost::optional<DonorStateEnum> newState) {
        prevState = nextState;
        nextState = newState;
        _reshardingCumulativeMetrics->onStateTransition(prevState, nextState);
        return checkDonorStateField(_reshardingCumulativeMetrics, nextState);
    };

    ASSERT(simulateTransitionTo(DonorStateEnum::kUnused));
    ASSERT(simulateTransitionTo(DonorStateEnum::kPreparingToDonate));
    ASSERT(simulateTransitionTo(DonorStateEnum::kDonatingInitialData));
    ASSERT(simulateTransitionTo(boost::none));
}

TEST_F(ReshardingCumulativeMetricsTest,
       SimulatedNormalRecipientStateTransitionReportsStateCorrectly) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kRecipient};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&recipient);

    ASSERT(checkRecipientStateField(_reshardingCumulativeMetrics, RecipientStateEnum::kUnused));

    boost::optional<RecipientStateEnum> prevState;
    boost::optional<RecipientStateEnum> nextState;

    auto simulateTransitionTo = [&](boost::optional<RecipientStateEnum> newState) {
        prevState = nextState;
        nextState = newState;
        _reshardingCumulativeMetrics->onStateTransition(prevState, nextState);
        return checkRecipientStateField(_reshardingCumulativeMetrics, nextState);
    };

    ASSERT(simulateTransitionTo(RecipientStateEnum::kUnused));
    ASSERT(simulateTransitionTo(RecipientStateEnum::kAwaitingFetchTimestamp));
    ASSERT(simulateTransitionTo(RecipientStateEnum::kCreatingCollection));
    ASSERT(simulateTransitionTo(RecipientStateEnum::kCloning));
    ASSERT(simulateTransitionTo(RecipientStateEnum::kApplying));
    ASSERT(simulateTransitionTo(RecipientStateEnum::kStrictConsistency));
    ASSERT(simulateTransitionTo(RecipientStateEnum::kDone));
    ASSERT(simulateTransitionTo(boost::none));
}

TEST_F(ReshardingCumulativeMetricsTest,
       SimulatedAbortedRecipientStateTransitionReportsStateCorrectly) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kRecipient};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&recipient);

    ASSERT(checkRecipientStateField(_reshardingCumulativeMetrics, RecipientStateEnum::kUnused));

    boost::optional<RecipientStateEnum> prevState;
    boost::optional<RecipientStateEnum> nextState;

    auto simulateTransitionTo = [&](boost::optional<RecipientStateEnum> newState) {
        prevState = nextState;
        nextState = newState;
        _reshardingCumulativeMetrics->onStateTransition(prevState, nextState);
        return checkRecipientStateField(_reshardingCumulativeMetrics, nextState);
    };

    ASSERT(simulateTransitionTo(RecipientStateEnum::kUnused));
    ASSERT(simulateTransitionTo(RecipientStateEnum::kAwaitingFetchTimestamp));
    ASSERT(simulateTransitionTo(RecipientStateEnum::kError));
    ASSERT(simulateTransitionTo(boost::none));
}

TEST_F(ReshardingCumulativeMetricsTest,
       SimulatedSteppedDownRecipientStateFromUnusedReportsStateCorrectly) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kRecipient};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&recipient);

    boost::optional<RecipientStateEnum> initState = RecipientStateEnum::kUnused;
    ASSERT(checkRecipientStateField(_reshardingCumulativeMetrics, initState));

    _reshardingCumulativeMetrics->onStateTransition(initState, {boost::none});
    ASSERT(checkRecipientStateField(_reshardingCumulativeMetrics, initState));
}

TEST_F(ReshardingCumulativeMetricsTest,
       SimulatedSteppedDownRecipientStateTransitionReportsStateCorrectly) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kRecipient};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&recipient);

    ASSERT(checkRecipientStateField(_reshardingCumulativeMetrics, RecipientStateEnum::kUnused));

    boost::optional<RecipientStateEnum> prevState;
    boost::optional<RecipientStateEnum> nextState;

    auto simulateTransitionTo = [&](boost::optional<RecipientStateEnum> newState) {
        prevState = nextState;
        nextState = newState;
        _reshardingCumulativeMetrics->onStateTransition(prevState, nextState);
        return checkRecipientStateField(_reshardingCumulativeMetrics, nextState);
    };

    ASSERT(simulateTransitionTo(RecipientStateEnum::kUnused));
    ASSERT(simulateTransitionTo(RecipientStateEnum::kAwaitingFetchTimestamp));
    ASSERT(simulateTransitionTo(RecipientStateEnum::kCreatingCollection));
    ASSERT(simulateTransitionTo(boost::none));
}

TEST_F(ReshardingCumulativeMetricsTest, ReportContainsRunCount) {
    RAIIServerParameterControllerForTest controller("featureFlagReshardingImprovements", true);
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock coordinator{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kCoordinator};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&coordinator);

    {
        BSONObjBuilder bob;
        _reshardingCumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countStarted"), 0);
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countSameKeyStarted"), 0);
    }

    _reshardingCumulativeMetrics->onStarted(false /*isSameKeyResharding*/);
    _reshardingCumulativeMetrics->onStarted(true /*isSameKeyResharding*/);

    {
        BSONObjBuilder bob;
        _reshardingCumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countStarted"), 2);
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countSameKeyStarted"), 1);
    }
}


TEST_F(ReshardingCumulativeMetricsTest, ReportContainsSucceededCount) {
    RAIIServerParameterControllerForTest controller("featureFlagReshardingImprovements", true);
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock coordinator{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kCoordinator};
    auto ignore = _cumulativeMetrics->registerInstanceMetrics(&coordinator);

    {
        BSONObjBuilder bob;
        _reshardingCumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countSucceeded"), 0);
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countSameKeySucceeded"), 0);
    }

    _reshardingCumulativeMetrics->onSuccess(false /*isSameKeyResharding*/);
    _reshardingCumulativeMetrics->onSuccess(true /*isSameKeyResharding*/);

    {
        BSONObjBuilder bob;
        _reshardingCumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countSucceeded"), 2);
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countSameKeySucceeded"), 1);
    }
}

TEST_F(ReshardingCumulativeMetricsTest, ReportContainsFailedCount) {
    RAIIServerParameterControllerForTest controller("featureFlagReshardingImprovements", true);
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock coordinator{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kCoordinator};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&coordinator);

    {
        BSONObjBuilder bob;
        _reshardingCumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countFailed"), 0);
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countSameKeyFailed"), 0);
    }

    _reshardingCumulativeMetrics->onFailure(false /*isSameKeyResharding*/);
    _reshardingCumulativeMetrics->onFailure(true /*isSameKeyResharding*/);

    {
        BSONObjBuilder bob;
        _reshardingCumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countFailed"), 2);
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countSameKeyFailed"), 1);
    }
}

TEST_F(ReshardingCumulativeMetricsTest, ReportContainsCanceledCount) {
    RAIIServerParameterControllerForTest controller("featureFlagReshardingImprovements", true);
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock coordinator{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kCoordinator};
    auto ignore = _reshardingCumulativeMetrics->registerInstanceMetrics(&coordinator);

    {
        BSONObjBuilder bob;
        _reshardingCumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countCanceled"), 0);
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countSameKeyCanceled"), 0);
    }

    _reshardingCumulativeMetrics->onCanceled(false /*isSameKeyResharding*/);
    _reshardingCumulativeMetrics->onCanceled(true /*isSameKeyResharding*/);

    {
        BSONObjBuilder bob;
        _reshardingCumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countCanceled"), 2);
        ASSERT_EQ(report.getObjectField(kResharding).getIntField("countSameKeyCanceled"), 1);
    }
}
}  // namespace
}  // namespace mongo
