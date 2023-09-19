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

#pragma once

#include <boost/optional/optional.hpp>
#include <boost/preprocessor/arithmetic/limits/dec_256.hpp>
#include <boost/preprocessor/control/expr_iif.hpp>
#include <boost/preprocessor/control/iif.hpp>
// IWYU pragma: no_include "boost/preprocessor/detail/limits/auto_rec_256.hpp"
#include <boost/preprocessor/logical/limits/bool_256.hpp>
// IWYU pragma: no_include "boost/preprocessor/repetition/detail/limits/for_256.hpp"
#include <boost/preprocessor/repetition/for.hpp>
#include <boost/preprocessor/seq/limits/elem_256.hpp>
#include <boost/preprocessor/seq/limits/size_256.hpp>
#include <boost/preprocessor/tuple/elem.hpp>
#include <boost/preprocessor/tuple/limits/to_seq_64.hpp>
#include <boost/preprocessor/tuple/to_seq.hpp>
#include <boost/preprocessor/variadic/limits/elem_64.hpp>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/s/metrics/cumulative_metrics_state_holder.h"
#include "mongo/db/s/metrics/sharding_data_transform_cumulative_metrics.h"
#include "mongo/db/s/metrics/sharding_data_transform_metrics_macros.h"
#include "mongo/db/s/metrics/with_oplog_application_count_metrics.h"
#include "mongo/db/s/metrics/with_oplog_application_latency_metrics.h"
#include "mongo/db/s/metrics/with_state_management_for_cumulative_metrics.h"
#include "mongo/db/s/resharding/resharding_cumulative_metrics_field_name_provider.h"
#include "mongo/s/resharding/common_types_gen.h"

namespace mongo {

namespace resharding_cumulative_metrics {
DEFINE_IDL_ENUM_SIZE_TEMPLATE_HELPER(ReshardingMetrics,
                                     CoordinatorStateEnum,
                                     DonorStateEnum,
                                     RecipientStateEnum)
using Base = WithOplogApplicationLatencyMetrics<WithOplogApplicationCountMetrics<
    WithStateManagementForCumulativeMetrics<ShardingDataTransformCumulativeMetrics,
                                            ReshardingMetricsEnumSizeTemplateHelper,
                                            CoordinatorStateEnum,
                                            DonorStateEnum,
                                            RecipientStateEnum>>>;
}  // namespace resharding_cumulative_metrics

class ReshardingCumulativeMetrics : public resharding_cumulative_metrics::Base {
public:
    using Base = resharding_cumulative_metrics::Base;

    ReshardingCumulativeMetrics();
    ReshardingCumulativeMetrics(const std::string& rootName);

    static boost::optional<StringData> fieldNameFor(AnyState state);
    void reportForServerStatus(BSONObjBuilder* bob) const override;

    void onStarted(bool isSameKeyResharding);
    void onSuccess(bool isSameKeyResharding);
    void onFailure(bool isSameKeyResharding);
    void onCanceled(bool isSameKeyResharding);

private:
    virtual void reportActive(BSONObjBuilder* bob) const;
    virtual void reportLatencies(BSONObjBuilder* bob) const;
    virtual void reportCurrentInSteps(BSONObjBuilder* bob) const;

    const ReshardingCumulativeMetricsFieldNameProvider* _fieldNames;

    AtomicWord<int64_t> _countSameKeyStarted{0};
    AtomicWord<int64_t> _countSameKeySucceeded{0};
    AtomicWord<int64_t> _countSameKeyFailed{0};
    AtomicWord<int64_t> _countSameKeyCancelled{0};
};

}  // namespace mongo
