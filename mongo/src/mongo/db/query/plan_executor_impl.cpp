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


#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr.hpp>
#include <cstddef>
#include <fmt/format.h>
#include <memory>
#include <string>
#include <utility>
#include <variant>

#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/curop.h"
#include "mongo/db/exec/cached_plan.h"
#include "mongo/db/exec/collection_scan.h"
#include "mongo/db/exec/document_value/document_metadata_fields.h"
#include "mongo/db/exec/plan_stage.h"
#include "mongo/db/exec/plan_stats.h"
#include "mongo/db/exec/subplan.h"
#include "mongo/db/exec/timeseries_modify.h"
#include "mongo/db/exec/trial_stage.h"
#include "mongo/db/exec/update_stage.h"
#include "mongo/db/exec/working_set.h"
#include "mongo/db/query/cursor_response.h"
#include "mongo/db/query/find_command.h"
#include "mongo/db/query/find_common.h"
#include "mongo/db/query/mock_yield_policies.h"
#include "mongo/db/query/plan_executor_impl.h"
#include "mongo/db/query/plan_explainer_factory.h"
#include "mongo/db/query/plan_explainer_impl.h"
#include "mongo/db/query/plan_insert_listener.h"
#include "mongo/db/query/plan_yield_policy_impl.h"
#include "mongo/db/query/stage_types.h"
#include "mongo/db/query/yield_policy_callbacks_impl.h"
#include "mongo/db/repl/optime.h"
#include "mongo/db/s/operation_sharding_state.h"
#include "mongo/db/service_context.h"
#include "mongo/db/shard_role.h"
#include "mongo/db/transaction_resources.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/compiler.h"
#include "mongo/util/decorable.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/future.h"
#include "mongo/util/intrusive_counter.h"
#include "mongo/util/namespace_string_util.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kQuery


namespace mongo {
using namespace fmt::literals;
using std::shared_ptr;
using std::string;
using std::unique_ptr;
using std::vector;

const OperationContext::Decoration<boost::optional<repl::OpTime>> clientsLastKnownCommittedOpTime =
    OperationContext::declareDecoration<boost::optional<repl::OpTime>>();

// This failpoint is also accessed by the SBE executor so we define it outside of an anonymous
// namespace.
MONGO_FAIL_POINT_DEFINE(planExecutorHangBeforeShouldWaitForInserts);

PlanExecutorImpl::PlanExecutorImpl(OperationContext* opCtx,
                                   unique_ptr<WorkingSet> ws,
                                   unique_ptr<PlanStage> rt,
                                   unique_ptr<QuerySolution> qs,
                                   unique_ptr<CanonicalQuery> cq,
                                   const boost::intrusive_ptr<ExpressionContext>& expCtx,
                                   VariantCollectionPtrOrAcquisition collection,
                                   bool returnOwnedBson,
                                   NamespaceString nss,
                                   PlanYieldPolicy::YieldPolicy yieldPolicy,
                                   boost::optional<size_t> cachedPlanHash)
    : _opCtx(opCtx),
      _cq(std::move(cq)),
      _expCtx(_cq ? _cq->getExpCtx() : expCtx),
      _workingSet(std::move(ws)),
      _qs(std::move(qs)),
      _root(std::move(rt)),
      _planExplainer(plan_explainer_factory::make(_root.get(), cachedPlanHash)),
      _mustReturnOwnedBson(returnOwnedBson),
      _nss(std::move(nss)) {
    invariant(!_expCtx || _expCtx->opCtx == _opCtx);
    invariant(!_cq || !_expCtx || _cq->getExpCtx() == _expCtx);

    const CollectionPtr* collectionPtr = &collection.getCollectionPtr();
    invariant(collectionPtr);
    const bool collectionExists = static_cast<bool>(*collectionPtr);

    // If we don't yet have a namespace string, then initialize it from either 'collection' or
    // '_cq'.
    if (_nss.isEmpty()) {
        if (collectionExists) {
            _nss = (*collectionPtr)->ns();
        } else {
            invariant(_cq);
            if (_cq->getFindCommandRequest().getNamespaceOrUUID().isNamespaceString()) {
                _nss = _cq->getFindCommandRequest().getNamespaceOrUUID().nss();
            }
        }
    }

    _yieldPolicy = makeClassicYieldPolicy(
        _opCtx,
        _nss,
        this,
        collectionExists ? yieldPolicy : PlanYieldPolicy::YieldPolicy::INTERRUPT_ONLY,
        collection);

    if (_qs) {
        _planExplainer->setQuerySolution(_qs.get());
        _planExplainer->updateEnumeratorExplainInfo(_qs->_enumeratorExplainInfo);
    } else if (const MultiPlanStage* mps = getMultiPlanStage()) {
        const QuerySolution* soln = mps->bestSolution();
        _planExplainer->setQuerySolution(soln);
        _planExplainer->updateEnumeratorExplainInfo(soln->_enumeratorExplainInfo);
    } else if (auto subplan = getStageByType(_root.get(), STAGE_SUBPLAN)) {
        auto subplanStage = static_cast<SubplanStage*>(subplan);
        _planExplainer->updateEnumeratorExplainInfo(
            subplanStage->compositeSolution()->_enumeratorExplainInfo);
    }

    // If this PlanExecutor is executing a COLLSCAN, keep a pointer directly to the COLLSCAN
    // stage. This is used for change streams in order to keep the the latest oplog timestamp
    // and post batch resume token up to date as the oplog scan progresses.
    if (auto collectionScan = getStageByType(_root.get(), STAGE_COLLSCAN)) {
        _collScanStage = static_cast<CollectionScan*>(collectionScan);
    }
}

PlanExecutorImpl::~PlanExecutorImpl() {
    invariant(_currentState == kDisposed);
}

PlanStage* PlanExecutorImpl::getRootStage() const {
    return _root.get();
}

CanonicalQuery* PlanExecutorImpl::getCanonicalQuery() const {
    return _cq.get();
}

const NamespaceString& PlanExecutorImpl::nss() const {
    return _nss;
}

const std::vector<NamespaceStringOrUUID>& PlanExecutorImpl::getSecondaryNamespaces() const {
    // Return a reference to an empty static array. This array will never contain any elements
    // because a PlanExecutorImpl is only capable of executing against a single namespace. As
    // such, it will never lock more than one namespace.
    const static std::vector<NamespaceStringOrUUID> emptyNssVector;
    return emptyNssVector;
}

OperationContext* PlanExecutorImpl::getOpCtx() const {
    return _opCtx;
}

void PlanExecutorImpl::saveState() {
    invariant(_currentState == kUsable || _currentState == kSaved);

    if (!isMarkedAsKilled()) {
        _root->saveState();
    }

    if (!_yieldPolicy->usesCollectionAcquisitions()) {
        _yieldPolicy->setYieldable(nullptr);
    }
    _currentState = kSaved;
}

void PlanExecutorImpl::restoreState(const RestoreContext& context) {
    try {
        restoreStateWithoutRetrying(context, context.collection());
    } catch (const StorageUnavailableException&) {
        if (!_yieldPolicy->canAutoYield())
            throw;

        // Handles retries by calling restoreStateWithoutRetrying() in a loop.
        uassertStatusOK(_yieldPolicy->yieldOrInterrupt(getOpCtx()));
    }
}

void PlanExecutorImpl::restoreStateWithoutRetrying(const RestoreContext& context,
                                                   const Yieldable* yieldable) {
    invariant(_currentState == kSaved);

    if (!_yieldPolicy->usesCollectionAcquisitions()) {
        _yieldPolicy->setYieldable(yieldable);
    }
    if (!isMarkedAsKilled()) {
        _root->restoreState(context);
    }

    _currentState = kUsable;
    uassertStatusOK(_killStatus);
}

void PlanExecutorImpl::detachFromOperationContext() {
    invariant(_currentState == kSaved);
    _opCtx = nullptr;
    _root->detachFromOperationContext();
    if (_expCtx) {
        _expCtx->opCtx = nullptr;
    }
    _currentState = kDetached;
}

void PlanExecutorImpl::reattachToOperationContext(OperationContext* opCtx) {
    invariant(_currentState == kDetached);

    // We're reattaching for a getMore now.  Reset the yield timer in order to prevent from
    // yielding again right away.
    _yieldPolicy->resetTimer();

    _opCtx = opCtx;
    _root->reattachToOperationContext(opCtx);
    if (_expCtx) {
        _expCtx->opCtx = opCtx;
    }
    _currentState = kSaved;
}

namespace {
/**
 * Helper function used to determine if we need to hang before inserts.
 */
void hangBeforeShouldWaitForInsertsIfFailpointEnabled(PlanExecutorImpl* exec) {
    if (MONGO_unlikely(
            planExecutorHangBeforeShouldWaitForInserts.shouldFail([exec](const BSONObj& data) {
                auto fpNss = NamespaceStringUtil::parseFailPointData(data, "namespace"_sd);
                return fpNss.isEmpty() || fpNss == exec->nss();
            }))) {
        LOGV2(20946,
              "PlanExecutor - planExecutorHangBeforeShouldWaitForInserts fail point "
              "enabled. Blocking until fail point is disabled");
        planExecutorHangBeforeShouldWaitForInserts.pauseWhileSet();
    }
}

/**
 * Helper function used to construct lambda passed into yielding logic.
 */
void doYield(OperationContext* opCtx) {
    // If we yielded because we encountered a sharding critical section, wait for the critical
    // section to end before continuing. By waiting for the critical section to be exited we avoid
    // busy spinning immediately and encountering the same critical section again. It is important
    // that this wait happens after having released the lock hierarchy -- otherwise deadlocks could
    // happen, or the very least, locks would be unnecessarily held while waiting.
    const auto& shardingCriticalSection = planExecutorShardingCriticalSectionFuture(opCtx);
    if (shardingCriticalSection) {
        OperationShardingState::waitForCriticalSectionToComplete(opCtx, *shardingCriticalSection)
            .ignore();
        planExecutorShardingCriticalSectionFuture(opCtx).reset();
    }
}
}  // namespace

PlanExecutor::ExecState PlanExecutorImpl::getNext(BSONObj* objOut, RecordId* dlOut) {
    const auto state = getNextDocument(&_docOutput, dlOut);
    if (objOut && state == ExecState::ADVANCED) {
        const bool includeMetadata = _expCtx && _expCtx->needsMerge;
        *objOut = includeMetadata ? _docOutput.toBsonWithMetaData() : _docOutput.toBson();
    }
    return state;
}

PlanExecutor::ExecState PlanExecutorImpl::getNextDocument(Document* objOut, RecordId* dlOut) {
    Snapshotted<Document> snapshotted;
    if (objOut) {
        snapshotted.value() = std::move(*objOut);
    }
    ExecState state = _getNextImpl(objOut ? &snapshotted : nullptr, dlOut);

    if (objOut) {
        *objOut = std::move(snapshotted.value());
    }

    return state;
}

PlanExecutor::ExecState PlanExecutorImpl::_getNextImpl(Snapshotted<Document>* objOut,
                                                       RecordId* dlOut) {
    checkFailPointPlanExecAlwaysFails();

    invariant(_currentState == kUsable);
    if (isMarkedAsKilled()) {
        uassertStatusOK(_killStatus);
    }

    if (!_stash.empty()) {
        invariant(objOut && !dlOut);
        *objOut = {SnapshotId(), _stash.front()};
        _stash.pop_front();
        return PlanExecutor::ADVANCED;
    }

    // The below are incremented on every WriteConflict or TemporarilyUnavailable error accordingly,
    // and reset to 0 on any successful call to _root->work.
    size_t writeConflictsInARow = 0;
    size_t tempUnavailErrorsInARow = 0;

    // Capped insert data; declared outside the loop so we hold a shared pointer to the capped
    // insert notifier the entire time we are in the loop.  Holding a shared pointer to the
    // capped insert notifier is necessary for the notifierVersion to advance.
    auto notifier = makeNotifier();

    for (;;) {
        // These are the conditions which can cause us to yield:
        //   1) The yield policy's timer elapsed, or
        //   2) some stage requested a yield, or
        //   3) we need to yield and retry due to a WriteConflictException.
        // In all cases, the actual yielding happens here.

        const auto whileYieldingFn = [&]() {
            doYield(_opCtx);
        };

        if (_yieldPolicy->shouldYieldOrInterrupt(_opCtx)) {
            uassertStatusOK(_yieldPolicy->yieldOrInterrupt(_opCtx, whileYieldingFn));
        }

        WorkingSetID id = WorkingSet::INVALID_ID;
        PlanStage::StageState code = _root->work(&id);

        if (code != PlanStage::NEED_YIELD) {
            writeConflictsInARow = 0;
            tempUnavailErrorsInARow = 0;
        }

        if (PlanStage::ADVANCED == code) {
            WorkingSetMember* member = _workingSet->get(id);
            bool hasRequestedData = true;

            if (nullptr != objOut) {
                if (WorkingSetMember::RID_AND_IDX == member->getState()) {
                    if (1 != member->keyData.size()) {
                        _workingSet->free(id);
                        hasRequestedData = false;
                    } else {
                        // TODO: currently snapshot ids are only associated with documents, and
                        // not with index keys.
                        *objOut = Snapshotted<Document>(SnapshotId(),
                                                        Document{member->keyData[0].keyData});
                    }
                } else if (member->hasObj()) {
                    std::swap(*objOut, member->doc);
                } else {
                    _workingSet->free(id);
                    hasRequestedData = false;
                }
            }

            if (nullptr != dlOut) {
                tassert(6297500, "Working set member has no record ID", member->hasRecordId());
                *dlOut = std::move(member->recordId);
            }

            if (hasRequestedData) {
                // transfer the metadata from the WSM to Document.
                if (objOut) {
                    if (_mustReturnOwnedBson) {
                        objOut->value() = objOut->value().getOwned();
                    }

                    if (member->metadata()) {
                        MutableDocument md(std::move(objOut->value()));
                        md.setMetadata(member->releaseMetadata());
                        objOut->setValue(md.freeze());
                    }
                }
                _workingSet->free(id);
                return PlanExecutor::ADVANCED;
            }
            // This result didn't have the data the caller wanted, try again.

        } else if (PlanStage::NEED_YIELD == code) {
            _handleNeedYield(writeConflictsInARow, tempUnavailErrorsInARow);

        } else if (PlanStage::NEED_TIME == code) {
            // Fall through to yield check at end of large conditional.

        } else if (_handleEOFAndExit(code, notifier)) {
            return PlanExecutor::IS_EOF;
        }
    }
}

namespace {
BSONObj makeBsonWithMetadata(Document& doc, WorkingSetMember* member) {
    if (member->metadata()) {
        MutableDocument md(std::move(doc));
        md.setMetadata(member->releaseMetadata());
        return md.freeze().toBsonWithMetaData();
    }

    return doc.toBsonWithMetaData();
}
}  // namespace

std::unique_ptr<insert_listener::Notifier> PlanExecutorImpl::makeNotifier() {
    if (insert_listener::shouldListenForInserts(_opCtx, _cq.get())) {
        // We always construct the insert_listener::Notifier for awaitData cursors.
        return insert_listener::getCappedInsertNotifier(_opCtx, _nss, _yieldPolicy.get());
    }
    return nullptr;
}

void PlanExecutorImpl::_handleNeedYield(size_t& writeConflictsInARow,
                                        size_t& tempUnavailErrorsInARow) {
    invariant(shard_role_details::getRecoveryUnit(_opCtx));

    if (_expCtx->getTemporarilyUnavailableException()) {
        _expCtx->setTemporarilyUnavailableException(false);

        if (!_yieldPolicy->canAutoYield()) {
            throwTemporarilyUnavailableException(
                "got TemporarilyUnavailable exception on a plan that "
                "cannot "
                "auto-yield");
        }

        tempUnavailErrorsInARow++;
        handleTemporarilyUnavailableException(
            _opCtx,
            tempUnavailErrorsInARow,
            "plan executor",
            NamespaceStringOrUUID(_nss),
            ExceptionFor<ErrorCodes::TemporarilyUnavailable>(
                Status(ErrorCodes::TemporarilyUnavailable, "temporarily unavailable")),
            writeConflictsInARow);

    } else {
        // We're yielding because of a WriteConflictException.
        if (!_yieldPolicy->canAutoYield() ||
            MONGO_unlikely(skipWriteConflictRetries.shouldFail())) {
            throwWriteConflictException(
                "Write conflict during plan execution and yielding is "
                "disabled.");
        }

        CurOp::get(_opCtx)->debug().additiveMetrics.incrementWriteConflicts(1);
        writeConflictsInARow++;
        logWriteConflictAndBackoff(
            writeConflictsInARow, "plan execution", ""_sd, NamespaceStringOrUUID(_nss));
    }

    // Yield next time through the loop.
    invariant(_yieldPolicy->canAutoYield());
    _yieldPolicy->forceYield();
}

bool PlanExecutorImpl::_handleEOFAndExit(PlanStage::StageState code,
                                         std::unique_ptr<insert_listener::Notifier>& notifier) {
    invariant(PlanStage::IS_EOF == code);
    hangBeforeShouldWaitForInsertsIfFailpointEnabled(this);

    // The !notifier check is necessary because shouldWaitForInserts can return 'true' when
    // shouldListenForInserts returned 'false' (above) in the case of a deadline becoming
    // "unexpired" due to the system clock going backwards.
    if (!notifier ||
        !insert_listener::shouldWaitForInserts(_opCtx, _cq.get(), _yieldPolicy.get())) {
        // Time to exit.
        return true;
    }

    insert_listener::waitForInserts(_opCtx, _yieldPolicy.get(), notifier);
    return false;
}

size_t PlanExecutorImpl::getNextBatch(size_t batchSize, AppendBSONObjFn append) {
    const bool includeMetadata = _expCtx && _expCtx->needsMerge;
    if (batchSize == 0) {
        return 0;
    }

    checkFailPointPlanExecAlwaysFails();
    _checkIfKilled();

    const auto whileYieldingFn = [opCtx = _opCtx]() {
        return doYield(opCtx);
    };
    auto notifier = makeNotifier();

    WorkingSetID id = WorkingSet::INVALID_ID;
    WorkingSetMember* member;
    PlanStage::StageState code;

    // The below are incremented on every WriteConflict or TemporarilyUnavailable error
    // accordingly, and reset to 0 on any successful call to _root->work.
    size_t writeConflictsInARow = 0;
    size_t tempUnavailErrorsInARow = 0;

    size_t numResults = 0;
    BSONObj objOut;

    // Handle case where previous execution stashed a result.
    if (!_stash.empty()) {
        objOut = includeMetadata ? _stash.front().toBson() : _stash.front().toBsonWithMetaData();
        _stash.pop_front();
        append(objOut, getPostBatchResumeToken(), numResults);
        numResults++;
    }

    for (;;) {
        _checkIfMustYield(whileYieldingFn);

        code = _root->work(&id);

        if (code != PlanStage::NEED_YIELD) {
            writeConflictsInARow = 0;
            tempUnavailErrorsInARow = 0;
        }

        if (code == PlanStage::ADVANCED) {
            // Process working set member.
            member = _workingSet->get(id);
            if (MONGO_likely(member->hasObj())) {
                if (includeMetadata) {
                    objOut = makeBsonWithMetadata(member->doc.value(), member);
                } else {
                    objOut = member->doc.value().toBson();
                }

            } else if (member->keyData.size() >= 1) {
                if (includeMetadata) {
                    _docOutput = Document{member->keyData[0].keyData};
                    objOut = makeBsonWithMetadata(_docOutput, member);
                } else {
                    objOut = member->keyData[0].keyData;
                }

            } else {
                _workingSet->free(id);
                continue;  // Try to call work() again- we didn't get what we needed.
            }

            _workingSet->free(id);

            if (MONGO_unlikely(!append(objOut, getPostBatchResumeToken(), numResults))) {
                stashResult(objOut);
                break;
            }
            numResults++;

            // Only check if the query has been killed or if we've filled up the batch once a result
            // has been produced. Doing these checks every loop can impact the performace of queries
            // that repeatedly return NEED_TIME.
            if (MONGO_unlikely(numResults >= batchSize)) {
                break;
            }

            _checkIfKilled();

        } else if (code == PlanStage::NEED_YIELD) {
            _handleNeedYield(writeConflictsInARow, tempUnavailErrorsInARow);

        } else if (code == PlanStage::NEED_TIME) {
            // Do nothing except reset counters; need more time.

        } else if (_handleEOFAndExit(code, notifier)) {
            break;
        }
    }
    return numResults;
}

bool PlanExecutorImpl::isEOF() {
    invariant(_currentState == kUsable);
    return isMarkedAsKilled() || (_stash.empty() && _root->isEOF());
}

void PlanExecutorImpl::markAsKilled(Status killStatus) {
    invariant(!killStatus.isOK());
    // If killed multiple times, only retain the first status.
    if (_killStatus.isOK()) {
        _killStatus = killStatus;
    }
}

void PlanExecutorImpl::dispose(OperationContext* opCtx) {
    _currentState = kDisposed;
}

void PlanExecutorImpl::executeExhaustive() {
    // We don't check batch size or do anything with returned BSON in exhaustDoWork().
    checkFailPointPlanExecAlwaysFails();
    _checkIfKilled();

    const auto whileYieldingFn = [opCtx = _opCtx]() {
        return doYield(opCtx);
    };
    auto notifier = makeNotifier();

    WorkingSetID id = WorkingSet::INVALID_ID;
    PlanStage::StageState code;

    // The below are incremented on every WriteConflict or TemporarilyUnavailable error
    // accordingly, and reset to 0 on any successful call to _root->work.
    size_t writeConflictsInARow = 0;
    size_t tempUnavailErrorsInARow = 0;

    for (;;) {
        _checkIfMustYield(whileYieldingFn);

        code = _root->work(&id);

        if (code != PlanStage::NEED_YIELD) {
            writeConflictsInARow = 0;
            tempUnavailErrorsInARow = 0;
        }

        if (code == PlanStage::ADVANCED) {
            // Free WSM.
            _workingSet->free(id);

            // Only check if the query has been killed or if we've filled up the batch once a result
            // has been produced. Doing these checks every loop can impact the performace of queries
            // that repeatedly return NEED_TIME.
            _checkIfKilled();

        } else if (code == PlanStage::NEED_YIELD) {
            _handleNeedYield(writeConflictsInARow, tempUnavailErrorsInARow);

        } else if (code == PlanStage::NEED_TIME) {
            // Do nothing except reset counters; need more time.

        } else if (_handleEOFAndExit(code, notifier)) {
            break;
        }
    }
}

long long PlanExecutorImpl::executeCount() {
    invariant(_root->stageType() == StageType::STAGE_COUNT ||
              _root->stageType() == StageType::STAGE_RECORD_STORE_FAST_COUNT);

    executeExhaustive();
    auto countStats = static_cast<const CountStats*>(_root->getSpecificStats());
    return countStats->nCounted;
}

UpdateResult PlanExecutorImpl::executeUpdate() {
    executeExhaustive();
    return getUpdateResult();
}

UpdateResult PlanExecutorImpl::getUpdateResult() const {
    auto updateStatsToResult = [](const UpdateStats& updateStats,
                                  bool containsDotsAndDollarsField) -> UpdateResult {
        return UpdateResult(updateStats.nMatched > 0 /* Did we update at least one obj? */,
                            updateStats.isModUpdate /* Is this a $mod update? */,
                            updateStats.nModified /* number of modified docs, no no-ops */,
                            updateStats.nMatched /* # of docs matched/updated, even no-ops */,
                            updateStats.objInserted,
                            containsDotsAndDollarsField);
    };

    // If we're updating a non-existent collection, then the delete plan may have an EOF as the
    // root stage.
    if (_root->stageType() == STAGE_EOF) {
        const auto stats = std::make_unique<UpdateStats>();
        return updateStatsToResult(static_cast<const UpdateStats&>(*stats), false);
    }

    // If the collection exists, then we expect the root of the plan tree to either be an update
    // stage, or (for findAndModify) a projection stage wrapping an update / TS_MODIFY stage.
    const auto updateStage = [&] {
        switch (_root->stageType()) {
            case StageType::STAGE_PROJECTION_DEFAULT:
            case StageType::STAGE_PROJECTION_COVERED:
            case StageType::STAGE_PROJECTION_SIMPLE: {
                tassert(7314604,
                        "Unexpected number of children: {}"_format(_root->getChildren().size()),
                        _root->getChildren().size() == 1U);
                auto childStage = _root->child().get();
                tassert(7314605,
                        "Unexpected child stage type: {}"_format(childStage->stageType()),
                        StageType::STAGE_UPDATE == childStage->stageType() ||
                            StageType::STAGE_TIMESERIES_MODIFY == childStage->stageType());
                return childStage;
            }
            default:
                return _root.get();
        }
    }();
    switch (updateStage->stageType()) {
        case StageType::STAGE_TIMESERIES_MODIFY: {
            const auto& stats =
                static_cast<const TimeseriesModifyStats&>(*updateStage->getSpecificStats());
            return UpdateResult(
                stats.nMeasurementsModified > 0 /* Did we update at least one obj? */,
                stats.isModUpdate /* Is this a $mod update? */,
                stats.nMeasurementsModified /* number of modified docs, no no-ops */,
                stats.nMeasurementsMatched /* # of docs matched/updated, even no-ops */,
                stats.objInserted /* objInserted */,
                static_cast<TimeseriesModifyStage*>(updateStage)->containsDotsAndDollarsField());
        }
        case StageType::STAGE_UPDATE: {
            const auto& stats = static_cast<const UpdateStats&>(*updateStage->getSpecificStats());
            return updateStatsToResult(
                stats, static_cast<UpdateStage*>(updateStage)->containsDotsAndDollarsField());
        }
        default:
            MONGO_UNREACHABLE_TASSERT(7314606);
    }
}

long long PlanExecutorImpl::executeDelete() {
    executeExhaustive();
    return getDeleteResult();
}

long long PlanExecutorImpl::getDeleteResult() const {
    // If we're deleting from a non-existent collection, then the delete plan may have an EOF as
    // the root stage.
    if (_root->stageType() == STAGE_EOF) {
        return 0LL;
    }

    // If the collection exists, the delete plan may either have a delete stage at the root, or
    // (for findAndModify) a projection stage wrapping a delete / TS_MODIFY stage.
    const auto deleteStage = [&] {
        switch (_root->stageType()) {
            case StageType::STAGE_PROJECTION_DEFAULT:
            case StageType::STAGE_PROJECTION_COVERED:
            case StageType::STAGE_PROJECTION_SIMPLE: {
                tassert(7308302,
                        "Unexpected number of children: {}"_format(_root->getChildren().size()),
                        _root->getChildren().size() == 1U);
                auto childStage = _root->child().get();
                tassert(7308303,
                        "Unexpected child stage type: {}"_format(childStage->stageType()),
                        StageType::STAGE_DELETE == childStage->stageType() ||
                            StageType::STAGE_TIMESERIES_MODIFY == childStage->stageType());
                return childStage;
            }
            default:
                return _root.get();
        }
    }();
    switch (deleteStage->stageType()) {
        case StageType::STAGE_TIMESERIES_MODIFY: {
            const auto& tsModifyStats =
                static_cast<const TimeseriesModifyStats&>(*deleteStage->getSpecificStats());
            return tsModifyStats.nMeasurementsModified;
        }
        case StageType::STAGE_DELETE:
        case StageType::STAGE_BATCHED_DELETE: {
            const auto& deleteStats =
                static_cast<const DeleteStats&>(*deleteStage->getSpecificStats());
            return deleteStats.docsDeleted;
        }
        default:
            MONGO_UNREACHABLE_TASSERT(7308306);
    }
}

BatchedDeleteStats PlanExecutorImpl::getBatchedDeleteStats() {
    // If we're deleting on a non-existent collection, then the delete plan may have an EOF as the
    // root stage.
    if (_root->stageType() == STAGE_EOF) {
        return BatchedDeleteStats();
    }

    invariant(_root->stageType() == StageType::STAGE_BATCHED_DELETE);

    // If the collection exists, we expect the root of the plan tree to be a batched delete stage.
    // Note: findAndModify is incompatible with the batched delete stage so no need to handle
    // projection stage wrapping.
    const auto stats = _root->getSpecificStats();
    auto batchedStats = static_cast<const BatchedDeleteStats*>(stats);
    return *batchedStats;
}

void PlanExecutorImpl::stashResult(const BSONObj& obj) {
    _stash.push_front(Document{obj.getOwned()});
}

Status PlanExecutorImpl::getKillStatus() {
    invariant(isMarkedAsKilled());
    return _killStatus;
}

bool PlanExecutorImpl::isDisposed() const {
    return _currentState == kDisposed;
}

Timestamp PlanExecutorImpl::getLatestOplogTimestamp() const {
    return _collScanStage ? _collScanStage->getLatestOplogTimestamp() : Timestamp{};
}

BSONObj PlanExecutorImpl::getPostBatchResumeToken() const {
    static const BSONObj kEmptyPBRT;
    return _collScanStage ? _collScanStage->getPostBatchResumeToken() : kEmptyPBRT;
}

PlanExecutor::LockPolicy PlanExecutorImpl::lockPolicy() const {
    // If this PlanExecutor is simply unspooling queued data, then there is no need to acquire
    // locks.
    if (_root->stageType() == StageType::STAGE_QUEUED_DATA) {
        return LockPolicy::kLocksInternally;
    }

    return LockPolicy::kLockExternally;
}

const PlanExplainer& PlanExecutorImpl::getPlanExplainer() const {
    invariant(_planExplainer);
    return *_planExplainer;
}

MultiPlanStage* PlanExecutorImpl::getMultiPlanStage() const {
    PlanStage* ps = getStageByType(_root.get(), StageType::STAGE_MULTI_PLAN);
    invariant(ps == nullptr || ps->stageType() == StageType::STAGE_MULTI_PLAN);
    return static_cast<MultiPlanStage*>(ps);
}

bool PlanExecutorImpl::usesCollectionAcquisitions() const {
    return _yieldPolicy->usesCollectionAcquisitions();
}
}  // namespace mongo
