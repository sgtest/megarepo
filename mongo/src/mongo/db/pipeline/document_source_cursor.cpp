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


#include <boost/optional.hpp>
#include <map>
#include <type_traits>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/bson/bsontypes.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/curop_failpoint_helpers.h"
#include "mongo/db/db_raii.h"
#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/pipeline/document_source_cursor.h"
#include "mongo/db/pipeline/document_source_limit.h"
#include "mongo/db/query/collection_query_info.h"
#include "mongo/db/query/explain.h"
#include "mongo/db/query/explain_options.h"
#include "mongo/db/query/find_common.h"
#include "mongo/db/query/query_knobs_gen.h"
#include "mongo/db/query/query_settings_gen.h"
#include "mongo/db/repl/optime.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/s/resharding/resume_token_gen.h"
#include "mongo/util/decorable.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/intrusive_counter.h"
#include "mongo/util/scopeguard.h"
#include "mongo/util/serialization_context.h"
#include "mongo/util/uuid.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kQuery


namespace mongo {

MONGO_FAIL_POINT_DEFINE(hangBeforeDocumentSourceCursorLoadBatch);

using boost::intrusive_ptr;
using std::shared_ptr;
using std::string;

const char* DocumentSourceCursor::getSourceName() const {
    return kStageName.rawData();
}

bool DocumentSourceCursor::Batch::isEmpty() const {
    switch (_type) {
        case CursorType::kRegular:
            return _batchOfDocs.empty();
        case CursorType::kEmptyDocuments:
            return !_count;
    }
    MONGO_UNREACHABLE;
}

void DocumentSourceCursor::Batch::enqueue(Document&& doc, boost::optional<BSONObj> resumeToken) {
    switch (_type) {
        case CursorType::kRegular: {
            invariant(doc.isOwned());
            _batchOfDocs.push_back(std::move(doc));
            _memUsageBytes += _batchOfDocs.back().getApproximateSize();
            if (resumeToken) {
                _resumeTokens.push_back(*resumeToken);
                dassert(_resumeTokens.size() == _batchOfDocs.size());
            }
            break;
        }
        case CursorType::kEmptyDocuments: {
            ++_count;
            break;
        }
    }
}

Document DocumentSourceCursor::Batch::dequeue() {
    invariant(!isEmpty());
    switch (_type) {
        case CursorType::kRegular: {
            Document out = std::move(_batchOfDocs.front());
            _batchOfDocs.pop_front();
            if (_batchOfDocs.empty()) {
                _memUsageBytes = 0;
            }
            if (!_resumeTokens.empty()) {
                _resumeTokens.pop_front();
                dassert(_resumeTokens.size() == _batchOfDocs.size());
            }
            return out;
        }
        case CursorType::kEmptyDocuments: {
            --_count;
            return Document{};
        }
    }
    MONGO_UNREACHABLE;
}

void DocumentSourceCursor::Batch::clear() {
    _batchOfDocs.clear();
    _count = 0;
    _memUsageBytes = 0;
}

DocumentSource::GetNextResult DocumentSourceCursor::doGetNext() {
    if (_currentBatch.isEmpty()) {
        loadBatch();
    }

    // If we are tracking the oplog timestamp, update our cached latest optime.
    if (_resumeTrackingType == ResumeTrackingType::kOplog && _exec)
        _updateOplogTimestamp();
    else if (_resumeTrackingType == ResumeTrackingType::kNonOplog && _exec)
        _updateNonOplogResumeToken();

    if (_currentBatch.isEmpty())
        return GetNextResult::makeEOF();

    return _currentBatch.dequeue();
}

void DocumentSourceCursor::loadBatch() {
    if (!_exec || _exec->isDisposed()) {
        // No more documents.
        return;
    }

    CurOpFailpointHelpers::waitWhileFailPointEnabled(
        &hangBeforeDocumentSourceCursorLoadBatch,
        pExpCtx->opCtx,
        "hangBeforeDocumentSourceCursorLoadBatch",
        []() {
            LOGV2(20895,
                  "Hanging aggregation due to 'hangBeforeDocumentSourceCursorLoadBatch' failpoint");
        },
        _exec->nss());

    PlanExecutor::ExecState state;
    Document resultObj;

    boost::optional<AutoGetCollectionForReadMaybeLockFree> autoColl;
    tassert(5565800,
            "Expected PlanExecutor to use an external lock policy",
            _exec->lockPolicy() == PlanExecutor::LockPolicy::kLockExternally);
    autoColl.emplace(
        pExpCtx->opCtx,
        _exec->nss(),
        AutoGetCollection::Options{}.secondaryNssOrUUIDs(_exec->getSecondaryNamespaces().cbegin(),
                                                         _exec->getSecondaryNamespaces().cend()));
    uassertStatusOK(repl::ReplicationCoordinator::get(pExpCtx->opCtx)
                        ->checkCanServeReadsFor(pExpCtx->opCtx, _exec->nss(), true));

    _exec->restoreState(autoColl ? &autoColl->getCollection() : nullptr);

    try {
        ON_BLOCK_EXIT([this] { recordPlanSummaryStats(); });

        while ((state = _exec->getNextDocument(&resultObj, nullptr)) == PlanExecutor::ADVANCED) {
            boost::optional<BSONObj> resumeToken;
            if (_resumeTrackingType == ResumeTrackingType::kNonOplog)
                resumeToken = _exec->getPostBatchResumeToken();
            _currentBatch.enqueue(transformDoc(std::move(resultObj)), std::move(resumeToken));

            // As long as we're waiting for inserts, we shouldn't do any batching at this level we
            // need the whole pipeline to see each document to see if we should stop waiting.
            bool batchCountFull = _batchSizeCount != 0 && _currentBatch.count() >= _batchSizeCount;
            if (batchCountFull || _currentBatch.memUsageBytes() > _batchSizeBytes ||
                awaitDataState(pExpCtx->opCtx).shouldWaitForInserts) {
                // End this batch and prepare PlanExecutor for yielding.
                _exec->saveState();
                // Double the size for next batch when batch is full.
                if (batchCountFull && overflow::mul(_batchSizeCount, 2, &_batchSizeCount)) {
                    _batchSizeCount = 0;  // Go unlimited if we overflow.
                }
                return;
            }
        }

        invariant(state == PlanExecutor::IS_EOF);

        // Keep the inner PlanExecutor alive if the cursor is tailable, since more results may
        // become available in the future, or if we are tracking the latest oplog resume inforation,
        // since we will need to retrieve the resume information the executor observed before
        // hitting EOF.
        if (_resumeTrackingType != ResumeTrackingType::kNone || pExpCtx->isTailableAwaitData()) {
            _exec->saveState();
            return;
        }
    } catch (...) {
        // Record error details before re-throwing the exception.
        _execStatus = exceptionToStatus().withContext("Error in $cursor stage");
        throw;
    }

    // If we got here, there won't be any more documents and we no longer need our PlanExecutor, so
    // destroy it.
    cleanupExecutor();
}

void DocumentSourceCursor::_updateOplogTimestamp() {
    // If we are about to return a result, set our oplog timestamp to the optime of that result.
    if (!_currentBatch.isEmpty()) {
        const auto& ts = _currentBatch.peekFront().getField(repl::OpTime::kTimestampFieldName);
        invariant(ts.getType() == BSONType::bsonTimestamp);
        _latestOplogTimestamp = ts.getTimestamp();
        return;
    }

    // If we have no more results to return, advance to the latest oplog timestamp.
    _latestOplogTimestamp = _exec->getLatestOplogTimestamp();
}

void DocumentSourceCursor::_updateNonOplogResumeToken() {
    // If we are about to return a result, set our resume token to the one for that result.
    if (!_currentBatch.isEmpty()) {
        _latestNonOplogResumeToken = _currentBatch.peekFrontResumeToken();
        return;
    }

    // If we have no more results to return, advance to the latest executor resume token.
    _latestNonOplogResumeToken = _exec->getPostBatchResumeToken();
}

void DocumentSourceCursor::recordPlanSummaryStats() {
    invariant(_exec);
    _exec->getPlanExplainer().getSummaryStats(&_stats.planSummaryStats);
}

Value DocumentSourceCursor::serialize(const SerializationOptions& opts) const {
    auto verbosity = opts.verbosity;
    // We never parse a DocumentSourceCursor, so we only serialize for explain. Since it's never
    // part of user input, there's no need to compute its query shape.
    if (!verbosity || opts.transformIdentifiers ||
        opts.literalPolicy != LiteralSerializationPolicy::kUnchanged)
        return Value();

    invariant(_exec);

    uassert(50660,
            "Mismatch between verbosity passed to serialize() and expression context verbosity",
            verbosity == pExpCtx->explain);

    MutableDocument out;

    BSONObjBuilder explainStatsBuilder;

    {
        auto opCtx = pExpCtx->opCtx;
        auto secondaryNssList = _exec->getSecondaryNamespaces();
        AutoGetCollectionForReadMaybeLockFree readLock(
            opCtx,
            _exec->nss(),
            AutoGetCollection::Options{}.secondaryNssOrUUIDs(secondaryNssList.cbegin(),
                                                             secondaryNssList.cend()));
        MultipleCollectionAccessor collections(opCtx,
                                               &readLock.getCollection(),
                                               readLock.getNss(),
                                               readLock.isAnySecondaryNamespaceAViewOrSharded(),
                                               secondaryNssList);

        Explain::explainStages(_exec.get(),
                               collections,
                               verbosity.value(),
                               _execStatus,
                               _winningPlanTrialStats,
                               BSONObj(),
                               SerializationContext::stateCommandReply(pExpCtx->serializationCtxt),
                               BSONObj(),
                               &explainStatsBuilder);
    }

    BSONObj explainStats = explainStatsBuilder.obj();
    invariant(explainStats["queryPlanner"]);
    out["queryPlanner"] = Value(explainStats["queryPlanner"]);

    if (verbosity.value() >= ExplainOptions::Verbosity::kExecStats) {
        invariant(explainStats["executionStats"]);
        out["executionStats"] = Value(explainStats["executionStats"]);
    }

    return Value(DOC(getSourceName() << out.freezeToValue()));
}

void DocumentSourceCursor::detachFromOperationContext() {
    // Only detach the underlying executor if it hasn't been detached already.
    if (_exec && _exec->getOpCtx()) {
        _exec->detachFromOperationContext();
    }
}

void DocumentSourceCursor::reattachToOperationContext(OperationContext* opCtx) {
    if (_exec) {
        _exec->reattachToOperationContext(opCtx);
    }
}

void DocumentSourceCursor::doDispose() {
    _currentBatch.clear();
    if (!_exec || _exec->isDisposed()) {
        // We've already properly disposed of our PlanExecutor.
        return;
    }
    cleanupExecutor();
}

void DocumentSourceCursor::cleanupExecutor() {
    invariant(_exec);
    _exec->dispose(pExpCtx->opCtx);

    // Not freeing _exec if we're in explain mode since it will be used in serialize() to gather
    // execution stats.
    if (!pExpCtx->explain) {
        _exec.reset();
    }
}

BSONObj DocumentSourceCursor::getPostBatchResumeToken() const {
    if (_resumeTrackingType == ResumeTrackingType::kOplog) {
        return ResumeTokenOplogTimestamp{getLatestOplogTimestamp()}.toBSON();
    } else if (_resumeTrackingType == ResumeTrackingType::kNonOplog) {
        return _latestNonOplogResumeToken;
    }
    return BSONObj{};
}

DocumentSourceCursor::~DocumentSourceCursor() {
    if (pExpCtx->explain) {
        invariant(_exec->isDisposed());  // _exec should have at least been disposed.
    } else {
        invariant(!_exec);  // '_exec' should have been cleaned up via dispose() before destruction.
    }
}

DocumentSourceCursor::DocumentSourceCursor(
    const MultipleCollectionAccessor& collections,
    std::unique_ptr<PlanExecutor, PlanExecutor::Deleter> exec,
    const intrusive_ptr<ExpressionContext>& pCtx,
    CursorType cursorType,
    ResumeTrackingType resumeTrackingType)
    : DocumentSource(kStageName, pCtx),
      _currentBatch(cursorType),
      _exec(std::move(exec)),
      _resumeTrackingType(resumeTrackingType),
      _queryFramework(_exec->getQueryFramework()) {
    // It is illegal for both 'kEmptyDocuments' to be set and _resumeTrackingType to be other than
    // 'kNone'.
    uassert(ErrorCodes::InvalidOptions,
            "The resumeToken is not compatible with this query",
            cursorType != CursorType::kEmptyDocuments ||
                resumeTrackingType == ResumeTrackingType::kNone);

    // Later code in the DocumentSourceCursor lifecycle expects that '_exec' is in a saved state.
    _exec->saveState();

    auto&& explainer = _exec->getPlanExplainer();
    _planSummary = explainer.getPlanSummary();
    recordPlanSummaryStats();

    if (pExpCtx->explain) {
        // It's safe to access the executor even if we don't have the collection lock since we're
        // just going to call getStats() on it.
        _winningPlanTrialStats = explainer.getWinningPlanTrialStats();
    }

    if (collections.hasMainCollection()) {
        const auto& coll = collections.getMainCollection();
        CollectionQueryInfo::get(coll).notifyOfQuery(pExpCtx->opCtx, coll, _stats.planSummaryStats);
    }
    for (auto& [nss, coll] : collections.getSecondaryCollections()) {
        if (coll) {
            PlanSummaryStats stats;
            explainer.getSecondarySummaryStats(nss, &stats);
            CollectionQueryInfo::get(coll).notifyOfQuery(pExpCtx->opCtx, coll, stats);
        }
    }

    initializeBatchSizeCounts();
    _batchSizeBytes = static_cast<size_t>(internalDocumentSourceCursorBatchSizeBytes.load());
}

void DocumentSourceCursor::initializeBatchSizeCounts() {
    // '0' means there's no limitation.
    _batchSizeCount = 0;
    if (auto cq = _exec->getCanonicalQuery()) {
        if (cq->getFindCommandRequest().getLimit().has_value()) {
            // $limit is pushed down into executor, skipping batch size count limitation.
            return;
        }
        for (const auto& ds : cq->cqPipeline()) {
            if (ds->documentSource()->getSourceName() == DocumentSourceLimit::kStageName) {
                // $limit is pushed down into executor, skipping batch size count limitation.
                return;
            }
        }
    }
    // No $limit is pushed down into executor, reading limit from knobs.
    _batchSizeCount = internalDocumentSourceCursorInitialBatchSize.load();
}

intrusive_ptr<DocumentSourceCursor> DocumentSourceCursor::create(
    const MultipleCollectionAccessor& collections,
    std::unique_ptr<PlanExecutor, PlanExecutor::Deleter> exec,
    const intrusive_ptr<ExpressionContext>& pExpCtx,
    CursorType cursorType,
    ResumeTrackingType resumeTrackingType) {
    intrusive_ptr<DocumentSourceCursor> source(new DocumentSourceCursor(
        collections, std::move(exec), pExpCtx, cursorType, resumeTrackingType));
    return source;
}
}  // namespace mongo
