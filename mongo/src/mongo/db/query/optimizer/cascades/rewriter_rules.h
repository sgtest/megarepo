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

#include "mongo/db/query/optimizer/defs.h"
#include "mongo/db/query/util/named_enum.h"

namespace mongo::optimizer::cascades {

#define LOGICALREWRITER_NAMES(F)                                  \
    F(Root)                                                       \
    /* "Linear" reordering rewrites. */                           \
    F(FilterEvaluationReorder)                                    \
    F(FilterCollationReorder)                                     \
    F(EvaluationCollationReorder)                                 \
    F(EvaluationLimitSkipReorder)                                 \
                                                                  \
    F(FilterGroupByReorder)                                       \
    F(GroupCollationReorder)                                      \
                                                                  \
    F(FilterUnwindReorder)                                        \
    F(EvaluationUnwindReorder)                                    \
    F(UnwindCollationReorder)                                     \
                                                                  \
    F(FilterExchangeReorder)                                      \
    F(ExchangeEvaluationReorder)                                  \
                                                                  \
    F(FilterUnionReorder)                                         \
                                                                  \
    F(SargableFilterReorder)                                      \
    F(SargableEvaluationReorder)                                  \
    F(SargableDisjunctiveReorder)                                 \
                                                                  \
    /* Merging rewrites. */                                       \
    F(CollationMerge)                                             \
    F(LimitSkipMerge)                                             \
    F(SargableMerge)                                              \
                                                                  \
    /* Local-global optimization for GroupBy */                   \
    F(GroupByExplore)                                             \
                                                                  \
    /* Propagate ValueScan nodes*/                                \
    F(FilterValueScanPropagate)                                   \
    F(EvaluationValueScanPropagate)                               \
    F(SargableValueScanPropagate)                                 \
    F(CollationValueScanPropagate)                                \
    F(LimitSkipValueScanPropagate)                                \
    F(ExchangeValueScanPropagate)                                 \
                                                                  \
    F(LimitSkipSubstitute)                                        \
                                                                  \
    /* Convert filter and evaluation nodes into sargable nodes */ \
    F(FilterSubstitute)                                           \
    F(EvaluationSubstitute)                                       \
    F(SargableSplit)                                              \
    F(FilterRIDIntersectReorder)                                  \
    F(EvaluationRIDIntersectReorder)                              \
                                                                  \
    /* Simplify filter node. */                                   \
    F(FilterSimplify)

QUERY_UTIL_NAMED_ENUM_DEFINE(LogicalRewriteType, LOGICALREWRITER_NAMES)
#undef LOGICALREWRITER_NAMES

#define PHYSICALREWRITER_NAMES(F) \
    F(Root)                       \
    F(Uninitialized)              \
    F(EnforceCollation)           \
    F(EnforceLimitSkip)           \
    F(EnforceDistribution)        \
    F(EnforceShardFilter)         \
    F(AttemptCoveringQuery)       \
    F(Seek)                       \
    F(PhysicalScan)               \
    F(ValueScan)                  \
    F(Evaluation)                 \
    F(Union)                      \
    F(LimitSkip)                  \
    F(HashGroup)                  \
    F(Unwind)                     \
    F(Collation)                  \
    F(Exchange)                   \
    F(NLJ)                        \
    F(Filter)                     \
    F(RenameProjection)           \
    F(EvaluationPassthrough)      \
    F(SargableIxScanConvert)      \
    F(SargableToIndex)            \
    F(SargableToPhysicalScan)     \
    F(SargableToSeek)             \
    F(RIDIntersectMergeJoin)      \
    F(RIDIntersectHashJoin)       \
    F(RIDIntersectGroupBy)        \
    F(RIDUnion)                   \
    F(RIDUnionUnique)             \
    F(IndexFetch)

QUERY_UTIL_NAMED_ENUM_DEFINE(PhysicalRewriteType, PHYSICALREWRITER_NAMES)
#undef PHYSICALREWRITER_NAMES

}  // namespace mongo::optimizer::cascades
