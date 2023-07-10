/**
 *    Copyright (C) 2018-present MongoDB, Inc.
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

#include <boost/preprocessor/control/iif.hpp>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/db/commands/fsync_locked.h"
#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/pipeline/document_source_current_op.h"
#include "mongo/db/query/allowed_contexts.h"
#include "mongo/s/sharding_feature_flags_gen.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/intrusive_counter.h"
#include "mongo/util/str.h"

namespace mongo {

namespace {
const StringData kAllUsersFieldName = "allUsers"_sd;
const StringData kIdleConnectionsFieldName = "idleConnections"_sd;
const StringData kIdleSessionsFieldName = "idleSessions"_sd;
const StringData kLocalOpsFieldName = "localOps"_sd;
const StringData kTruncateOpsFieldName = "truncateOps"_sd;
const StringData kIdleCursorsFieldName = "idleCursors"_sd;
const StringData kBacktraceFieldName = "backtrace"_sd;
const StringData kTargetAllNodesFieldName = "targetAllNodes"_sd;

const StringData kOpIdFieldName = "opid"_sd;
const StringData kClientFieldName = "client"_sd;
const StringData kMongosClientFieldName = "client_s"_sd;
const StringData kShardFieldName = "shard"_sd;
}  // namespace

using boost::intrusive_ptr;

REGISTER_DOCUMENT_SOURCE(currentOp,
                         DocumentSourceCurrentOp::LiteParsed::parse,
                         DocumentSourceCurrentOp::createFromBson,
                         AllowedWithApiStrict::kNeverInVersion1);

constexpr StringData DocumentSourceCurrentOp::kStageName;

std::unique_ptr<DocumentSourceCurrentOp::LiteParsed> DocumentSourceCurrentOp::LiteParsed::parse(
    const NamespaceString& nss, const BSONElement& spec) {
    // Need to check the value of allUsers; if true then inprog privilege is required.
    if (spec.type() != BSONType::Object) {
        uasserted(ErrorCodes::TypeMismatch,
                  str::stream() << "$currentOp options must be specified in an object, but found: "
                                << typeName(spec.type()));
    }

    auto allUsers = kDefaultUserMode;
    auto localOps = kDefaultLocalOpsMode;

    // Check the spec for all fields named 'allUsers'. If any of them are 'true', we require
    // the 'inprog' privilege. This avoids the possibility that a spec with multiple
    // allUsers fields might allow an unauthorized user to view all operations. We also check for
    // the presence of a 'localOps' field, which instructs this $currentOp to list local mongoS
    // operations rather than forwarding the request to the shards.
    for (auto&& elem : spec.embeddedObject()) {
        if (elem.fieldNameStringData() == "allUsers"_sd) {
            if (elem.type() != BSONType::Bool) {
                uasserted(ErrorCodes::TypeMismatch,
                          str::stream() << "The 'allUsers' parameter of the $currentOp stage "
                                           "must be a boolean value, but found: "
                                        << typeName(elem.type()));
            }

            if (elem.boolean()) {
                allUsers = UserMode::kIncludeAll;
            }
        } else if (elem.fieldNameStringData() == "localOps") {
            uassert(ErrorCodes::TypeMismatch,
                    str::stream() << "The 'localOps' parameter of the $currentOp stage must be a "
                                     "boolean value, but found: "
                                  << typeName(elem.type()),
                    elem.type() == BSONType::Bool);

            if (elem.boolean()) {
                localOps = LocalOpsMode::kLocalMongosOps;
            }
        }
    }

    return std::make_unique<DocumentSourceCurrentOp::LiteParsed>(
        spec.fieldName(), nss.tenantId(), allUsers, localOps);
}

const char* DocumentSourceCurrentOp::getSourceName() const {
    return kStageName.rawData();
}

DocumentSource::GetNextResult DocumentSourceCurrentOp::doGetNext() {
    if (_ops.empty()) {
        _ops = pExpCtx->mongoProcessInterface->getCurrentOps(
            pExpCtx,
            _includeIdleConnections.value_or(kDefaultConnMode),
            _includeIdleSessions.value_or(kDefaultSessionMode),
            _includeOpsFromAllUsers.value_or(kDefaultUserMode),
            _truncateOps.value_or(kDefaultTruncationMode),
            _idleCursors.value_or(kDefaultCursorMode),
            _backtrace.value_or(kDefaultBacktraceMode));

        _opsIter = _ops.begin();

        if (pExpCtx->fromMongos) {
            _shardName = pExpCtx->mongoProcessInterface->getShardName(pExpCtx->opCtx);

            uassert(40465,
                    "Aggregation request specified 'fromMongos' but unable to retrieve shard name "
                    "for $currentOp pipeline stage.",
                    !_shardName.empty());
        }
    }

    if (_opsIter != _ops.end()) {
        if (!pExpCtx->fromMongos) {
            return Document(*_opsIter++);
        }

        // This $currentOp is running in a sharded context.
        invariant(!_shardName.empty());

        const BSONObj& op = *_opsIter++;
        MutableDocument doc;

        // Add the shard name to the output document.
        doc.addField(kShardFieldName, Value(_shardName));

        if (feature_flags::gClusterFsyncLock.isEnabled(serverGlobalParams.featureCompatibility) &&
            mongo::lockedForWriting()) {
            doc.addField(StringData("fsyncLock"), Value(true));
        }

        // For operations on a shard, we change the opid from the raw numeric form to
        // 'shardname:opid'. We also change the fieldname 'client' to 'client_s' to indicate
        // that the IP is that of the mongos which initiated this request.
        for (auto&& elt : op) {
            StringData fieldName = elt.fieldNameStringData();

            if (fieldName == kOpIdFieldName) {
                uassert(ErrorCodes::TypeMismatch,
                        str::stream() << "expected numeric opid for $currentOp response from '"
                                      << _shardName << "' but got: " << typeName(elt.type()),
                        elt.isNumber());

                std::string shardOpID = (str::stream() << _shardName << ":" << elt.numberInt());
                doc.addField(kOpIdFieldName, Value(shardOpID));
            } else if (fieldName == kClientFieldName) {
                doc.addField(kMongosClientFieldName, Value(elt.str()));
            } else {
                doc.addField(fieldName, Value(elt));
            }
        }

        return doc.freeze();
    }

    return GetNextResult::makeEOF();
}

intrusive_ptr<DocumentSource> DocumentSourceCurrentOp::createFromBson(
    BSONElement spec, const intrusive_ptr<ExpressionContext>& pExpCtx) {
    uassert(ErrorCodes::FailedToParse,
            str::stream() << "$currentOp options must be specified in an object, but found: "
                          << typeName(spec.type()),
            spec.type() == BSONType::Object);

    const NamespaceString& nss = pExpCtx->ns;

    uassert(ErrorCodes::InvalidNamespace,
            "$currentOp must be run against the 'admin' database with {aggregate: 1}",
            nss.isAdminDB() && nss.isCollectionlessAggregateNS());

    boost::optional<ConnMode> includeIdleConnections;
    boost::optional<SessionMode> includeIdleSessions;
    boost::optional<UserMode> includeOpsFromAllUsers;
    boost::optional<LocalOpsMode> showLocalOpsOnMongoS;
    boost::optional<TruncationMode> truncateOps;
    boost::optional<CursorMode> idleCursors;
    boost::optional<BacktraceMode> backtrace;
    boost::optional<bool> targetAllNodes;

    for (auto&& elem : spec.embeddedObject()) {
        const auto fieldName = elem.fieldNameStringData();

        if (fieldName == kIdleConnectionsFieldName) {
            uassert(ErrorCodes::FailedToParse,
                    str::stream() << "The 'idleConnections' parameter of the $currentOp stage must "
                                     "be a boolean value, but found: "
                                  << typeName(elem.type()),
                    elem.type() == BSONType::Bool);
            includeIdleConnections =
                (elem.boolean() ? ConnMode::kIncludeIdle : ConnMode::kExcludeIdle);
        } else if (fieldName == kIdleSessionsFieldName) {
            uassert(
                ErrorCodes::FailedToParse,
                str::stream() << "The 'idleSessions' parameter of the $currentOp stage must be a "
                                 "boolean value, but found: "
                              << typeName(elem.type()),
                elem.type() == BSONType::Bool);
            includeIdleSessions =
                (elem.boolean() ? SessionMode::kIncludeIdle : SessionMode::kExcludeIdle);
        } else if (fieldName == kAllUsersFieldName) {
            uassert(ErrorCodes::FailedToParse,
                    str::stream() << "The 'allUsers' parameter of the $currentOp stage must be a "
                                     "boolean value, but found: "
                                  << typeName(elem.type()),
                    elem.type() == BSONType::Bool);
            includeOpsFromAllUsers =
                (elem.boolean() ? UserMode::kIncludeAll : UserMode::kExcludeOthers);
        } else if (fieldName == kLocalOpsFieldName) {
            uassert(ErrorCodes::FailedToParse,
                    str::stream() << "The 'localOps' parameter of the $currentOp stage must be "
                                     "a boolean value, but found: "
                                  << typeName(elem.type()),
                    elem.type() == BSONType::Bool);
            uassert(ErrorCodes::FailedToParse,
                    str::stream() << "The 'localOps' parameter of the $currentOp stage cannot be "
                                     "true when 'targetAllNodes' is also true",
                    !(targetAllNodes.value_or(false) && elem.boolean()));
            showLocalOpsOnMongoS =
                (elem.boolean() ? LocalOpsMode::kLocalMongosOps : LocalOpsMode::kRemoteShardOps);
        } else if (fieldName == kTruncateOpsFieldName) {
            uassert(ErrorCodes::FailedToParse,
                    str::stream() << "The 'truncateOps' parameter of the $currentOp stage must be "
                                     "a boolean value, but found: "
                                  << typeName(elem.type()),
                    elem.type() == BSONType::Bool);
            truncateOps =
                (elem.boolean() ? TruncationMode::kTruncateOps : TruncationMode::kNoTruncation);
        } else if (fieldName == kIdleCursorsFieldName) {
            uassert(ErrorCodes::FailedToParse,
                    str::stream() << "The 'idleCursors' parameter of the $currentOp stage must be "
                                     "a boolean value, but found: "
                                  << typeName(elem.type()),
                    elem.type() == BSONType::Bool);
            idleCursors =
                (elem.boolean() ? CursorMode::kIncludeCursors : CursorMode::kExcludeCursors);
        } else if (fieldName == kBacktraceFieldName) {
            uassert(ErrorCodes::FailedToParse,
                    str::stream() << "The 'backtrace' parameter of the $currentOp stage must be "
                                     "a boolean value, but found: "
                                  << typeName(elem.type()),
                    elem.type() == BSONType::Bool);
            backtrace = (elem.boolean() ? BacktraceMode::kIncludeBacktrace
                                        : BacktraceMode::kExcludeBacktrace);
        } else if (fieldName == kTargetAllNodesFieldName) {
            uassert(ErrorCodes::FailedToParse,
                    str::stream() << "The 'targetAllNodes' parameter of the $currentOp stage must "
                                     "be a boolean value, but found: "
                                  << typeName(elem.type()),
                    elem.type() == BSONType::Bool);
            uassert(ErrorCodes::FailedToParse,
                    "The 'localOps' parameter of the $currentOp stage cannot be "
                    "true when 'targetAllNodes' is also true",
                    !((showLocalOpsOnMongoS &&
                       showLocalOpsOnMongoS.value() == LocalOpsMode::kLocalMongosOps) &&
                      elem.boolean()));
            targetAllNodes = elem.boolean();
            if (targetAllNodes.value_or(false)) {
                uassert(ErrorCodes::FailedToParse,
                        "$currentOp supports targetAllNodes parameter only for sharded clusters",
                        pExpCtx->fromMongos || pExpCtx->inMongos);
            }
        } else {
            uasserted(ErrorCodes::FailedToParse,
                      str::stream()
                          << "Unrecognized option '" << fieldName << "' in $currentOp stage.");
        }
    }

    return new DocumentSourceCurrentOp(pExpCtx,
                                       includeIdleConnections,
                                       includeIdleSessions,
                                       includeOpsFromAllUsers,
                                       showLocalOpsOnMongoS,
                                       truncateOps,
                                       idleCursors,
                                       backtrace,
                                       targetAllNodes);
}

intrusive_ptr<DocumentSourceCurrentOp> DocumentSourceCurrentOp::create(
    const boost::intrusive_ptr<ExpressionContext>& pExpCtx,
    boost::optional<ConnMode> includeIdleConnections,
    boost::optional<SessionMode> includeIdleSessions,
    boost::optional<UserMode> includeOpsFromAllUsers,
    boost::optional<LocalOpsMode> showLocalOpsOnMongoS,
    boost::optional<TruncationMode> truncateOps,
    boost::optional<CursorMode> idleCursors,
    boost::optional<BacktraceMode> backtrace,
    boost::optional<bool> targetAllNodes) {
    return new DocumentSourceCurrentOp(pExpCtx,
                                       includeIdleConnections,
                                       includeIdleSessions,
                                       includeOpsFromAllUsers,
                                       showLocalOpsOnMongoS,
                                       truncateOps,
                                       idleCursors,
                                       backtrace,
                                       targetAllNodes);
}

Value DocumentSourceCurrentOp::serialize(SerializationOptions opts) const {
    return Value(Document{
        {getSourceName(),
         Document{
             {kIdleConnectionsFieldName,
              _includeIdleConnections.has_value()
                  ? opts.serializeLiteral(_includeIdleConnections.value() == ConnMode::kIncludeIdle)
                  : Value()},
             {kIdleSessionsFieldName,
              _includeIdleSessions.has_value()
                  ? opts.serializeLiteral(_includeIdleSessions.value() == SessionMode::kIncludeIdle)
                  : Value()},
             {kAllUsersFieldName,
              _includeOpsFromAllUsers.has_value()
                  ? opts.serializeLiteral(_includeOpsFromAllUsers.value() == UserMode::kIncludeAll)
                  : Value()},
             {kLocalOpsFieldName,
              _showLocalOpsOnMongoS.has_value()
                  ? opts.serializeLiteral(_showLocalOpsOnMongoS.value() ==
                                          LocalOpsMode::kLocalMongosOps)
                  : Value()},
             {kTruncateOpsFieldName,
              _truncateOps.has_value()
                  ? opts.serializeLiteral(_truncateOps.value() == TruncationMode::kTruncateOps)
                  : Value()},
             {kIdleCursorsFieldName,
              _idleCursors.has_value()
                  ? opts.serializeLiteral(_idleCursors.value() == CursorMode::kIncludeCursors)
                  : Value()},
             {kBacktraceFieldName,
              _backtrace.has_value()
                  ? opts.serializeLiteral(_backtrace.value() == BacktraceMode::kIncludeBacktrace)
                  : Value()},
             {kTargetAllNodesFieldName,
              _targetAllNodes.has_value() ? opts.serializeLiteral(_targetAllNodes.value())
                                          : Value()}}}});
}
}  // namespace mongo
