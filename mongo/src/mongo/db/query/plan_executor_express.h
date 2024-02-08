/**
 *    Copyright (C) 2024-present MongoDB, Inc.
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

#include "mongo/db/exec/shard_filterer_impl.h"
#include "mongo/db/query/plan_executor.h"
#include "mongo/db/query/plan_explainer_express.h"


namespace mongo {

class PlanExecutorExpress final : public PlanExecutor {
public:
    PlanExecutorExpress(OperationContext* opCtx,
                        std::unique_ptr<CanonicalQuery> cq,
                        VariantCollectionPtrOrAcquisition coll,
                        boost::optional<ScopedCollectionFilter> collectionFilter,
                        bool isClusteredOnId);

    ExecState getNext(BSONObj* out, RecordId* dlOut) override;

    ExecState getNextDocument(Document* objOut, RecordId* dlOut) override {
        BSONObj bsonDoc;
        auto state = getNext(&bsonDoc, dlOut);
        *objOut = Document(bsonDoc);
        return state;
    }

    CanonicalQuery* getCanonicalQuery() const override {
        return _cq.get();
    }

    Pipeline* getPipeline() const override {
        MONGO_UNREACHABLE_TASSERT(8375801);
    }

    const NamespaceString& nss() const override {
        return _nss;
    }

    const std::vector<NamespaceStringOrUUID>& getSecondaryNamespaces() const override {
        return _secondaryNss;
    }

    OperationContext* getOpCtx() const override {
        return _opCtx;
    }

    void saveState() override {}

    void restoreState(const RestoreContext& context) override {
        _coll = context.collection();
    }

    void detachFromOperationContext() override {
        _opCtx = nullptr;
    }

    void reattachToOperationContext(OperationContext* opCtx) override {
        _opCtx = opCtx;
    }

    Timestamp getLatestOplogTimestamp() const override {
        return {};
    }

    BSONObj getPostBatchResumeToken() const override {
        return {};
    }

    LockPolicy lockPolicy() const override {
        return LockPolicy::kLockExternally;
    }

    const PlanExplainer& getPlanExplainer() const override {
        return _planExplainer;
    }

    void enableSaveRecoveryUnitAcrossCommandsIfSupported() override {}

    bool isSaveRecoveryUnitAcrossCommandsEnabled() const override {
        return false;
    }

    QueryFramework getQueryFramework() const override {
        return PlanExecutor::QueryFramework::kClassicOnly;
    }

    bool usesCollectionAcquisitions() const override {
        return _coll.isAcquisition();
    }

    bool isEOF() override {
        return _done || isMarkedAsKilled();
    }

    long long executeCount() override {
        MONGO_UNREACHABLE_TASSERT(8375802);
    }

    UpdateResult executeUpdate() override {
        MONGO_UNREACHABLE_TASSERT(8375803);
    }

    UpdateResult getUpdateResult() const override {
        MONGO_UNREACHABLE_TASSERT(8375804);
    }

    long long executeDelete() override {
        MONGO_UNREACHABLE_TASSERT(8375805);
    }

    long long getDeleteResult() const override {
        MONGO_UNREACHABLE_TASSERT(8375806);
    }

    BatchedDeleteStats getBatchedDeleteStats() override {
        MONGO_UNREACHABLE_TASSERT(8375807);
    }

    void markAsKilled(Status killStatus) override {
        invariant(!killStatus.isOK());
        if (_killStatus.isOK()) {
            _killStatus = killStatus;
        }
    }

    void dispose(OperationContext* opCtx) override {
        _isDisposed = true;
    }

    void stashResult(const BSONObj& obj) override {
        MONGO_UNREACHABLE_TASSERT(8375808);
    }

    bool isMarkedAsKilled() const override {
        return !_killStatus.isOK();
    }

    Status getKillStatus() override {
        invariant(isMarkedAsKilled());
        return _killStatus;
    }

    bool isDisposed() const override {
        return _isDisposed;
    }

    const mongo::CommonStats* getCommonStats() const {
        return &_commonStats;
    }

private:
    /**
     * Fast path for finding a document by _id.
     *
     * Will invariant() if no _id index or collection clustered by id is present.
     *
     * Returns true if doc is found, false otherwise.
     */
    bool findById(const BSONObj& query, BSONObj& result, RecordId* ridOut = nullptr) const;

    OperationContext* _opCtx;
    std::unique_ptr<CanonicalQuery> _cq;
    bool _isDisposed{false};
    bool _done{false};
    const bool _isClusteredOnId;
    VariantCollectionPtrOrAcquisition _coll;
    mongo::CommonStats _commonStats;
    const NamespaceString _nss;
    Status _killStatus = Status::OK();
    PlanExplainerExpress _planExplainer;
    std::vector<NamespaceStringOrUUID> _secondaryNss;
    boost::optional<ShardFiltererImpl> _shardFilterer;
};

}  // namespace mongo
