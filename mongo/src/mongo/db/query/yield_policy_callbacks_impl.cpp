/**
 *    Copyright (C) 2021-present MongoDB, Inc.
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

#include <string>
#include <utility>

#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/db/curop.h"
#include "mongo/db/curop_failpoint_helpers.h"
#include "mongo/db/query/yield_policy_callbacks_impl.h"
#include "mongo/platform/compiler.h"
#include "mongo/util/duration.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/namespace_string_util.h"
#include "mongo/util/time_support.h"

namespace mongo {
namespace {
MONGO_FAIL_POINT_DEFINE(setInterruptOnlyPlansCheckForInterruptHang);
MONGO_FAIL_POINT_DEFINE(setYieldAllLocksHang);
MONGO_FAIL_POINT_DEFINE(setYieldAllLocksHangSecond);
MONGO_FAIL_POINT_DEFINE(setYieldAllLocksWait);
}  // namespace

YieldPolicyCallbacksImpl::YieldPolicyCallbacksImpl(NamespaceString nssForFailpoints)
    : _nss(std::move(nssForFailpoints)) {}

void YieldPolicyCallbacksImpl::duringYield(OperationContext* opCtx) const {
    CurOp::get(opCtx)->yielded();

    const auto& nss = _nss;
    auto failPointHang = [opCtx, nss](FailPoint* fp) {
        fp->executeIf(
            [opCtx, fp](const BSONObj& config) {
                fp->pauseWhileSet();

                if (config.getField("checkForInterruptAfterHang").trueValue()) {
                    // Throws.
                    opCtx->checkForInterrupt();
                }
            },
            [nss](const BSONObj& config) {
                const auto fpNss = NamespaceStringUtil::parseFailPointData(config, "namespace");
                return fpNss.isEmpty() || fpNss == nss;
            });
    };
    failPointHang(&setYieldAllLocksHang);
    failPointHang(&setYieldAllLocksHangSecond);

    setYieldAllLocksWait.executeIf(
        [&](const BSONObj& data) { sleepFor(Milliseconds(data["waitForMillis"].numberInt())); },
        [&](const BSONObj& config) {
            const auto fpNss = NamespaceStringUtil::parseFailPointData(config, "namespace");
            return fpNss.isEmpty() || _nss == fpNss;
        });
}

void YieldPolicyCallbacksImpl::handledWriteConflict(OperationContext* opCtx) const {
    CurOp::get(opCtx)->debug().additiveMetrics.incrementWriteConflicts(1);
}

void YieldPolicyCallbacksImpl::preCheckInterruptOnly(OperationContext* opCtx) const {
    // If the 'setInterruptOnlyPlansCheckForInterruptHang' fail point is enabled, set the
    // 'failPointMsg' field of this operation's CurOp to signal that we've hit this point.
    if (MONGO_unlikely(setInterruptOnlyPlansCheckForInterruptHang.shouldFail())) {
        CurOpFailpointHelpers::waitWhileFailPointEnabled(
            &setInterruptOnlyPlansCheckForInterruptHang,
            opCtx,
            "setInterruptOnlyPlansCheckForInterruptHang");
    }
}

}  // namespace mongo
