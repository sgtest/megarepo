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

#include "mongo/util/namespace_string_util.h"

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional.hpp>
#include <utility>

#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/oid.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/multitenancy_gen.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/server_feature_flags_gen.h"
#include "mongo/db/server_options.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/str.h"

namespace mongo {

std::string NamespaceStringUtil::serialize(const NamespaceString& ns,
                                           const SerializationContext& context) {
    if (!gMultitenancySupport)
        return ns.toString();

    switch (context.getSource()) {
        case SerializationContext::Source::AuthPrevalidated:
            return serializeForAuthPrevalidated(ns, context);
        case SerializationContext::Source::Command:
            if (context.getCallerType() == SerializationContext::CallerType::Reply) {
                return serializeForCommands(ns, context);
            }
            [[fallthrough]];
        case SerializationContext::Source::Storage:
        case SerializationContext::Source::Catalog:
        case SerializationContext::Source::Default:
            // Use forStorage as the default serializing rule
            return serializeForStorage(ns, context);
        default:
            MONGO_UNREACHABLE;
    }
}

std::string NamespaceStringUtil::serialize(const NamespaceString& ns,
                                           const SerializationOptions& options,
                                           const SerializationContext& context) {
    return options.serializeIdentifier(serialize(ns, context));
}

std::string NamespaceStringUtil::serializeForAuthPrevalidated(const NamespaceString& ns,
                                                              const SerializationContext& context) {
    // We want everything in the NamespaceString (tenantId, db, coll) to be present in the
    // serialized output to prevent loss of information in the prevalidated context.
    return ns.toStringWithTenantId();
}

std::string NamespaceStringUtil::serializeForCatalog(const NamespaceString& ns) {
    return ns.toStringWithTenantId();
}

std::string NamespaceStringUtil::serializeForStorage(const NamespaceString& ns,
                                                     const SerializationContext& context) {
    if (context.getSource() == SerializationContext::Source::Catalog) {
        // always return prefixed namespace for catalog.
        return ns.toStringWithTenantId();
    }

    if (gFeatureFlagRequireTenantID.isEnabled(serverGlobalParams.featureCompatibility)) {
        return ns.toString();
    }
    return ns.toStringWithTenantId();
}

std::string NamespaceStringUtil::serializeForCommands(const NamespaceString& ns,
                                                      const SerializationContext& context) {
    // tenantId came from either a $tenant field or security token.
    if (context.receivedNonPrefixedTenantId()) {
        switch (context.getPrefix()) {
            case SerializationContext::Prefix::ExcludePrefix:
                // fallthrough
            case SerializationContext::Prefix::Default:
                return ns.toString();
            case SerializationContext::Prefix::IncludePrefix:
                return ns.toStringWithTenantId();
            default:
                MONGO_UNREACHABLE;
        }
    }

    // tenantId came from the prefix.
    switch (context.getPrefix()) {
        case SerializationContext::Prefix::ExcludePrefix:
            return ns.toString();
        case SerializationContext::Prefix::Default:
            // fallthrough
        case SerializationContext::Prefix::IncludePrefix:
            return ns.toStringWithTenantId();
        default:
            MONGO_UNREACHABLE;
    }
}

NamespaceString NamespaceStringUtil::deserialize(boost::optional<TenantId> tenantId,
                                                 StringData ns,
                                                 const SerializationContext& context) {
    if (!gMultitenancySupport) {
        massert(6972102,
                str::stream() << "TenantId must not be set, but it is: " << tenantId->toString(),
                tenantId == boost::none);
        return NamespaceString(boost::none, ns);
    }

    if (ns.empty()) {
        return NamespaceString(tenantId, ns);
    }

    switch (context.getSource()) {
        case SerializationContext::Source::AuthPrevalidated:
            return deserializeForAuthPrevalidated(std::move(tenantId), ns, context);
        case SerializationContext::Source::Command:
            if (context.getCallerType() == SerializationContext::CallerType::Request) {
                return deserializeForCommands(std::move(tenantId), ns, context);
            }
            [[fallthrough]];
        case SerializationContext::Source::Storage:
        case SerializationContext::Source::Catalog:
        case SerializationContext::Source::Default:
            // Use forStorage as the default deserializing rule
            return deserializeForStorage(std::move(tenantId), ns, context);
        default:
            MONGO_UNREACHABLE;
    }
}

NamespaceString NamespaceStringUtil::deserialize(const DatabaseName& dbName, StringData coll) {
    // TODO SERVER-78534: if gMultitenancySupport is false, create a NamespaceString object
    // directly. Otherwise, check the tenant id, db name and collection name before creating a
    // NamespaceString object, because We allow only specific global internal collections to be
    // created without a tenantId.
    return NamespaceString{dbName, coll};
}

NamespaceString NamespaceStringUtil::deserializeForAuthPrevalidated(
    boost::optional<TenantId> tenantId, StringData ns, const SerializationContext& context) {
    if (context.shouldExpectTenantPrefixForAuth()) {
        // If there is a tenantId, expect that it's included in the ns string, and that the tenantId
        // field passed will be empty.
        uassert(7489601, "TenantId must not be set, but it is", tenantId == boost::none);
        return parseFromStringExpectTenantIdInMultitenancyMode(ns);
    }
    // In the prevalidated context, we are passing in validated and correct values, so skip
    // checks.
    return NamespaceString(std::move(tenantId), ns);
}

NamespaceString NamespaceStringUtil::deserializeForStorage(boost::optional<TenantId> tenantId,
                                                           StringData ns,
                                                           const SerializationContext& context) {
    if (gFeatureFlagRequireTenantID.isEnabled(serverGlobalParams.featureCompatibility)) {
        StringData dbName = ns.substr(0, ns.find('.'));
        if (!(dbName == DatabaseName::kAdmin.db()) && !(dbName == DatabaseName::kLocal.db()) &&
            !(dbName == DatabaseName::kConfig.db())) {
            massert(6972100,
                    str::stream() << "TenantId must be set on nss " << ns,
                    tenantId != boost::none);
        }
        return NamespaceString(std::move(tenantId), ns);
    }

    auto nss = parseFromStringExpectTenantIdInMultitenancyMode(ns);
    // TenantId could be prefixed, or passed in separately (or both) and namespace is always
    // constructed with the tenantId separately.
    if (tenantId != boost::none) {
        if (!nss.tenantId()) {
            return NamespaceString(std::move(tenantId), ns);
        }
        massert(6972101,
                str::stream() << "TenantId must match the db prefix tenantId: "
                              << tenantId->toString() << " prefix " << nss.tenantId()->toString(),
                tenantId == nss.tenantId());
    }

    return nss;
}

NamespaceString NamespaceStringUtil::deserializeForCommands(boost::optional<TenantId> tenantId,
                                                            StringData ns,
                                                            const SerializationContext& context) {
    // we only get here if we are processing a Command Request.  We disregard the feature flag
    // in this case, essentially letting the request dictate the state of the feature.

    // We received a tenantId from $tenant or the security token.
    if (tenantId != boost::none && context.receivedNonPrefixedTenantId()) {
        switch (context.getPrefix()) {
            case SerializationContext::Prefix::ExcludePrefix:
                // fallthrough
            case SerializationContext::Prefix::Default:
                return NamespaceString(std::move(tenantId), ns);
            case SerializationContext::Prefix::IncludePrefix: {
                auto nss = parseFromStringExpectTenantIdInMultitenancyMode(ns);
                massert(8423385,
                        str::stream() << "TenantId from $tenant or security token present as '"
                                      << tenantId->toString()
                                      << "' with expectPrefix field set but without a prefix set",
                        nss.tenantId());
                massert(8423381,
                        str::stream()
                            << "TenantId from $tenant or security token must match prefixed "
                               "tenantId: "
                            << tenantId->toString() << " prefix " << nss.tenantId()->toString(),
                        tenantId.value() == nss.tenantId());
                return nss;
            }
            default:
                MONGO_UNREACHABLE;
        }
    }

    // We received the tenantId from the prefix.
    auto nss = parseFromStringExpectTenantIdInMultitenancyMode(ns);
    if ((nss.dbName() != DatabaseName::kAdmin) && (nss.dbName() != DatabaseName::kLocal) &&
        (nss.dbName() != DatabaseName::kConfig)) {
        massert(8423387,
                str::stream() << "TenantId must be set on nss " << ns,
                nss.tenantId() != boost::none);
    }

    return nss;
}

NamespaceString NamespaceStringUtil::deserialize(const boost::optional<TenantId>& tenantId,
                                                 StringData db,
                                                 StringData coll,
                                                 const SerializationContext& context) {
    if (coll.empty())
        return deserialize(tenantId, db, context);

    // TODO SERVER-80361: Pass both StringDatas down to nss constructor to make this more performant
    return deserialize(tenantId, str::stream() << db << "." << coll, context);
}

NamespaceString NamespaceStringUtil::parseFromStringExpectTenantIdInMultitenancyMode(
    StringData ns) {

    if (!gMultitenancySupport) {
        return NamespaceString(boost::none, ns);
    }

    const auto tenantDelim = ns.find('_');
    const auto collDelim = ns.find('.');

    // If the first '_' is after the '.' that separates the db and coll names, the '_' is part
    // of the coll name and is not a db prefix.
    if (tenantDelim == std::string::npos || collDelim < tenantDelim) {
        return NamespaceString(boost::none, ns);
    }

    auto swOID = OID::parse(ns.substr(0, tenantDelim));
    if (!swOID.getStatus().isOK()) {
        // If we fail to parse an OID, either the size of the substring is incorrect, or there
        // is an invalid character. This indicates that the db has the "_" character, but it
        // does not act as a delimeter for a tenantId prefix.
        return NamespaceString(boost::none, ns);
    }

    const TenantId tenantId(swOID.getValue());
    return NamespaceString(tenantId, ns.substr(tenantDelim + 1, ns.size() - 1 - tenantDelim));
}

NamespaceString NamespaceStringUtil::parseFailPointData(const BSONObj& data,
                                                        StringData nsFieldName) {
    const auto ns = data.getStringField(nsFieldName);
    const auto tenantField = data.getField("$tenant");
    const auto tenantId = tenantField.ok()
        ? boost::optional<TenantId>(TenantId::parseFromBSON(tenantField))
        : boost::none;
    return NamespaceStringUtil::deserialize(tenantId, ns);
}

NamespaceString NamespaceStringUtil::deserializeForErrorMsg(StringData nsInErrMsg) {
    // TenantId always prefix in the error message. This method returns either (tenantId,
    // nonPrefixedDb) or (none, prefixedDb) depending on gMultitenancySupport flag.
    return NamespaceStringUtil::parseFromStringExpectTenantIdInMultitenancyMode(nsInErrMsg);
}

}  // namespace mongo
