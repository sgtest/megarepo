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

#include "mongo/db/s/metrics/sharding_data_transform_instance_metrics.h"

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <fmt/format.h>
#include <utility>

#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/s/metrics/sharding_data_transform_metrics_observer.h"
#include "mongo/db/server_options.h"
#include "mongo/s/resharding/resharding_feature_flag_gen.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/duration.h"
#include "mongo/util/namespace_string_util.h"

namespace mongo {

namespace {
constexpr auto kNoEstimate = Milliseconds{-1};

boost::optional<Milliseconds> readCoordinatorEstimate(const AtomicWord<Milliseconds>& field) {
    auto estimate = field.load();
    if (estimate == kNoEstimate) {
        return boost::none;
    }
    return estimate;
}

template <typename T>
void appendOptionalMillisecondsFieldAs(BSONObjBuilder& builder,
                                       const StringData& fieldName,
                                       const boost::optional<Milliseconds> value) {
    if (!value) {
        return;
    }
    builder.append(fieldName, durationCount<T>(*value));
}

}  // namespace

ShardingDataTransformInstanceMetrics::ShardingDataTransformInstanceMetrics(
    UUID instanceId,
    BSONObj originalCommand,
    NamespaceString sourceNs,
    Role role,
    Date_t startTime,
    ClockSource* clockSource,
    ShardingDataTransformCumulativeMetrics* cumulativeMetrics,
    FieldNameProviderPtr fieldNames)
    : ShardingDataTransformInstanceMetrics{
          std::move(instanceId),
          std::move(originalCommand),
          std::move(sourceNs),
          role,
          startTime,
          clockSource,
          cumulativeMetrics,
          std::move(fieldNames),
          std::make_unique<ShardingDataTransformMetricsObserver>(this)} {}

ShardingDataTransformInstanceMetrics::ShardingDataTransformInstanceMetrics(
    UUID instanceId,
    BSONObj originalCommand,
    NamespaceString sourceNs,
    Role role,
    Date_t startTime,
    ClockSource* clockSource,
    ShardingDataTransformCumulativeMetrics* cumulativeMetrics,
    FieldNameProviderPtr fieldNames,
    ObserverPtr observer)
    : _instanceId{std::move(instanceId)},
      _originalCommand{std::move(originalCommand)},
      _sourceNs{std::move(sourceNs)},
      _role{role},
      _fieldNames{std::move(fieldNames)},
      _startTime{startTime},
      _clockSource{clockSource},
      _observer{std::move(observer)},
      _cumulativeMetrics{cumulativeMetrics},
      _approxDocumentsToProcess{0},
      _documentsProcessed{0},
      _approxBytesToScan{0},
      _bytesWritten{0},
      _coordinatorHighEstimateRemainingTimeMillis{kNoEstimate},
      _coordinatorLowEstimateRemainingTimeMillis{kNoEstimate},
      _writesDuringCriticalSection{0} {}

boost::optional<Milliseconds>
ShardingDataTransformInstanceMetrics::getHighEstimateRemainingTimeMillis() const {
    switch (_role) {
        case Role::kRecipient:
            return getRecipientHighEstimateRemainingTimeMillis();
        case Role::kCoordinator:
            return readCoordinatorEstimate(_coordinatorHighEstimateRemainingTimeMillis);
        case Role::kDonor:
            break;
    }
    MONGO_UNREACHABLE;
}

boost::optional<Milliseconds>
ShardingDataTransformInstanceMetrics::getLowEstimateRemainingTimeMillis() const {
    switch (_role) {
        case Role::kRecipient:
            return getHighEstimateRemainingTimeMillis();
        case Role::kCoordinator:
            return readCoordinatorEstimate(_coordinatorLowEstimateRemainingTimeMillis);
        case Role::kDonor:
            break;
    }
    MONGO_UNREACHABLE;
}

Date_t ShardingDataTransformInstanceMetrics::getStartTimestamp() const {
    return _startTime;
}

const UUID& ShardingDataTransformInstanceMetrics::getInstanceId() const {
    return _instanceId;
}

ShardingDataTransformInstanceMetrics::Role ShardingDataTransformInstanceMetrics::getRole() const {
    return _role;
}

std::string ShardingDataTransformInstanceMetrics::createOperationDescription() const noexcept {
    return fmt::format("ShardingDataTransformMetrics{}Service {}",
                       ShardingDataTransformMetrics::getRoleName(_role),
                       _instanceId.toString());
}

StringData ShardingDataTransformInstanceMetrics::getStateString() const noexcept {
    return "Unknown";
}

BSONObj ShardingDataTransformInstanceMetrics::reportForCurrentOp() const noexcept {

    BSONObjBuilder builder;
    builder.append(_fieldNames->getForType(), "op");
    builder.append(_fieldNames->getForDescription(), createOperationDescription());
    builder.append(_fieldNames->getForOp(), "command");
    builder.append(_fieldNames->getForNamespace(), NamespaceStringUtil::serialize(_sourceNs));
    builder.append(_fieldNames->getForOriginatingCommand(), _originalCommand);
    builder.append(_fieldNames->getForOpTimeElapsed(), getOperationRunningTimeSecs().count());
    switch (_role) {
        case Role::kCoordinator:
            appendOptionalMillisecondsFieldAs<Seconds>(
                builder,
                _fieldNames->getForAllShardsHighestRemainingOperationTimeEstimatedSecs(),
                getHighEstimateRemainingTimeMillis());
            appendOptionalMillisecondsFieldAs<Seconds>(
                builder,
                _fieldNames->getForAllShardsLowestRemainingOperationTimeEstimatedSecs(),
                getLowEstimateRemainingTimeMillis());
            builder.append(_fieldNames->getForCoordinatorState(), getStateString());
            if (resharding::gFeatureFlagReshardingImprovements.isEnabled(
                    serverGlobalParams.featureCompatibility)) {
                builder.append(_fieldNames->getForIsSameKeyResharding(),
                               _isSameKeyResharding.load());
            }
            break;
        case Role::kDonor:
            builder.append(_fieldNames->getForDonorState(), getStateString());
            builder.append(_fieldNames->getForCountWritesDuringCriticalSection(),
                           _writesDuringCriticalSection.load());
            builder.append(_fieldNames->getForCountReadsDuringCriticalSection(),
                           _readsDuringCriticalSection.load());
            break;
        case Role::kRecipient:
            builder.append(_fieldNames->getForRecipientState(), getStateString());
            appendOptionalMillisecondsFieldAs<Seconds>(
                builder,
                _fieldNames->getForRemainingOpTimeEstimated(),
                getHighEstimateRemainingTimeMillis());
            builder.append(_fieldNames->getForApproxDocumentsToProcess(),
                           _approxDocumentsToProcess.load());
            builder.append(_fieldNames->getForApproxBytesToScan(), _approxBytesToScan.load());
            builder.append(_fieldNames->getForBytesWritten(), _bytesWritten.load());
            builder.append(_fieldNames->getForCountWritesToStashCollections(),
                           _writesToStashCollections.load());
            builder.append(_fieldNames->getForDocumentsProcessed(), _documentsProcessed.load());
            if (resharding::gFeatureFlagReshardingImprovements.isEnabled(
                    serverGlobalParams.featureCompatibility)) {
                builder.append(_fieldNames->getForIndexesToBuild(), _indexesToBuild.load());
                builder.append(_fieldNames->getForIndexesBuilt(), _indexesBuilt.load());
            }
            break;
        default:
            MONGO_UNREACHABLE;
    }

    return builder.obj();
}

void ShardingDataTransformInstanceMetrics::onDocumentsProcessed(int64_t documentCount,
                                                                int64_t totalDocumentsSizeBytes,
                                                                Milliseconds elapsed) {
    _documentsProcessed.addAndFetch(documentCount);
    _bytesWritten.addAndFetch(totalDocumentsSizeBytes);
    _cumulativeMetrics->onInsertsDuringCloning(documentCount, totalDocumentsSizeBytes, elapsed);
}

int64_t ShardingDataTransformInstanceMetrics::getDocumentsProcessedCount() const {
    return _documentsProcessed.load();
}

int64_t ShardingDataTransformInstanceMetrics::getBytesWrittenCount() const {
    return _bytesWritten.load();
}

int64_t ShardingDataTransformInstanceMetrics::getApproxBytesToScanCount() const {
    return _approxBytesToScan.load();
}

void ShardingDataTransformInstanceMetrics::restoreDocumentsProcessed(
    int64_t documentCount, int64_t totalDocumentsSizeBytes) {
    _documentsProcessed.store(documentCount);
    _bytesWritten.store(totalDocumentsSizeBytes);
}

void ShardingDataTransformInstanceMetrics::restoreWritesToStashCollections(
    int64_t writesToStashCollections) {
    _writesToStashCollections.store(writesToStashCollections);
}

void ShardingDataTransformInstanceMetrics::setDocumentsToProcessCounts(
    int64_t documentCount, int64_t totalDocumentsSizeBytes) {
    _approxDocumentsToProcess.store(documentCount);
    _approxBytesToScan.store(totalDocumentsSizeBytes);
}

void ShardingDataTransformInstanceMetrics::setCoordinatorHighEstimateRemainingTimeMillis(
    Milliseconds milliseconds) {
    _coordinatorHighEstimateRemainingTimeMillis.store(milliseconds);
}

void ShardingDataTransformInstanceMetrics::setCoordinatorLowEstimateRemainingTimeMillis(
    Milliseconds milliseconds) {
    _coordinatorLowEstimateRemainingTimeMillis.store(milliseconds);
}

void ShardingDataTransformInstanceMetrics::onWriteDuringCriticalSection() {
    _writesDuringCriticalSection.addAndFetch(1);
    _cumulativeMetrics->onWriteDuringCriticalSection();
}

Seconds ShardingDataTransformInstanceMetrics::getOperationRunningTimeSecs() const {
    return duration_cast<Seconds>(_clockSource->now() - _startTime);
}

void ShardingDataTransformInstanceMetrics::onWriteToStashedCollections() {
    _writesToStashCollections.fetchAndAdd(1);
    _cumulativeMetrics->onWriteToStashedCollections();
}

void ShardingDataTransformInstanceMetrics::onReadDuringCriticalSection() {
    _readsDuringCriticalSection.fetchAndAdd(1);
    _cumulativeMetrics->onReadDuringCriticalSection();
}

void ShardingDataTransformInstanceMetrics::onCloningRemoteBatchRetrieval(Milliseconds elapsed) {
    _cumulativeMetrics->onCloningRemoteBatchRetrieval(elapsed);
}

ShardingDataTransformCumulativeMetrics*
ShardingDataTransformInstanceMetrics::getCumulativeMetrics() {
    return _cumulativeMetrics;
}

ClockSource* ShardingDataTransformInstanceMetrics::getClockSource() const {
    return _clockSource;
}

void ShardingDataTransformInstanceMetrics::onStarted(bool isSameKeyResharding) {
    _cumulativeMetrics->onStarted(isSameKeyResharding);
}

void ShardingDataTransformInstanceMetrics::onSuccess(bool isSameKeyResharding) {
    _cumulativeMetrics->onSuccess(isSameKeyResharding);
}

void ShardingDataTransformInstanceMetrics::onFailure(bool isSameKeyResharding) {
    _cumulativeMetrics->onFailure(isSameKeyResharding);
}

void ShardingDataTransformInstanceMetrics::onCanceled(bool isSameKeyResharding) {
    _cumulativeMetrics->onCanceled(isSameKeyResharding);
}

void ShardingDataTransformInstanceMetrics::setLastOpEndingChunkImbalance(int64_t imbalanceCount) {
    _cumulativeMetrics->setLastOpEndingChunkImbalance(imbalanceCount);
}

void ShardingDataTransformInstanceMetrics::setIsSameKeyResharding(bool isSameKeyResharding) {
    _isSameKeyResharding.store(isSameKeyResharding);
}

void ShardingDataTransformInstanceMetrics::setIndexesToBuild(int64_t numIndexes) {
    _indexesToBuild.store(numIndexes);
}

void ShardingDataTransformInstanceMetrics::setIndexesBuilt(int64_t numIndexes) {
    _indexesBuilt.store(numIndexes);
}

ShardingDataTransformInstanceMetrics::UniqueScopedObserver
ShardingDataTransformInstanceMetrics::registerInstanceMetrics() {
    return _cumulativeMetrics->registerInstanceMetrics(_observer.get());
}

}  // namespace mongo
