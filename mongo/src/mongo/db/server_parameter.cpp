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

#include "mongo/db/server_parameter.h"

#include <boost/preprocessor/control/iif.hpp>
#include <fmt/format.h>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/bson/bsontypes.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/server_options.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/util/static_immortal.h"
#include "mongo/util/time_support.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kControl

namespace mongo {

using namespace fmt::literals;
using SPT = ServerParameterType;

MONGO_INITIALIZER_GROUP(BeginServerParameterRegistration, (), ("EndServerParameterRegistration"))
MONGO_INITIALIZER_GROUP(EndServerParameterRegistration,
                        ("BeginServerParameterRegistration"),
                        ("BeginStartupOptionHandling"))

ServerParameter::ServerParameter(StringData name, ServerParameterType spt)
    : _name{name}, _type(spt) {}

Status ServerParameter::set(const BSONElement& newValueElement,
                            const boost::optional<TenantId>& tenantId) {
    auto validateStatus = validate(newValueElement, tenantId);
    if (!validateStatus.isOK()) {
        return validateStatus;
    }

    auto swValue = _coerceToString(newValueElement);
    if (!swValue.isOK())
        return swValue.getStatus();
    return setFromString(swValue.getValue(), boost::none);
}

ServerParameterSet* ServerParameterSet::getNodeParameterSet() {
    static StaticImmortal obj = [] {
        ServerParameterSet sps;
        sps.setValidate([](ServerParameter* sp) {
            uassert(6225102,
                    "Registering cluster-wide parameter '{}' as node-local server parameter"
                    ""_format(sp->name()),
                    sp->isNodeLocal());
        });
        return sps;
    }();
    return &*obj;
}

bool ServerParameter::isEnabled() const {
    return isEnabledOnVersion(
        serverGlobalParams.featureCompatibility.isVersionInitialized()
            ? serverGlobalParams.featureCompatibility.getVersion()
            : multiversion::FeatureCompatibilityVersion::kUnsetDefaultLastLTSBehavior);
}

bool ServerParameter::isEnabledOnVersion(
    const multiversion::FeatureCompatibilityVersion& targetFCV) const {
    if (_disableState != DisableState::Enabled) {
        return false;
    }
    return _isEnabledOnVersion(targetFCV);
}

bool ServerParameter::canBeEnabledOnVersion(
    const multiversion::FeatureCompatibilityVersion& targetFCV) const {
    if (_disableState == DisableState::PermanentlyDisabled) {
        return false;
    }
    return _isEnabledOnVersion(targetFCV);
}

bool ServerParameter::_isEnabledOnVersion(
    const multiversion::FeatureCompatibilityVersion& targetFCV) const {
    return minFCVIsLessThanOrEqualToVersion(targetFCV) &&
        !featureFlagIsDisabledOnVersion(targetFCV);
}

bool ServerParameter::featureFlagIsDisabledOnVersion(
    const multiversion::FeatureCompatibilityVersion& targetFCV) const {
    return _featureFlag && !_featureFlag->isEnabledOnVersion(targetFCV);
}

ServerParameterSet* ServerParameterSet::getClusterParameterSet() {
    static StaticImmortal obj = [] {
        ServerParameterSet sps;
        sps.setValidate([](ServerParameter* sp) {
            uassert(6225103,
                    "Registering node-local parameter '{}' as cluster-wide server parameter"
                    ""_format(sp->name()),
                    sp->isClusterWide());
        });
        return sps;
    }();
    return &*obj;
}

void ServerParameterSet::add(ServerParameter* sp) {
    if (_validate)
        _validate(sp);
    auto [it, ok] = _map.insert({sp->name(), sp});
    uassert(23784, "Duplicate server parameter registration for '{}'"_format(sp->name()), ok);
}

StatusWith<std::string> ServerParameter::_coerceToString(const BSONElement& element) {
    switch (element.type()) {
        case NumberDouble:
            return std::to_string(element.Double());
        case String:
            return element.String();
        case NumberInt:
            return std::to_string(element.Int());
        case NumberLong:
            return std::to_string(element.Long());
        case Date:
            return dateToISOStringLocal(element.Date());
        default:
            std::string diag;
            if (isRedact()) {
                diag = "###";
            } else {
                diag = element.toString();
            }
            return {ErrorCodes::BadValue,
                    str::stream() << "Unsupported type " << typeName(element.type()) << " (value: '"
                                  << diag << "') for setParameter: " << name()};
    }
}

void ServerParameterSet::remove(const std::string& name) {
    invariant(1 == _map.erase(name), "Failed to erase key \"{}\""_format(name));
}

IDLServerParameterDeprecatedAlias::IDLServerParameterDeprecatedAlias(StringData name,
                                                                     ServerParameter* sp)
    : ServerParameter(name, sp->getServerParameterType()), _sp(sp) {
    if (_sp->isTestOnly()) {
        setTestOnly();
    }
}

void IDLServerParameterDeprecatedAlias::append(OperationContext* opCtx,
                                               BSONObjBuilder* b,
                                               StringData fieldName,
                                               const boost::optional<TenantId>& tenantId) {
    std::call_once(_warnOnce, [&] {
        LOGV2_WARNING(636300,
                      "Use of deprecated server parameter name",
                      "deprecatedName"_attr = name(),
                      "canonicalName"_attr = _sp->name());
    });
    _sp->append(opCtx, b, fieldName, tenantId);
}

Status IDLServerParameterDeprecatedAlias::reset(const boost::optional<TenantId>& tenantId) {
    std::call_once(_warnOnce, [&] {
        LOGV2_WARNING(636301,
                      "Use of deprecated server parameter name",
                      "deprecatedName"_attr = name(),
                      "canonicalName"_attr = _sp->name());
    });
    return _sp->reset(tenantId);
}

Status IDLServerParameterDeprecatedAlias::set(const BSONElement& newValueElement,
                                              const boost::optional<TenantId>& tenantId) {
    std::call_once(_warnOnce, [&] {
        LOGV2_WARNING(636302,
                      "Use of deprecated server parameter name",
                      "deprecatedName"_attr = name(),
                      "canonicalName"_attr = _sp->name());
    });
    return _sp->set(newValueElement, tenantId);
}

Status IDLServerParameterDeprecatedAlias::setFromString(StringData str,
                                                        const boost::optional<TenantId>& tenantId) {
    std::call_once(_warnOnce, [&] {
        LOGV2_WARNING(636303,
                      "Use of deprecated server parameter name",
                      "deprecatedName"_attr = name(),
                      "canonicalName"_attr = _sp->name());
    });
    return _sp->setFromString(str, tenantId);
}

void ServerParameterSet::disableTestParameters() {
    for (auto& spit : _map) {
        auto*& sp = spit.second;
        if (sp->isTestOnly()) {
            sp->disable(true /* permanent */);
        }
    }
}

void registerServerParameter(ServerParameter* p) {
    ServerParameterSet::getParameterSet(p->getServerParameterType())->add(p);
}

}  // namespace mongo
