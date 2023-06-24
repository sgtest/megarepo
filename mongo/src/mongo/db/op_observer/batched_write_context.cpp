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

#include "mongo/db/op_observer/batched_write_context.h"

#include <utility>

#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>

#include "mongo/db/concurrency/locker.h"
#include "mongo/db/repl/oplog_entry.h"
#include "mongo/db/repl/oplog_entry_gen.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/decorable.h"

namespace mongo {
const OperationContext::Decoration<BatchedWriteContext> BatchedWriteContext::get =
    OperationContext::declareDecoration<BatchedWriteContext>();

BatchedWriteContext::BatchedWriteContext() {}

void BatchedWriteContext::addBatchedOperation(OperationContext* opCtx,
                                              const BatchedOperation& operation) {
    invariant(_batchWrites);

    // Current support is only limited to insert update and delete operations, no change stream
    // pre-images, no multi-doc transactions, no retryable writes.
    invariant(operation.getOpType() == repl::OpTypeEnum::kDelete ||
              operation.getOpType() == repl::OpTypeEnum::kInsert ||
              operation.getOpType() == repl::OpTypeEnum::kUpdate);
    invariant(operation.getChangeStreamPreImageRecordingMode() ==
              repl::ReplOperation::ChangeStreamPreImageRecordingMode::kOff);
    invariant(!opCtx->inMultiDocumentTransaction());
    invariant(!opCtx->getTxnNumber());
    invariant(opCtx->lockState()->inAWriteUnitOfWork());

    invariantStatusOK(_batchedOperations.addOperation(operation));
}

TransactionOperations* BatchedWriteContext::getBatchedOperations(OperationContext* opCtx) {
    invariant(_batchWrites);
    return &_batchedOperations;
}

void BatchedWriteContext::clearBatchedOperations(OperationContext* opCtx) {
    _batchedOperations.clear();
}

bool BatchedWriteContext::writesAreBatched() const {
    return _batchWrites;
}
void BatchedWriteContext::setWritesAreBatched(bool batched) {
    _batchWrites = batched;
}

}  // namespace mongo
