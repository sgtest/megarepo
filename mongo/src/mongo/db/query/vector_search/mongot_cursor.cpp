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

#include "mongo/db/query/vector_search/mongot_cursor.h"

namespace mongo::mongot_cursor {

namespace {

executor::RemoteCommandRequest getRemoteCommandRequestForVectorSearchQuery(
    const boost::intrusive_ptr<ExpressionContext>& expCtx, const VectorSearchSpec& request) {
    BSONObjBuilder cmdBob;
    cmdBob.append(kVectorSearchCmd, expCtx->ns.coll());
    uassert(7828001,
            str::stream()
                << "A uuid is required for a vector search query, but was missing. Got namespace "
                << expCtx->ns.toStringForErrorMsg(),
            expCtx->uuid);
    expCtx->uuid.value().appendToBuilder(&cmdBob, kCollectionUuidField);

    cmdBob.append(VectorSearchSpec::kQueryVectorFieldName, request.getQueryVector());
    cmdBob.append(VectorSearchSpec::kPathFieldName, request.getPath());
    cmdBob.append(VectorSearchSpec::kLimitFieldName, request.getLimit().coerceToLong());

    if (request.getIndex()) {
        cmdBob.append(VectorSearchSpec::kIndexFieldName, *request.getIndex());
    }

    if (request.getNumCandidates()) {
        cmdBob.append(VectorSearchSpec::kNumCandidatesFieldName,
                      request.getNumCandidates()->coerceToLong());
    }

    if (request.getFilter()) {
        cmdBob.append(VectorSearchSpec::kFilterFieldName, *request.getFilter());
    }
    if (expCtx->explain) {
        cmdBob.append("explain",
                      BSON("verbosity" << ExplainOptions::verbosityString(*expCtx->explain)));
    }

    return getRemoteCommandRequest(expCtx->opCtx, expCtx->ns, cmdBob.obj());
}

}  // namespace

executor::TaskExecutorCursor establishVectorSearchCursor(
    const boost::intrusive_ptr<ExpressionContext>& expCtx,
    const VectorSearchSpec& request,
    std::shared_ptr<executor::TaskExecutor> taskExecutor) {
    // Note that we always pre-fetch the next batch here. This is because we generally expect
    // everything to fit into one batch, since we give mongot the exact upper bound initially - we
    // will only see multiple batches if this upper bound doesn't fit in 16MB. This should be a rare
    // enough case that it shouldn't overwhelm mongot to pre-fetch.
    auto cursors = establishCursors(expCtx,
                                    getRemoteCommandRequestForVectorSearchQuery(expCtx, request),
                                    taskExecutor,
                                    true /* preFetchNextBatch */);
    // Should always have one results cursor.
    tassert(7828000, "Expected exactly one cursor from mongot", cursors.size() == 1);
    return std::move(cursors.front());
}

BSONObj getVectorSearchExplainResponse(const boost::intrusive_ptr<ExpressionContext>& expCtx,
                                       const VectorSearchSpec& spec,
                                       executor::TaskExecutor* taskExecutor) {
    auto request = getRemoteCommandRequestForVectorSearchQuery(expCtx, spec);
    return mongot_cursor::getExplainResponse(expCtx.get(), request, taskExecutor);
}

}  // namespace mongo::mongot_cursor
