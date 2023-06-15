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

#include <type_traits>

#include <grpcpp/security/server_credentials.h>
#include <grpcpp/support/status_code_enum.h>

#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"

namespace mongo::transport::grpc::util {

/**
 * Parse a PEM-encoded file that contains a single certificate and its associated private key
 * into a PemKeyCertPair.
 */
::grpc::SslServerCredentialsOptions::PemKeyCertPair parsePEMKeyFile(StringData filePath);

/**
 * Converts a gRPC status code into its corresponding MongoDB error code.
 */
ErrorCodes::Error statusToErrorCode(::grpc::StatusCode statusCode);

/**
 * Converts a MongoDB error code into its corresponding gRPC status code.
 * Note that the mapping between gRPC status codes and MongoDB errors codes is not 1 to 1, so the
 * following does not have to evaluate to true:
 * `errorToStatusCode(statusToErrorCode(sc)) == sc`
 */
::grpc::StatusCode errorToStatusCode(ErrorCodes::Error errorCode);

/**
 * Converts a MongoDB status to its gRPC counterpart, and vice versa.
 * Prefer using this over direct invocations of `errorToStatusCode` and `statusToErrorCode`.
 */
template <typename StatusType>
inline auto convertStatus(StatusType status) {
    if constexpr (std::is_same<StatusType, Status>::value) {
        return ::grpc::Status(errorToStatusCode(status.code()), status.reason());
    } else {
        static_assert(std::is_same<StatusType, ::grpc::Status>::value == true);
        return Status(statusToErrorCode(status.error_code()), status.error_message());
    }
}

}  // namespace mongo::transport::grpc::util
