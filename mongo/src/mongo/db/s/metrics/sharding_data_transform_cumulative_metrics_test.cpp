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

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/db/s/metrics/sharding_data_transform_cumulative_metrics.h"
#include "mongo/db/s/metrics/sharding_data_transform_metrics_test_fixture.h"
#include "mongo/idl/server_parameter_test_util.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/bson_test_util.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/clock_source.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kTest


namespace mongo {
namespace {

class ScopedObserverMock {
public:
    using Ptr = std::unique_ptr<ScopedObserverMock>;

    ScopedObserverMock(Date_t startTime,
                       int64_t timeRemaining,
                       ClockSource* clockSource,
                       ShardingDataTransformCumulativeMetrics* parent)
        : _mock{startTime, timeRemaining},
          _scopedOpObserver{parent->registerInstanceMetrics(&_mock)} {}

private:
    ObserverMock _mock;
    ShardingDataTransformCumulativeMetrics::UniqueScopedObserver _scopedOpObserver;
};

TEST_F(ShardingDataTransformMetricsTestFixture, AddAndRemoveMetrics) {
    auto deregister = _cumulativeMetrics->registerInstanceMetrics(getOldestObserver());
    ASSERT_EQ(_cumulativeMetrics->getObservedMetricsCount(), 1);
    deregister.reset();
    ASSERT_EQ(_cumulativeMetrics->getObservedMetricsCount(), 0);
}

TEST_F(ShardingDataTransformMetricsTestFixture, MetricsReportsOldestWhenInsertedFirst) {
    auto deregisterOldest = _cumulativeMetrics->registerInstanceMetrics(getOldestObserver());
    auto deregisterYoungest = _cumulativeMetrics->registerInstanceMetrics(getYoungestObserver());
    ASSERT_EQ(_cumulativeMetrics->getOldestOperationHighEstimateRemainingTimeMillis(
                  ObserverMock::kDefaultRole),
              kOldestTimeLeft);
}

TEST_F(ShardingDataTransformMetricsTestFixture, MetricsReportsOldestWhenInsertedLast) {
    auto deregisterYoungest = _cumulativeMetrics->registerInstanceMetrics(getYoungestObserver());
    auto deregisterOldest = _cumulativeMetrics->registerInstanceMetrics(getOldestObserver());
    ASSERT_EQ(_cumulativeMetrics->getOldestOperationHighEstimateRemainingTimeMillis(
                  ObserverMock::kDefaultRole),
              kOldestTimeLeft);
}

TEST_F(ShardingDataTransformMetricsTestFixture, NoServerStatusWhenNeverUsed) {
    BSONObjBuilder bob;
    _cumulativeMetrics->reportForServerStatus(&bob);
    auto report = bob.done();
    ASSERT_BSONOBJ_EQ(report, BSONObj());
}

TEST_F(ShardingDataTransformMetricsTestFixture, RemainingTimeReportsMinusOneWhenEmpty) {
    ASSERT_EQ(_cumulativeMetrics->getObservedMetricsCount(), 0);
    ASSERT_EQ(_cumulativeMetrics->getOldestOperationHighEstimateRemainingTimeMillis(
                  ObserverMock::kDefaultRole),
              -1);
}

TEST_F(ShardingDataTransformMetricsTestFixture, UpdatesOldestWhenOldestIsRemoved) {
    auto deregisterYoungest = _cumulativeMetrics->registerInstanceMetrics(getYoungestObserver());
    auto deregisterOldest = _cumulativeMetrics->registerInstanceMetrics(getOldestObserver());
    ASSERT_EQ(_cumulativeMetrics->getOldestOperationHighEstimateRemainingTimeMillis(
                  ObserverMock::kDefaultRole),
              kOldestTimeLeft);
    deregisterOldest.reset();
    ASSERT_EQ(_cumulativeMetrics->getOldestOperationHighEstimateRemainingTimeMillis(
                  ObserverMock::kDefaultRole),
              kYoungestTimeLeft);
}

TEST_F(ShardingDataTransformMetricsTestFixture, InsertsTwoWithSameStartTime) {
    auto deregisterOldest = _cumulativeMetrics->registerInstanceMetrics(getOldestObserver());
    ObserverMock sameAsOldest{kOldestTime, kOldestTimeLeft};
    auto deregisterOldest2 = _cumulativeMetrics->registerInstanceMetrics(&sameAsOldest);
    ASSERT_EQ(_cumulativeMetrics->getObservedMetricsCount(), 2);
    ASSERT_EQ(_cumulativeMetrics->getOldestOperationHighEstimateRemainingTimeMillis(
                  ObserverMock::kDefaultRole),
              kOldestTimeLeft);
}

TEST_F(ShardingDataTransformMetricsTestFixture, StillReportsOldestAfterRandomOperations) {
    doRandomOperationsTest<ScopedObserverMock>();
}

TEST_F(ShardingDataTransformMetricsTestFixture,
       StillReportsOldestAfterRandomOperationsMultithreaded) {
    doRandomOperationsMultithreadedTest<ScopedObserverMock>();
}

TEST_F(ShardingDataTransformMetricsTestFixture, ReportsOldestByRole) {
    using Role = ShardingDataTransformMetrics::Role;
    auto& metrics = _cumulativeMetrics;
    ObserverMock oldDonor{Date_t::fromMillisSinceEpoch(100), 100, 100, Role::kDonor};
    ObserverMock youngDonor{Date_t::fromMillisSinceEpoch(200), 200, 200, Role::kDonor};
    ObserverMock oldRecipient{Date_t::fromMillisSinceEpoch(300), 300, 300, Role::kRecipient};
    ObserverMock youngRecipient{Date_t::fromMillisSinceEpoch(400), 400, 400, Role::kRecipient};
    auto removeOldD = metrics->registerInstanceMetrics(&oldDonor);
    auto removeYoungD = metrics->registerInstanceMetrics(&youngDonor);
    auto removeOldR = metrics->registerInstanceMetrics(&oldRecipient);
    auto removeYoungR = metrics->registerInstanceMetrics(&youngRecipient);

    ASSERT_EQ(metrics->getObservedMetricsCount(), 4);
    ASSERT_EQ(metrics->getObservedMetricsCount(Role::kDonor), 2);
    ASSERT_EQ(metrics->getObservedMetricsCount(Role::kRecipient), 2);
    ASSERT_EQ(metrics->getOldestOperationHighEstimateRemainingTimeMillis(Role::kDonor), 100);
    ASSERT_EQ(metrics->getOldestOperationHighEstimateRemainingTimeMillis(Role::kRecipient), 300);
    removeOldD.reset();
    ASSERT_EQ(metrics->getObservedMetricsCount(), 3);
    ASSERT_EQ(metrics->getObservedMetricsCount(Role::kDonor), 1);
    ASSERT_EQ(metrics->getOldestOperationHighEstimateRemainingTimeMillis(Role::kDonor), 200);
    removeOldR.reset();
    ASSERT_EQ(metrics->getObservedMetricsCount(), 2);
    ASSERT_EQ(metrics->getObservedMetricsCount(Role::kRecipient), 1);
    ASSERT_EQ(metrics->getOldestOperationHighEstimateRemainingTimeMillis(Role::kRecipient), 400);
}

TEST_F(ShardingDataTransformMetricsTestFixture, ReportContainsTimeEstimates) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(100), 100, 100, Role::kRecipient};
    ObserverMock coordinator{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kCoordinator};
    auto recipientObserver = _cumulativeMetrics->registerInstanceMetrics(&recipient);
    auto coordinatorObserver = _cumulativeMetrics->registerInstanceMetrics(&coordinator);

    BSONObjBuilder bob;
    _cumulativeMetrics->reportForServerStatus(&bob);
    auto report = bob.done();
    auto section = report.getObjectField(kTestMetricsName).getObjectField("oldestActive");
    ASSERT_EQ(section.getIntField("recipientRemainingOperationTimeEstimatedMillis"), 100);
    ASSERT_EQ(
        section.getIntField("coordinatorAllShardsHighestRemainingOperationTimeEstimatedMillis"),
        400);
    ASSERT_EQ(
        section.getIntField("coordinatorAllShardsLowestRemainingOperationTimeEstimatedMillis"),
        300);
}

TEST_F(ShardingDataTransformMetricsTestFixture, ReportContainsRunCount) {
    RAIIServerParameterControllerForTest controller("featureFlagReshardingImprovements", true);
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock coordinator{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kCoordinator};
    auto ignore = _cumulativeMetrics->registerInstanceMetrics(&coordinator);

    {
        BSONObjBuilder bob;
        _cumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countStarted"), 0);
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countSameKeyStarted"), 0);
    }

    _cumulativeMetrics->onStarted(false /*isSameKeyResharding*/);
    _cumulativeMetrics->onStarted(true /*isSameKeyResharding*/);

    {
        BSONObjBuilder bob;
        _cumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countStarted"), 1);
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countSameKeyStarted"), 1);
    }
}

TEST_F(ShardingDataTransformMetricsTestFixture, ReportContainsSucceededCount) {
    RAIIServerParameterControllerForTest controller("featureFlagReshardingImprovements", true);
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock coordinator{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kCoordinator};
    auto ignore = _cumulativeMetrics->registerInstanceMetrics(&coordinator);

    {
        BSONObjBuilder bob;
        _cumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countSucceeded"), 0);
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countSameKeySucceeded"), 0);
    }

    _cumulativeMetrics->onSuccess(false /*isSameKeyResharding*/);
    _cumulativeMetrics->onSuccess(true /*isSameKeyResharding*/);

    {
        BSONObjBuilder bob;
        _cumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countSucceeded"), 1);
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countSameKeySucceeded"), 1);
    }
}

TEST_F(ShardingDataTransformMetricsTestFixture, ReportContainsFailedCount) {
    RAIIServerParameterControllerForTest controller("featureFlagReshardingImprovements", true);
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock coordinator{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kCoordinator};
    auto ignore = _cumulativeMetrics->registerInstanceMetrics(&coordinator);

    {
        BSONObjBuilder bob;
        _cumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countFailed"), 0);
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countSameKeyFailed"), 0);
    }

    _cumulativeMetrics->onFailure(false /*isSameKeyResharding*/);
    _cumulativeMetrics->onFailure(true /*isSameKeyResharding*/);

    {
        BSONObjBuilder bob;
        _cumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countFailed"), 1);
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countSameKeyFailed"), 1);
    }
}

TEST_F(ShardingDataTransformMetricsTestFixture, ReportContainsCanceledCount) {
    RAIIServerParameterControllerForTest controller("featureFlagReshardingImprovements", true);
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock coordinator{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kCoordinator};
    auto ignore = _cumulativeMetrics->registerInstanceMetrics(&coordinator);

    {
        BSONObjBuilder bob;
        _cumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countCanceled"), 0);
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countSameKeyCanceled"), 0);
    }

    _cumulativeMetrics->onCanceled(false /*isSameKeyResharding*/);
    _cumulativeMetrics->onCanceled(true /*isSameKeyResharding*/);

    {
        BSONObjBuilder bob;
        _cumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countCanceled"), 1);
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("countSameKeyCanceled"), 1);
    }
}

TEST_F(ShardingDataTransformMetricsTestFixture, ReportContainsLastChunkImbalanceCount) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock coordinator{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kCoordinator};
    auto ignore = _cumulativeMetrics->registerInstanceMetrics(&coordinator);

    {
        BSONObjBuilder bob;
        _cumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("lastOpEndingChunkImbalance"),
                  0);
    }

    _cumulativeMetrics->setLastOpEndingChunkImbalance(111);

    {
        BSONObjBuilder bob;
        _cumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("lastOpEndingChunkImbalance"),
                  111);
    }

    _cumulativeMetrics->setLastOpEndingChunkImbalance(777);

    {
        BSONObjBuilder bob;
        _cumulativeMetrics->reportForServerStatus(&bob);
        auto report = bob.done();
        ASSERT_EQ(report.getObjectField(kTestMetricsName).getIntField("lastOpEndingChunkImbalance"),
                  777);
    }
}

TEST_F(ShardingDataTransformMetricsTestFixture, ReportContainsInsertsDuringCloning) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(100), 100, 100, Role::kRecipient};
    auto ignore = _cumulativeMetrics->registerInstanceMetrics(&recipient);

    auto latencies = getCumulativeMetricsReportForSection(kLatencies);
    ASSERT_EQ(latencies.getIntField("collectionCloningTotalLocalInserts"), 0);
    ASSERT_EQ(latencies.getIntField("collectionCloningTotalLocalInsertTimeMillis"), 0);

    auto active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("documentsProcessed"), 0);
    ASSERT_EQ(active.getIntField("bytesWritten"), 0);

    _cumulativeMetrics->onInsertsDuringCloning(140, 20763, Milliseconds(15));

    latencies = getCumulativeMetricsReportForSection(kLatencies);
    ASSERT_EQ(latencies.getIntField("collectionCloningTotalLocalInserts"), 1);
    ASSERT_EQ(latencies.getIntField("collectionCloningTotalLocalInsertTimeMillis"), 15);

    active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("documentsProcessed"), 140);
    ASSERT_EQ(active.getIntField("bytesWritten"), 20763);
}

TEST_F(ShardingDataTransformMetricsTestFixture, ReportContainsReadDuringCriticalSection) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock donor{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kDonor};
    auto ignore = _cumulativeMetrics->registerInstanceMetrics(&donor);

    auto active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("countReadsDuringCriticalSection"), 0);

    _cumulativeMetrics->onReadDuringCriticalSection();

    active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("countReadsDuringCriticalSection"), 1);
}

TEST_F(ShardingDataTransformMetricsTestFixture, ReportContainsWriteDuringCriticalSection) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock donor{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kDonor};
    auto ignore = _cumulativeMetrics->registerInstanceMetrics(&donor);

    auto active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("countWritesDuringCriticalSection"), 0);

    _cumulativeMetrics->onWriteDuringCriticalSection();

    active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("countWritesDuringCriticalSection"), 1);
}

TEST_F(ShardingDataTransformMetricsTestFixture, ReportContainsWriteToStashedCollection) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(200), 400, 300, Role::kRecipient};
    auto ignore = _cumulativeMetrics->registerInstanceMetrics(&recipient);

    auto active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("countWritesToStashCollections"), 0);

    _cumulativeMetrics->onWriteToStashedCollections();

    active = getCumulativeMetricsReportForSection(kActive);
    ASSERT_EQ(active.getIntField("countWritesToStashCollections"), 1);
}

TEST_F(ShardingDataTransformMetricsTestFixture, ReportContainsBatchRetrievedDuringCloning) {
    using Role = ShardingDataTransformMetrics::Role;
    ObserverMock recipient{Date_t::fromMillisSinceEpoch(100), 100, 100, Role::kRecipient};
    auto ignore = _cumulativeMetrics->registerInstanceMetrics(&recipient);

    auto latencies = getCumulativeMetricsReportForSection(kLatencies);
    ASSERT_EQ(latencies.getIntField("collectionCloningTotalRemoteBatchesRetrieved"), 0);
    ASSERT_EQ(latencies.getIntField("collectionCloningTotalRemoteBatchRetrievalTimeMillis"), 0);

    _cumulativeMetrics->onCloningRemoteBatchRetrieval(Milliseconds(19));

    latencies = getCumulativeMetricsReportForSection(kLatencies);
    ASSERT_EQ(latencies.getIntField("collectionCloningTotalRemoteBatchesRetrieved"), 1);
    ASSERT_EQ(latencies.getIntField("collectionCloningTotalRemoteBatchRetrievalTimeMillis"), 19);
}

}  // namespace
}  // namespace mongo
