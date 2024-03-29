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

#include "mongo/db/query/query_knob_configuration.h"

namespace mongo {

QueryKnobConfiguration::QueryKnobConfiguration(const query_settings::QuerySettings& querySettings) {
    _sbeDisableGroupPushdownValue =
        internalQuerySlotBasedExecutionDisableGroupPushdown.loadRelaxed();
    _sbeDisableLookupPushdownValue =
        internalQuerySlotBasedExecutionDisableLookupPushdown.loadRelaxed();
    _sbeDisableTimeSeriesValue =
        internalQuerySlotBasedExecutionDisableTimeSeriesPushdown.loadRelaxed();

    _queryFrameworkControlValue = querySettings.getQueryFramework().value_or_eval([]() {
        return ServerParameterSet::getNodeParameterSet()
            ->get<QueryFrameworkControl>("internalQueryFrameworkControl")
            ->_data.get();
    });

    _planEvaluationMaxResults = internalQueryPlanEvaluationMaxResults.loadRelaxed();
    _maxScansToExplodeValue = static_cast<size_t>(internalQueryMaxScansToExplode.loadRelaxed());
}

QueryFrameworkControlEnum QueryKnobConfiguration::getInternalQueryFrameworkControlForOp() const {
    return _queryFrameworkControlValue;
}

bool QueryKnobConfiguration::getSbeDisableGroupPushdownForOp() const {
    return _sbeDisableGroupPushdownValue;
}

bool QueryKnobConfiguration::getSbeDisableLookupPushdownForOp() const {
    return _sbeDisableLookupPushdownValue;
}

bool QueryKnobConfiguration::getSbeDisableTimeSeriesForOp() const {
    return _sbeDisableTimeSeriesValue;
}

bool QueryKnobConfiguration::isForceClassicEngineEnabled() const {
    return _queryFrameworkControlValue == QueryFrameworkControlEnum::kForceClassicEngine;
}

size_t QueryKnobConfiguration::getPlanEvaluationMaxResultsForOp() const {
    return _planEvaluationMaxResults;
}

size_t QueryKnobConfiguration::getMaxScansToExplodeForOp() const {
    return _maxScansToExplodeValue;
}

bool QueryKnobConfiguration::canPushDownFullyCompatibleStages() const {
    switch (_queryFrameworkControlValue) {
        case QueryFrameworkControlEnum::kForceClassicEngine:
        case QueryFrameworkControlEnum::kTrySbeRestricted:
        case QueryFrameworkControlEnum::kTryBonsai:
        case QueryFrameworkControlEnum::kTryBonsaiExperimental:
        case QueryFrameworkControlEnum::kForceBonsai:
            return false;
        case QueryFrameworkControlEnum::kTrySbeEngine:
            return true;
    }
    MONGO_UNREACHABLE;
}

}  // namespace mongo
