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

#include <boost/optional/optional.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>
#include <memory>
#include <utility>

#include "mongo/base/status_with.h"
#include "mongo/db/matcher/expression.h"
#include "mongo/db/matcher/expression_parser.h"
#include "mongo/db/matcher/extensions_callback.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/query/collation/collator_interface.h"
#include "mongo/db/query/distinct_command_gen.h"

namespace mongo {

/**
 * Represents a distinct command request, but with more fully parsed ASTs for some fields which are
 * still raw BSONObj on the DistinctCommandRequest type.
 */
struct ParsedDistinctCommand {
    std::unique_ptr<CollatorInterface> collator;
    std::unique_ptr<MatchExpression> query;

    // The IDL parser does not handle generic command arguments thus you can't get them from
    // DistinctCommandRequest. Since the canonical distinct requires the following options, manually
    // parse and keep them beside distinctCommandRequest.
    boost::optional<int> maxTimeMS;
    boost::optional<BSONObj> queryOptions;
    boost::optional<BSONObj> readConcern;

    // All other parameters to the find command which do not have AST-like types and can be
    // appropriately tracked as raw value types like ints. The fields above like 'query' are all
    // still present in their raw form on this DistinctCommandRequest, but it is not expected that
    // they will be useful other than to keep the original BSON values around in-memory to avoid
    // copying large strings and such.
    std::unique_ptr<DistinctCommandRequest> distinctCommandRequest;
};

namespace parsed_distinct_command {
/**
 * Parses each big component of the input 'distinctCommand'.
 *
 * 'extensionsCallback' allows for additional mongod parsing. If called from mongos, an
 * ExtensionsCallbackNoop object should be passed to skip this parsing.
 */
std::unique_ptr<ParsedDistinctCommand> parse(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    const BSONObj& cmd,
    std::unique_ptr<DistinctCommandRequest> distinctCommand,
    const ExtensionsCallback& extensionsCallback,
    MatchExpressionParser::AllowedFeatureSet allowedFeatures);

}  // namespace parsed_distinct_command
}  // namespace mongo
