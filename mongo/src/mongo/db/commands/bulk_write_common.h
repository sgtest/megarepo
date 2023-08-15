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

#pragma once

#include <cstddef>
#include <cstdint>
#include <vector>

#include "mongo/bson/bsonobj.h"
#include "mongo/db/auth/privilege.h"
#include "mongo/db/commands/bulk_write_gen.h"
#include "mongo/db/commands/bulk_write_parser.h"

/**
 * Contains common functionality shared between the bulkWrite command in mongos and mongod.
 */

namespace mongo {
namespace bulk_write_common {

/**
 * Validates the given bulkWrite command request and throws if the request is malformed.
 */
void validateRequest(const BulkWriteCommandRequest& req);

/**
 * Get the privileges needed to perform the given bulkWrite command.
 */
std::vector<Privilege> getPrivileges(const BulkWriteCommandRequest& req);

/**
 * Get the statement ID for an operation within a bulkWrite command, taking into consideration
 * whether the stmtId / stmtIds fields are present on the request.
 */
int32_t getStatementId(const BulkWriteCommandRequest& req, size_t currentOpIdx);

/**
 * From a serialized BulkWriteCommandRequest containing a single NamespaceInfoEntry,
 * extract that NamespaceInfoEntry. For bulkWrite with queryable encryption.
 */
NamespaceInfoEntry getFLENamespaceInfoEntry(const BSONObj& bulkWrite);

/**
 * Helper for FLE support. Build a InsertCommandRequest from a BulkWriteCommandRequest.
 */
write_ops::InsertCommandRequest makeInsertCommandRequestForFLE(
    const std::vector<mongo::BSONObj>& documents,
    const BulkWriteCommandRequest& req,
    const mongo::NamespaceInfoEntry& nsInfoEntry);

/**
 * Helper for FLE support. Build a UpdateCommandRequest from a BulkWriteUpdateOp.
 */
write_ops::UpdateCommandRequest makeUpdateCommandRequestForFLE(
    OperationContext* opCtx,
    const BulkWriteUpdateOp* op,
    const BulkWriteCommandRequest& req,
    const mongo::NamespaceInfoEntry& nsInfoEntry);

/**
 * Helper for FLE support. Build a DeleteCommandRequest from a BulkWriteDeleteOp.
 */
write_ops::DeleteCommandRequest makeDeleteCommandRequestForFLE(
    OperationContext* opCtx,
    const BulkWriteDeleteOp* op,
    const BulkWriteCommandRequest& req,
    const mongo::NamespaceInfoEntry& nsInfoEntry);
}  // namespace bulk_write_common
}  // namespace mongo
