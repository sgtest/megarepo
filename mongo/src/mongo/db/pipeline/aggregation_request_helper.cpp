/**
 *    Copyright (C) 2020-present MongoDB, Inc.
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

#include "mongo/db/pipeline/aggregation_request_helper.h"

#include <boost/cstdint.hpp>
#include <boost/move/utility_core.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <cstdint>
#include <memory>
#include <string>

#include <boost/none.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/simple_bsonobj_comparator.h"
#include "mongo/db/api_parameters.h"
#include "mongo/db/basic_types.h"
#include "mongo/db/client.h"
#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/pipeline/aggregate_command_gen.h"
#include "mongo/db/query/query_request_helper.h"
#include "mongo/db/server_options.h"
#include "mongo/db/write_concern_options.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/s/resharding/resharding_feature_flag_gen.h"
#include "mongo/transport/session.h"
#include "mongo/util/decorable.h"
#include "mongo/util/namespace_string_util.h"
#include "mongo/util/str.h"

namespace mongo {
namespace aggregation_request_helper {

/**
 * Validate the aggregate command object.
 */
void validate(OperationContext* opCtx,
              const BSONObj& cmdObj,
              const NamespaceString& nss,
              boost::optional<ExplainOptions::Verbosity> explainVerbosity);

AggregateCommandRequest parseFromBSON(OperationContext* opCtx,
                                      const DatabaseName& dbName,
                                      const BSONObj& cmdObj,
                                      boost::optional<ExplainOptions::Verbosity> explainVerbosity,
                                      bool apiStrict,
                                      const SerializationContext& serializationContext) {
    return parseFromBSON(
        opCtx, parseNs(dbName, cmdObj), cmdObj, explainVerbosity, apiStrict, serializationContext);
}

StatusWith<AggregateCommandRequest> parseFromBSONForTests(
    NamespaceString nss,
    const BSONObj& cmdObj,
    boost::optional<ExplainOptions::Verbosity> explainVerbosity,
    bool apiStrict) {
    try {
        return parseFromBSON(
            /*opCtx=*/nullptr, nss, cmdObj, explainVerbosity, apiStrict, SerializationContext());
    } catch (const AssertionException&) {
        return exceptionToStatus();
    }
}

StatusWith<AggregateCommandRequest> parseFromBSONForTests(
    const DatabaseName& dbName,
    const BSONObj& cmdObj,
    boost::optional<ExplainOptions::Verbosity> explainVerbosity,
    bool apiStrict) {
    try {
        // TODO SERVER-75930: pass serializationContext in
        return parseFromBSON(
            /*opCtx=*/nullptr, dbName, cmdObj, explainVerbosity, apiStrict, SerializationContext());
    } catch (const AssertionException&) {
        return exceptionToStatus();
    }
}

AggregateCommandRequest parseFromBSON(OperationContext* opCtx,
                                      NamespaceString nss,
                                      const BSONObj& cmdObj,
                                      boost::optional<ExplainOptions::Verbosity> explainVerbosity,
                                      bool apiStrict,
                                      const SerializationContext& serializationContext) {

    // if the command object lacks field 'aggregate' or '$db', we will use the namespace in 'nss'.
    bool cmdObjChanged = false;
    auto cmdObjBob = BSONObjBuilder{BSON(AggregateCommandRequest::kCommandName << nss.coll())};
    if (!cmdObj.hasField(AggregateCommandRequest::kCommandName) ||
        !cmdObj.hasField(AggregateCommandRequest::kDbNameFieldName)) {
        cmdObjBob.append("$db", nss.db_deprecated());
        cmdObjBob.appendElementsUnique(cmdObj);
        cmdObjChanged = true;
    }

    AggregateCommandRequest request(nss);
    // TODO SERVER-75930: tenantId in VTS isn't properly detected by call to parse(IDLParseContext&,
    // BSONObj&)
    request = AggregateCommandRequest::parse(
        IDLParserContext("aggregate", apiStrict, nss.tenantId(), serializationContext),
        cmdObjChanged ? cmdObjBob.obj() : cmdObj);

    if (explainVerbosity) {
        uassert(ErrorCodes::FailedToParse,
                str::stream() << "The '" << AggregateCommandRequest::kExplainFieldName
                              << "' option is illegal when a explain verbosity is also provided",
                !cmdObj.hasField(AggregateCommandRequest::kExplainFieldName));
        request.setExplain(explainVerbosity);
    }

    validate(opCtx, cmdObj, nss, explainVerbosity);
    return request;
}

NamespaceString parseNs(const DatabaseName& dbName, const BSONObj& cmdObj) {
    auto firstElement = cmdObj.firstElement();

    if (firstElement.isNumber()) {
        uassert(ErrorCodes::FailedToParse,
                str::stream() << "Invalid command format: the '"
                              << firstElement.fieldNameStringData()
                              << "' field must specify a collection name or 1",
                firstElement.number() == 1);
        return NamespaceString::makeCollectionlessAggregateNSS(dbName);
    } else {
        uassert(ErrorCodes::TypeMismatch,
                str::stream() << "collection name has invalid type: "
                              << typeName(firstElement.type()),
                firstElement.type() == BSONType::String);

        NamespaceString nss(
            NamespaceStringUtil::parseNamespaceFromRequest(dbName, firstElement.valueStringData()));

        uassert(ErrorCodes::InvalidNamespace,
                str::stream() << "Invalid namespace specified '" << nss.toStringForErrorMsg()
                              << "'",
                nss.isValid() && !nss.isCollectionlessAggregateNS());

        return nss;
    }
}

BSONObj serializeToCommandObj(const AggregateCommandRequest& request) {
    return request.toBSON(BSONObj());
}

Document serializeToCommandDoc(const AggregateCommandRequest& request) {
    return Document(request.toBSON(BSONObj()).getOwned());
}

void validate(OperationContext* opCtx,
              const BSONObj& cmdObj,
              const NamespaceString& nss,
              boost::optional<ExplainOptions::Verbosity> explainVerbosity) {
    bool hasCursorElem = cmdObj.hasField(AggregateCommandRequest::kCursorFieldName);
    bool hasExplainElem = cmdObj.hasField(AggregateCommandRequest::kExplainFieldName);
    bool hasExplain = explainVerbosity ||
        (hasExplainElem && cmdObj[AggregateCommandRequest::kExplainFieldName].Bool());
    bool hasFromMongosElem = cmdObj.hasField(AggregateCommandRequest::kFromMongosFieldName);
    bool hasNeedsMergeElem = cmdObj.hasField(AggregateCommandRequest::kNeedsMergeFieldName);

    // 'hasExplainElem' implies an aggregate command-level explain option, which does not require
    // a cursor argument.
    uassert(ErrorCodes::FailedToParse,
            str::stream() << "The '" << AggregateCommandRequest::kCursorFieldName
                          << "' option is required, except for aggregate with the explain argument",
            hasCursorElem || hasExplainElem);

    uassert(ErrorCodes::FailedToParse,
            str::stream() << "Aggregation explain does not support the'"
                          << WriteConcernOptions::kWriteConcernField << "' option",
            !hasExplain || !cmdObj[WriteConcernOptions::kWriteConcernField]);

    uassert(ErrorCodes::FailedToParse,
            str::stream() << "Cannot specify '" << AggregateCommandRequest::kNeedsMergeFieldName
                          << "' without '" << AggregateCommandRequest::kFromMongosFieldName << "'",
            (!hasNeedsMergeElem || hasFromMongosElem));

    auto requestReshardingResumeTokenElem =
        cmdObj[AggregateCommandRequest::kRequestReshardingResumeTokenFieldName];
    uassert(ErrorCodes::FailedToParse,
            str::stream() << AggregateCommandRequest::kRequestReshardingResumeTokenFieldName
                          << " must be a boolean type",
            !requestReshardingResumeTokenElem || requestReshardingResumeTokenElem.isBoolean());
    bool hasRequestReshardingResumeToken =
        requestReshardingResumeTokenElem && requestReshardingResumeTokenElem.boolean();
    uassert(ErrorCodes::FailedToParse,
            str::stream() << AggregateCommandRequest::kRequestReshardingResumeTokenFieldName
                          << " must only be set for the oplog namespace, not "
                          << nss.toStringForErrorMsg(),
            !hasRequestReshardingResumeToken || nss.isOplog());

    auto requestResumeTokenElem = cmdObj[AggregateCommandRequest::kRequestResumeTokenFieldName];
    uassert(ErrorCodes::InvalidOptions,
            "$_requestResumeToken is not supported without Resharding Improvements",
            !requestResumeTokenElem ||
                resharding::gFeatureFlagReshardingImprovements.isEnabled(
                    serverGlobalParams.featureCompatibility));
    uassert(ErrorCodes::FailedToParse,
            str::stream() << AggregateCommandRequest::kRequestResumeTokenFieldName
                          << " must be a boolean type",
            !requestResumeTokenElem || requestResumeTokenElem.isBoolean());
    bool hasRequestResumeToken = requestResumeTokenElem && requestResumeTokenElem.boolean();
    uassert(ErrorCodes::FailedToParse,
            str::stream() << AggregateCommandRequest::kRequestResumeTokenFieldName
                          << " must be set for non-oplog namespace",
            !hasRequestResumeToken || !nss.isOplog());
    if (hasRequestResumeToken) {
        auto hintElem = cmdObj[AggregateCommandRequest::kHintFieldName];
        uassert(ErrorCodes::BadValue,
                "hint must be {$natural:1} if 'requestResumeToken' is enabled",
                hintElem && hintElem.isABSONObj() &&
                    SimpleBSONObjComparator::kInstance.evaluate(
                        hintElem.Obj() == BSON(query_request_helper::kNaturalSortField << 1)));
    }
}

void validateRequestForAPIVersion(const OperationContext* opCtx,
                                  const AggregateCommandRequest& request) {
    invariant(opCtx);

    auto apiParameters = APIParameters::get(opCtx);
    bool apiStrict = apiParameters.getAPIStrict().value_or(false);
    const auto apiVersion = apiParameters.getAPIVersion().value_or("");
    auto client = opCtx->getClient();

    // An internal client could be one of the following :
    //     - Does not have any transport session
    //     - The transport session tag is internal
    bool isInternalClient =
        !client->session() || (client->session()->getTags() & transport::Session::kInternalClient);

    // Checks that the 'exchange' or 'fromMongos' option can only be specified by the internal
    // client.
    if ((request.getExchange() || request.getFromMongos()) && apiStrict && apiVersion == "1") {
        uassert(ErrorCodes::APIStrictError,
                str::stream() << "'exchange' and 'fromMongos' option cannot be specified with "
                                 "'apiStrict: true' in API Version "
                              << apiVersion,
                isInternalClient);
    }
}

void validateRequestFromClusterQueryWithoutShardKey(const AggregateCommandRequest& request) {
    if (request.getIsClusterQueryWithoutShardKeyCmd()) {
        uassert(ErrorCodes::InvalidOptions,
                "Only mongos can set the isClusterQueryWithoutShardKeyCmd field",
                request.getFromMongos());
    }
}

PlanExecutorPipeline::ResumableScanType getResumableScanType(const AggregateCommandRequest& request,
                                                             bool isChangeStream) {
    // $changeStream cannot be run on the oplog, and $_requestReshardingResumeToken can only be run
    // on the oplog. An aggregation request with both should therefore never reach this point.
    tassert(5353400,
            "$changeStream can't be combined with _requestReshardingResumeToken: true",
            !(isChangeStream && request.getRequestReshardingResumeToken()));
    if (isChangeStream) {
        return PlanExecutorPipeline::ResumableScanType::kChangeStream;
    }
    if (request.getRequestReshardingResumeToken()) {
        return PlanExecutorPipeline::ResumableScanType::kOplogScan;
    }
    return PlanExecutorPipeline::ResumableScanType::kNone;
}
}  // namespace aggregation_request_helper

// Custom serializers/deserializers for AggregateCommandRequest.

/**
 * IMPORTANT: The method should not be modified, as API version input/output guarantees could
 * break because of it.
 */
boost::optional<mongo::ExplainOptions::Verbosity> parseExplainModeFromBSON(
    const BSONElement& explainElem) {
    uassert(ErrorCodes::TypeMismatch,
            "explain must be a boolean",
            explainElem.type() == BSONType::Bool);

    if (explainElem.Bool()) {
        return ExplainOptions::Verbosity::kQueryPlanner;
    }

    return boost::none;
}

/**
 * IMPORTANT: The method should not be modified, as API version input/output guarantees could
 * break because of it.
 */
void serializeExplainToBSON(const mongo::ExplainOptions::Verbosity& explain,
                            StringData fieldName,
                            BSONObjBuilder* builder) {
    // Note that we do not serialize 'explain' field to the command object. This serializer only
    // serializes an empty cursor object for field 'cursor' when it is an explain command.
    builder->append(AggregateCommandRequest::kCursorFieldName, BSONObj());

    return;
}

/**
 * IMPORTANT: The method should not be modified, as API version input/output guarantees could
 * break because of it.
 */
mongo::SimpleCursorOptions parseAggregateCursorFromBSON(const BSONElement& cursorElem) {
    if (cursorElem.eoo()) {
        SimpleCursorOptions cursor;
        cursor.setBatchSize(aggregation_request_helper::kDefaultBatchSize);
        return cursor;
    }

    uassert(ErrorCodes::TypeMismatch,
            "cursor field must be missing or an object",
            cursorElem.type() == mongo::Object);

    SimpleCursorOptions cursor = SimpleCursorOptions::parse(
        IDLParserContext(AggregateCommandRequest::kCursorFieldName), cursorElem.embeddedObject());
    if (!cursor.getBatchSize())
        cursor.setBatchSize(aggregation_request_helper::kDefaultBatchSize);

    return cursor;
}

/**
 * IMPORTANT: The method should not be modified, as API version input/output guarantees could
 * break because of it.
 */
void serializeAggregateCursorToBSON(const mongo::SimpleCursorOptions& cursor,
                                    StringData fieldName,
                                    BSONObjBuilder* builder) {
    if (!builder->hasField(fieldName)) {
        builder->append(
            fieldName,
            BSON(aggregation_request_helper::kBatchSizeField
                 << cursor.getBatchSize().value_or(aggregation_request_helper::kDefaultBatchSize)));
    }

    return;
}
}  // namespace mongo
