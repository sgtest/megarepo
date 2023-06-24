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

#include <utility>

#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/commands/test_commands_enabled.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/query/query_feature_flags_gen.h"
#include "mongo/db/query/query_knobs_gen.h"
#include "mongo/db/server_options.h"
#include "mongo/db/tenant_id.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/util/synchronized_value.h"

namespace mongo {

void QueryFrameworkControl::append(OperationContext*,
                                   BSONObjBuilder* b,
                                   StringData name,
                                   const boost::optional<TenantId>&) {
    *b << name << QueryFrameworkControl_serializer(_data.get());
}

Status QueryFrameworkControl::setFromString(StringData value, const boost::optional<TenantId>&) {
    auto newVal =
        QueryFrameworkControl_parse(IDLParserContext("internalQueryFrameworkControl"), value);

    // To enable Bonsai, the feature flag must be enabled. Here, we return an error to the user if
    // they try to set the framework control knob to use Bonsai while the feature flag is disabled.
    //
    // The feature flag should be initialized by this point because
    // server_options_detail::applySetParameterOptions(std::map ...)
    // handles setParameters in alphabetical order, so "feature" comes before "internal".

    switch (newVal) {
        case QueryFrameworkControlEnum::kForceClassicEngine:
        case QueryFrameworkControlEnum::kTrySbeEngine:
            break;
        case QueryFrameworkControlEnum::kTryBonsai:
            if (feature_flags::gFeatureFlagCommonQueryFramework.isEnabled(
                    serverGlobalParams.featureCompatibility)) {
                break;
            }
            return {ErrorCodes::IllegalOperation,
                    "featureFlagCommonQueryFramework must be enabled to run with tryBonsai"};
        case QueryFrameworkControlEnum::kTryBonsaiExperimental:
        case QueryFrameworkControlEnum::kForceBonsai:
            if (getTestCommandsEnabled()) {
                break;
            }
            return {
                ErrorCodes::IllegalOperation,
                "testCommands must be enabled to run with tryBonsaiExperimental or forceBonsai"};
    }

    _data = std::move(newVal);
    return Status::OK();
}

}  // namespace mongo
