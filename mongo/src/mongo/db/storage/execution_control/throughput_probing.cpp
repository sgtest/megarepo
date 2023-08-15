/**
 *    Copyright (C) 2023-present MongoDB, Inc.
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

#include "mongo/db/storage/execution_control/throughput_probing.h"

#include <algorithm>
#include <boost/preprocessor/control/iif.hpp>
#include <cmath>
#include <utility>

#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/db/storage/execution_control/throughput_probing_gen.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/util/assert_util_core.h"
#include "mongo/util/processinfo.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kStorage

namespace mongo::execution_control {
namespace throughput_probing {

Status validateInitialConcurrency(int32_t concurrency, const boost::optional<TenantId>&) {
    if (concurrency < gMinConcurrency) {
        return {ErrorCodes::BadValue,
                "Throughput probing initial concurrency cannot be less than minimum concurrency"};
    }

    if (concurrency > gMaxConcurrency.load()) {
        return {
            ErrorCodes::BadValue,
            "Throughput probing initial concurrency cannot be greater than maximum concurrency"};
    }

    return Status::OK();
}

Status validateMinConcurrency(int32_t concurrency, const boost::optional<TenantId>&) {
    if (concurrency < 1) {
        return {ErrorCodes::BadValue,
                "Throughput probing minimum concurrency cannot be less than 1"};
    }

    if (concurrency > gMaxConcurrency.load()) {
        return {
            ErrorCodes::BadValue,
            "Throughput probing minimum concurrency cannot be greater than maximum concurrency"};
    }

    return Status::OK();
}

Status validateMaxConcurrency(int32_t concurrency, const boost::optional<TenantId>&) {
    if (concurrency < gMinConcurrency) {
        return {ErrorCodes::BadValue,
                "Throughput probing maximum concurrency cannot be less than minimum concurrency"};
    }

    return Status::OK();
}

}  // namespace throughput_probing

using namespace throughput_probing;

ThroughputProbing::ThroughputProbing(ServiceContext* svcCtx,
                                     TicketHolder* readTicketHolder,
                                     TicketHolder* writeTicketHolder,
                                     Milliseconds interval)
    : TicketHolderMonitor(svcCtx, readTicketHolder, writeTicketHolder, interval),
      _stableConcurrency(gInitialConcurrency
                             ? gInitialConcurrency
                             : std::clamp(static_cast<int32_t>(ProcessInfo::getNumCores() * 2),
                                          gMinConcurrency * 2,
                                          gMaxConcurrency.load() * 2)),
      _timer(svcCtx->getTickSource()) {
    _resetConcurrency();
}

void ThroughputProbing::appendStats(BSONObjBuilder& builder) const {
    _stats.serialize(builder);
}

void ThroughputProbing::_run(Client* client) {
    auto numFinishedProcessing =
        _readTicketHolder->numFinishedProcessing() + _writeTicketHolder->numFinishedProcessing();
    invariant(numFinishedProcessing >= _prevNumFinishedProcessing);

    // Initialize on first iteration.
    if (_prevNumFinishedProcessing < 0) {
        _prevNumFinishedProcessing = numFinishedProcessing;
        _timer.reset();
        return;
    }

    Microseconds elapsed = _timer.elapsed();
    if (elapsed == Microseconds{0}) {
        // The clock used to sleep between iterations may not be reliable, and thus the timer may
        // report that no time has elapsed. If this occurs, just wait for the next iteration.
        return;
    }

    auto throughput =
        (numFinishedProcessing - _prevNumFinishedProcessing) / static_cast<double>(elapsed.count());

    switch (_state) {
        case ProbingState::kStable:
            _probeStable(throughput);
            break;
        case ProbingState::kUp:
            _probeUp(throughput);
            break;
        case ProbingState::kDown:
            _probeDown(throughput);
            break;
    }

    // Reset these with fresh values after we've made our adjustment to establish a better
    // cause-effect relationship.
    _prevNumFinishedProcessing =
        _readTicketHolder->numFinishedProcessing() + _writeTicketHolder->numFinishedProcessing();
    _timer.reset();
}

namespace {
// Computes the moving average by weighing 'newValue' with the provided 'weight'.
double expMovingAverage(double average, double newValue, double weight) {
    return (newValue * weight) + (average * (1 - weight));
}

std::pair<int32_t, int32_t> newReadWriteConcurrencies(double stableConcurrency, double step) {
    auto readPct = gReadWriteRatio.load();
    auto writePct = 1 - readPct;

    auto min = gMinConcurrency;
    auto max = gMaxConcurrency.load();

    auto clamp = [&](double pct) {
        return std::clamp(
            static_cast<int32_t>(std::round(stableConcurrency * pct * step)), min, max);
    };

    return {clamp(readPct), clamp(writePct)};
}
}  // namespace

void ThroughputProbing::_probeStable(double throughput) {
    invariant(_state == ProbingState::kStable);

    LOGV2_DEBUG(7346000, 3, "Throughput Probing: stable", "throughput"_attr = throughput);

    // Record the baseline reading.
    _stableThroughput = throughput;

    auto readTotal = _readTicketHolder->outof();
    auto writeTotal = _writeTicketHolder->outof();
    auto readPeak = _readTicketHolder->getAndResetPeakUsed();
    auto writePeak = _writeTicketHolder->getAndResetPeakUsed();

    if ((readTotal < gMaxConcurrency.load() && readPeak >= readTotal) ||
        (writeTotal < gMaxConcurrency.load() && writePeak >= writeTotal)) {
        // At least one of the ticket pools is exhausted, so try increasing concurrency.
        _state = ProbingState::kUp;
        _increaseConcurrency();
    } else if (readPeak > gMinConcurrency || writePeak > gMinConcurrency) {
        // Neither of the ticket pools are exhausted, so try decreasing concurrency to just below
        // the current level of usage.
        _state = ProbingState::kDown;
        _decreaseConcurrency();
    }
}

void ThroughputProbing::_probeUp(double throughput) {
    invariant(_state == ProbingState::kUp);

    LOGV2_DEBUG(7346001, 3, "Throughput Probing: up", "throughput"_attr = throughput);

    if (throughput > _stableThroughput) {
        // Increasing concurrency caused throughput to increase, so use this information to adjust
        // our stable concurrency. We don't want to leave this at the current level. Instead, we use
        // this to update the moving average to avoid over-correcting on recent measurements.
        auto concurrency = _readTicketHolder->outof() + _writeTicketHolder->outof();
        auto newConcurrency = expMovingAverage(
            _stableConcurrency, concurrency, gConcurrencyMovingAverageWeight.load());
        auto oldStableConcurrency = _stableConcurrency;

        _state = ProbingState::kStable;
        _stableThroughput = throughput;
        _stableConcurrency = newConcurrency;
        _resetConcurrency();

        _stats.timesIncreased.fetchAndAdd(1);
        _stats.totalAmountIncreased.fetchAndAdd(_readTicketHolder->outof() +
                                                _writeTicketHolder->outof() - oldStableConcurrency);
    } else {
        // Increasing concurrency did not cause throughput to increase, so go back to stable and get
        // a new baseline to compare against.
        _state = ProbingState::kStable;
        _resetConcurrency();
    }
}

void ThroughputProbing::_probeDown(double throughput) {
    invariant(_state == ProbingState::kDown);

    LOGV2_DEBUG(7346002, 3, "Throughput Probing: down", "throughput"_attr = throughput);

    if (throughput > _stableThroughput) {
        // Decreasing concurrency caused throughput to increase, so use this information to adjust
        // our stable concurrency. We don't want to leave this at the current level. Instead, we use
        // this to update the moving average to avoid over-correcting on recent measurements.
        auto concurrency = _readTicketHolder->outof() + _writeTicketHolder->outof();
        auto newConcurrency = expMovingAverage(
            _stableConcurrency, concurrency, gConcurrencyMovingAverageWeight.load());
        auto oldStableConcurrency = _stableConcurrency;

        _state = ProbingState::kStable;
        _stableThroughput = throughput;
        _stableConcurrency = newConcurrency;
        _resetConcurrency();

        _stats.timesIncreased.fetchAndAdd(1);
        _stats.totalAmountIncreased.fetchAndAdd(oldStableConcurrency - _readTicketHolder->outof() -
                                                _writeTicketHolder->outof());
    } else {
        // Decreasing concurrency did not cause throughput to increase, so go back to stable and get
        // a new baseline to compare against.
        _state = ProbingState::kStable;
        _resetConcurrency();
    }
}

void ThroughputProbing::_resetConcurrency() {
    auto [newReadConcurrency, newWriteConcurrency] =
        newReadWriteConcurrencies(_stableConcurrency, 1);

    _readTicketHolder->resize(newReadConcurrency);
    _writeTicketHolder->resize(newWriteConcurrency);

    LOGV2_DEBUG(7796900,
                3,
                "Throughput Probing: reset concurrency to stable",
                "readConcurrency"_attr = newReadConcurrency,
                "writeConcurrency"_attr = newWriteConcurrency);
}

void ThroughputProbing::_increaseConcurrency() {
    auto [newReadConcurrency, newWriteConcurrency] =
        newReadWriteConcurrencies(_stableConcurrency, 1 + gStepMultiple.load());

    if (newReadConcurrency == _readTicketHolder->outof()) {
        ++newReadConcurrency;
    }
    if (newWriteConcurrency == _writeTicketHolder->outof()) {
        ++newWriteConcurrency;
    }

    _readTicketHolder->resize(newReadConcurrency);
    _writeTicketHolder->resize(newWriteConcurrency);

    LOGV2_DEBUG(7796901,
                3,
                "Throughput Probing: increasing concurrency",
                "readConcurrency"_attr = newReadConcurrency,
                "writeConcurrency"_attr = newWriteConcurrency);
}

void ThroughputProbing::_decreaseConcurrency() {
    auto [newReadConcurrency, newWriteConcurrency] =
        newReadWriteConcurrencies(_stableConcurrency, 1 - gStepMultiple.load());

    if (newReadConcurrency == _readTicketHolder->outof()) {
        --newReadConcurrency;
    }
    if (newWriteConcurrency == _writeTicketHolder->outof()) {
        --newWriteConcurrency;
    }

    _readTicketHolder->resize(newReadConcurrency);
    _writeTicketHolder->resize(newWriteConcurrency);

    LOGV2_DEBUG(7796902,
                3,
                "Throughput Probing: decreasing concurrency",
                "readConcurrency"_attr = newReadConcurrency,
                "writeConcurrency"_attr = newWriteConcurrency);
}

void ThroughputProbing::Stats::serialize(BSONObjBuilder& builder) const {
    builder.append("timesDecreased", static_cast<long long>(timesDecreased.load()));
    builder.append("timesIncreased", static_cast<long long>(timesIncreased.load()));
    builder.append("totalAmountDecreased", static_cast<long long>(totalAmountDecreased.load()));
    builder.append("totalAmountIncreased", static_cast<long long>(totalAmountIncreased.load()));
}

}  // namespace mongo::execution_control
