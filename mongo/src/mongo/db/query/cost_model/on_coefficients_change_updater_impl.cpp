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

#include "mongo/db/query/cost_model/on_coefficients_change_updater_impl.h"

#include <memory>
#include <string>
#include <utility>

#include "mongo/bson/json.h"
#include "mongo/db/query/query_knobs_gen.h"

namespace mongo::cost_model {

const Decorable<ServiceContext>::Decoration<CostModelManager> costModelManager =
    ServiceContext::declareDecoration<CostModelManager>();

void OnCoefficientsChangeUpdaterImpl::updateCoefficients(ServiceContext* serviceCtx,
                                                         const BSONObj& overrides) {
    costModelManager(serviceCtx).updateCostModelCoefficients(overrides);
}

ServiceContext::ConstructorActionRegisterer costModelUpdaterRegisterer{
    "costModelUpdaterRegisterer", [](ServiceContext* serviceCtx) {
        const std::string costModelCoefficients = internalCostModelCoefficients.get();
        BSONObj overrides =
            costModelCoefficients.empty() ? BSONObj() : fromjson(costModelCoefficients);

        onCoefficientsChangeUpdater(serviceCtx) =
            std::make_unique<OnCoefficientsChangeUpdaterImpl>(serviceCtx, overrides);
    }};

OnCoefficientsChangeUpdaterImpl::OnCoefficientsChangeUpdaterImpl(ServiceContext* serviceCtx,
                                                                 const BSONObj& overrides) {
    updateCoefficients(serviceCtx, overrides);
}
}  // namespace mongo::cost_model
