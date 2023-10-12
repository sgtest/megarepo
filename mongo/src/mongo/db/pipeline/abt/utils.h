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
#include <cstddef>
#include <functional>
#include <utility>

#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/exec/sbe/values/value.h"
#include "mongo/db/field_ref.h"
#include "mongo/db/pipeline/field_path.h"
#include "mongo/db/query/optimizer/defs.h"
#include "mongo/db/query/optimizer/index_bounds.h"
#include "mongo/db/query/optimizer/node.h"  // IWYU pragma: keep
#include "mongo/db/query/optimizer/syntax/syntax.h"


namespace mongo::optimizer {

using ABTFieldNameFn =
    std::function<ABT(FieldNameType fieldName, const bool isLastElement, ABT input)>;

/**
 * Translates an aggregation FieldPath by invoking the `fieldNameFn` for each path component.
 */
ABT translateFieldPath(const FieldPath& fieldPath,
                       ABT initial,
                       const ABTFieldNameFn& fieldNameFn,
                       size_t skipFromStart = 0);

/**
 * Translates a given FieldRef (typically used in a MatchExpression) with 'initial' as the input
 * ABT.
 */
ABT translateFieldRef(const FieldRef& fieldRef, ABT initial);

/**
 * Translates a given field of a shard key into an ABT. The resulting paths will not contain
 * 'PathTraverse' elements because shard keys are guarenteed to be non-multikey.
 */
ABT translateShardKeyField(std::string shardKey);

/**
 * Return the minimum or maximum value for the "class" of values represented by the input
 * constant. Used to support type bracketing. Note that we only take a type tag as input; this means
 * that bracketing for special _values_ such as NaN is not handled here. Not to be confused with
 * getMinMaxBoundForValue().
 * Return format is <min/max value, bool inclusive>
 */
std::pair<boost::optional<ABT>, bool> getMinMaxBoundForType(bool isMin,
                                                            const sbe::value::TypeTags& tag);

/**
 * Used by the optimizer to optionally convert path elements (e.g. PathArr) directly into intervals.
 */
boost::optional<IntervalReqExpr::Node> defaultConvertPathToInterval(const ABT& node);

}  // namespace mongo::optimizer
