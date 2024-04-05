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

#include <memory>
#include <utility>

#include "mongo/base/error_codes.h"
#include "mongo/base/error_extra_info.h"
#include "mongo/base/status.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"

namespace mongo {

/**
 * This class wraps an error originally thrown when a transaction participant shard fails when
 * unyielding its resources after processing remote responses. This allows distinguishing between a
 * local error versus a remote error, which is important for transaction machinery to correctly
 * handle the error.
 */
class TransactionParticipantFailedUnyieldInfo final : public ErrorExtraInfo {
public:
    static constexpr auto code = ErrorCodes::TransactionParticipantFailedUnyield;
    static constexpr StringData kOriginalErrorFieldName = "originalError"_sd;

    TransactionParticipantFailedUnyieldInfo(const Status& originalError)
        : _originalError(originalError) {}

    const auto& getOriginalError() const {
        return _originalError;
    }

    void serialize(BSONObjBuilder* bob) const final;
    static std::shared_ptr<const ErrorExtraInfo> parse(const BSONObj& obj);
    static TransactionParticipantFailedUnyieldInfo parseFromCommandError(const BSONObj& obj);

private:
    Status _originalError;
};

}  // namespace mongo
