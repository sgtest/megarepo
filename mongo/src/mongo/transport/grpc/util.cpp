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

#include <fmt/format.h>

#include "mongo/transport/grpc/util.h"

#include "mongo/util/assert_util.h"
#include "mongo/util/net/ssl_util.h"
#include "mongo/util/testing_proctor.h"

using namespace fmt::literals;

namespace mongo::transport::grpc::util {

::grpc::SslServerCredentialsOptions::PemKeyCertPair parsePEMKeyFile(StringData filePath) {

    ::grpc::SslServerCredentialsOptions::PemKeyCertPair certPair;

    auto certificateKeyFileContents = uassertStatusOK(ssl_util::readPEMFile(filePath));
    certPair.cert_chain = certificateKeyFileContents;
    certPair.private_key = certificateKeyFileContents;

    return certPair;
}

ErrorCodes::Error statusToErrorCode(::grpc::StatusCode statusCode) {
    switch (statusCode) {
        case ::grpc::OK:
            return ErrorCodes::OK;
        case ::grpc::UNAUTHENTICATED:
            return ErrorCodes::AuthenticationFailed;
        case ::grpc::CANCELLED:
            return ErrorCodes::CallbackCanceled;
        case ::grpc::INVALID_ARGUMENT:
            return ErrorCodes::BadValue;
        case ::grpc::DEADLINE_EXCEEDED:
            return ErrorCodes::ExceededTimeLimit;
        case ::grpc::FAILED_PRECONDITION:
            return ErrorCodes::RPCProtocolNegotiationFailed;
        case ::grpc::UNIMPLEMENTED:
            return ErrorCodes::NotImplemented;
        case ::grpc::INTERNAL:
            return ErrorCodes::InternalError;
        case ::grpc::UNAVAILABLE:
            return ErrorCodes::HostUnreachable;
        case ::grpc::PERMISSION_DENIED:
            return ErrorCodes::Unauthorized;
        case ::grpc::RESOURCE_EXHAUSTED:
            return ErrorCodes::ResourceExhausted;
        default:
            return ErrorCodes::UnknownError;
    }
}

::grpc::StatusCode errorToStatusCode(ErrorCodes::Error errorCode) {
    switch (errorCode) {
        case ErrorCodes::OK:
            return ::grpc::OK;
        case ErrorCodes::UnknownError:
            return ::grpc::UNKNOWN;
        case ErrorCodes::InterruptedAtShutdown:
        case ErrorCodes::ShutdownInProgress:
            return ::grpc::UNAVAILABLE;
        case ErrorCodes::CallbackCanceled:
        case ErrorCodes::ClientMarkedKilled:
            return ::grpc::CANCELLED;
        default:
            invariant(TestingProctor::instance().isEnabled(),
                      "No known conversion for MongoDB error code: "_format(errorCode));
            return ::grpc::UNKNOWN;
    }
}

}  // namespace mongo::transport::grpc::util
