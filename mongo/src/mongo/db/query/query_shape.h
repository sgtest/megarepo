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

#pragma once

#include <boost/smart_ptr/intrusive_ptr.hpp>
#include <functional>
#include <string>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/crypto/sha256_block.h"
#include "mongo/db/matcher/expression.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/pipeline/aggregate_command_gen.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/pipeline/pipeline.h"
#include "mongo/db/query/find_command.h"
#include "mongo/db/query/parsed_find_command.h"
#include "mongo/db/query/query_request_helper.h"
#include "mongo/db/query/serialization_options.h"

namespace mongo::query_shape {

using QueryShapeHash = SHA256Block;

/**
 * Computes a BSONObj that is meant to be used to classify queries according to their shape, for the
 * purposes of collecting queryStats.
 *
 * For example, if the MatchExpression represents {a: 2}, it will return the same BSONObj as the
 * MatchExpression for {a: 1}, {a: 10}, and {a: {$eq: 2}} (identical bits but not sharing memory)
 * because they are considered to be the same shape.
 *
 * Note that the shape of a MatchExpression is only part of the overall query shape - which should
 * include other options like the sort and projection.
 *
 * TODO better consider how this interacts with persistent query settings project, and document it.
 * TODO (TODO SERVER ticket) better distinguish this from a plan cache or CQ 'query shape'.
 */
BSONObj debugPredicateShape(const MatchExpression* predicate);
BSONObj representativePredicateShape(const MatchExpression* predicate);

BSONObj debugPredicateShape(const MatchExpression* predicate,
                            std::function<std::string(StringData)> transformIdentifiersCallback);
BSONObj representativePredicateShape(
    const MatchExpression* predicate,
    std::function<std::string(StringData)> transformIdentifiersCallback);

BSONObj extractSortShape(const BSONObj& sortSpec,
                         const boost::intrusive_ptr<ExpressionContext>& expCtx,
                         const SerializationOptions& opts);

BSONObj extractQueryShape(const ParsedFindCommand& findRequest,
                          const SerializationOptions& opts,
                          const boost::intrusive_ptr<ExpressionContext>& expCtx);
BSONObj extractQueryShape(const AggregateCommandRequest& aggregateCommand,
                          const Pipeline& pipeline,
                          const SerializationOptions& opts,
                          const boost::intrusive_ptr<ExpressionContext>& expCtx,
                          const NamespaceString& nss);

QueryShapeHash hash(const BSONObj& queryShape);
}  // namespace mongo::query_shape
