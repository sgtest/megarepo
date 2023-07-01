/**
 *    Copyright (C) 2019-present MongoDB, Inc.
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

#include <memory>
#include <string>

#include "mongo/base/shim.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/s/transaction_coordinator_worker_curop_repository.h"
#include "mongo/db/session/logical_session_id.h"
#include "mongo/db/session/logical_session_id_gen.h"

namespace mongo {
namespace {

class NoOpTransactionCoordinatorWorkerCurOpRepository final
    : public TransactionCoordinatorWorkerCurOpRepository {
public:
    NoOpTransactionCoordinatorWorkerCurOpRepository() {}

    void set(OperationContext* opCtx,
             const LogicalSessionId& lsid,
             const TxnNumberAndRetryCounter TxnNumberAndRetryCounter,
             const CoordinatorAction action) override {}

    void reportState(OperationContext* opCtx, BSONObjBuilder* parent) const override {}
};

const auto _transactionCoordinatorWorkerCurOpRepository =
    std::make_shared<NoOpTransactionCoordinatorWorkerCurOpRepository>();

std::shared_ptr<TransactionCoordinatorWorkerCurOpRepository>
getTransactionCoordinatorWorkerCurOpRepositoryImpl() {
    return _transactionCoordinatorWorkerCurOpRepository;
}

auto getTransactionCoordinatorWorkerCurOpRepositoryRegistration =
    MONGO_WEAK_FUNCTION_REGISTRATION(getTransactionCoordinatorWorkerCurOpRepository,
                                     getTransactionCoordinatorWorkerCurOpRepositoryImpl);

}  // namespace
}  // namespace mongo
