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


#include <absl/container/node_hash_set.h>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional.hpp>
#include <boost/optional/optional.hpp>
#include <fmt/format.h>
#include <set>
#include <string>
#include <type_traits>
#include <utility>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/db/auth/address_restriction.h"
#include "mongo/db/auth/auth_name.h"
#include "mongo/db/auth/auth_types_gen.h"
#include "mongo/db/auth/authz_manager_external_state_local.h"
#include "mongo/db/auth/builtin_roles.h"
#include "mongo/db/auth/parsed_privilege_gen.h"
#include "mongo/db/auth/privilege.h"
#include "mongo/db/auth/restriction_set.h"
#include "mongo/db/auth/user_document_parser.h"
#include "mongo/db/auth/user_name.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/storage/storage_options.h"
#include "mongo/db/tenant_id.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/stdx/unordered_set.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/duration.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/str.h"
#include "mongo/util/time_support.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kAccessControl


namespace mongo {
using namespace fmt::literals;

using std::vector;
using ResolveRoleOption = AuthzManagerExternalStateLocal::ResolveRoleOption;

Status AuthzManagerExternalStateLocal::hasValidStoredAuthorizationVersion(
    OperationContext* opCtx, BSONObj* foundVersionDoc) {
    Status status = findOne(opCtx,
                            NamespaceString::kServerConfigurationNamespace,
                            AuthorizationManager::versionDocumentQuery,
                            foundVersionDoc);
    if (status.isOK()) {
        BSONElement versionElement =
            (*foundVersionDoc)[AuthorizationManager::schemaVersionFieldName];
        if (versionElement.isNumber()) {
            return Status::OK();
        } else if (versionElement.eoo()) {
            return Status(ErrorCodes::NoSuchKey,
                          str::stream() << "No " << AuthorizationManager::schemaVersionFieldName
                                        << " field in version document.");
        } else {
            return Status(ErrorCodes::TypeMismatch,
                          str::stream()
                              << "Could not determine schema version of authorization data.  "
                                 "Bad (non-numeric) type "
                              << typeName(versionElement.type()) << " (" << versionElement.type()
                              << ") for " << AuthorizationManager::schemaVersionFieldName
                              << " field in version document");
        }
    } else {
        return status;
    }
}

Status AuthzManagerExternalStateLocal::getStoredAuthorizationVersion(OperationContext* opCtx,
                                                                     int* outVersion) {
    BSONObj foundVersionDoc;
    auto status = hasValidStoredAuthorizationVersion(opCtx, &foundVersionDoc);
    if (status.isOK()) {
        *outVersion = foundVersionDoc.getIntField(AuthorizationManager::schemaVersionFieldName);
        return status;
    } else if (status == ErrorCodes::NoMatchingDocument) {
        *outVersion = AuthorizationManager::schemaVersion28SCRAM;
        return Status::OK();
    }

    return status;
}

namespace {

NamespaceString getUsersCollection(const boost::optional<TenantId>& tenant) {
    return NamespaceString::makeTenantUsersCollection(tenant);
}

NamespaceString getRolesCollection(const boost::optional<TenantId>& tenant) {
    return NamespaceString::makeTenantRolesCollection(tenant);
}

void serializeResolvedRoles(BSONObjBuilder* user,
                            const AuthzManagerExternalState::ResolvedRoleData& data,
                            boost::optional<const BSONObj&> roleDoc = boost::none) {
    BSONArrayBuilder rolesBuilder(user->subarrayStart("inheritedRoles"));
    for (const auto& roleName : data.roles.value()) {
        roleName.serializeToBSON(&rolesBuilder);
    }
    rolesBuilder.doneFast();

    if (data.privileges) {
        BSONArrayBuilder privsBuilder(user->subarrayStart("inheritedPrivileges"));
        if (roleDoc) {
            auto privs = roleDoc.value()["privileges"];
            if (privs) {
                for (const auto& privilege : privs.Obj()) {
                    privsBuilder.append(privilege);
                }
            }
        }
        for (const auto& privilege : data.privileges.value()) {
            privsBuilder.append(privilege.toBSON());
        }
        privsBuilder.doneFast();
    }

    if (data.restrictions) {
        BSONArrayBuilder arBuilder(user->subarrayStart("inheritedAuthenticationRestrictions"));
        if (roleDoc) {
            auto ar = roleDoc.value()["authenticationRestrictions"];
            if ((ar.type() == Array) && (ar.Obj().nFields() > 0)) {
                arBuilder.append(ar);
            }
        }
        if (auto ar = data.restrictions->toBSON(); ar.nFields() > 0) {
            // TODO: SERVER-50283 Refactor UnnamedRestriction BSON serialization APIs.
            for (const auto& elem : ar) {
                arBuilder.append(elem);
            }
        }
        arBuilder.doneFast();
    }
}

/**
 * Make sure the roleDoc as retreived from storage matches expectations for options.
 */
constexpr auto kRolesFieldName = "roles"_sd;
constexpr auto kPrivilegesFieldName = "privileges"_sd;
constexpr auto kAuthenticationRestrictionFieldName = "authenticationRestrictions"_sd;

std::vector<RoleName> filterAndMapRole(BSONObjBuilder* builder,
                                       BSONObj role,
                                       ResolveRoleOption option,
                                       bool liftAuthenticationRestrictions,
                                       const boost::optional<TenantId>& tenant) {
    std::vector<RoleName> subRoles;
    bool sawRestrictions = false;

    for (const auto& elem : role) {
        if (elem.fieldNameStringData() == kRolesFieldName) {
            uassert(
                ErrorCodes::BadValue, "Invalid roles field, expected array", elem.type() == Array);
            for (const auto& roleName : elem.Obj()) {
                subRoles.push_back(RoleName::parseFromBSON(roleName, tenant));
            }
            if ((option & ResolveRoleOption::kRoles) == 0) {
                continue;
            }
        }

        if ((elem.fieldNameStringData() == kPrivilegesFieldName) &&
            ((option & ResolveRoleOption::kPrivileges) == 0)) {
            continue;
        }

        if (elem.fieldNameStringData() == kAuthenticationRestrictionFieldName) {
            sawRestrictions = true;
            if (option & ResolveRoleOption::kRestrictions) {
                if (liftAuthenticationRestrictions) {
                    // For a rolesInfo invocation, we need to lift ARs up into a container.
                    BSONArrayBuilder arBuilder(
                        builder->subarrayStart(kAuthenticationRestrictionFieldName));
                    arBuilder.append(elem);
                    arBuilder.doneFast();
                } else {
                    // For a usersInfo invocation, we leave it as is.
                    builder->append(elem);
                }
            }
            continue;
        }

        builder->append(elem);
    }

    if (!sawRestrictions && (option & ResolveRoleOption::kRestrictions)) {
        builder->append(kAuthenticationRestrictionFieldName, BSONArray());
    }

    return subRoles;
}

ResolveRoleOption makeResolveRoleOption(PrivilegeFormat showPrivileges,
                                        AuthenticationRestrictionsFormat showRestrictions) {
    auto option = ResolveRoleOption::kRoles;
    if (showPrivileges != PrivilegeFormat::kOmit) {
        option = static_cast<ResolveRoleOption>(option | ResolveRoleOption::kPrivileges);
    }
    if (showRestrictions != AuthenticationRestrictionsFormat::kOmit) {
        option = static_cast<ResolveRoleOption>(option | ResolveRoleOption::kRestrictions);
    }

    return option;
}

MONGO_FAIL_POINT_DEFINE(authLocalGetUser);
void handleAuthLocalGetUserFailPoint(const std::vector<RoleName>& directRoles) {
    auto sfp = authLocalGetUser.scoped();
    if (!sfp.isActive()) {
        return;
    }

    IDLParserContext ctx("authLocalGetUser");
    auto delay = AuthLocalGetUserFailPoint::parse(ctx, sfp.getData()).getResolveRolesDelayMS();

    if (delay <= 0) {
        return;
    }

    LOGV2_DEBUG(4859400,
                3,
                "Sleeping prior to merging direct roles, after user acquisition",
                "duration"_attr = Milliseconds(delay),
                "directRoles"_attr = directRoles);
    sleepmillis(delay);
}
}  // namespace

Status AuthzManagerExternalStateLocal::hasAnyUserDocuments(
    OperationContext* opCtx, const boost::optional<TenantId>& tenantId) {
    BSONObj userBSONObj;
    return findOne(
        opCtx, NamespaceString::makeTenantUsersCollection(tenantId), BSONObj(), &userBSONObj);
}

// If tenantId is none, we're checking whether to enable localhost auth bypass which by definition
// will be a local user.
bool AuthzManagerExternalStateLocal::hasAnyPrivilegeDocuments(OperationContext* opCtx) {
    if (_hasAnyPrivilegeDocuments.load()) {
        return true;
    }

    Status statusFindUsers = hasAnyUserDocuments(opCtx, boost::none);

    // If we were unable to complete the query,
    // it's best to assume that there _are_ privilege documents.
    if (statusFindUsers != ErrorCodes::NoMatchingDocument) {
        _hasAnyPrivilegeDocuments.store(true);
        return true;
    }

    BSONObj userBSONObj;
    Status statusFindRoles =
        findOne(opCtx, NamespaceString::kAdminRolesNamespace, BSONObj(), &userBSONObj);
    if (statusFindRoles != ErrorCodes::NoMatchingDocument) {
        _hasAnyPrivilegeDocuments.store(true);
        return true;
    }

    return false;
}

AuthzManagerExternalStateLocal::RolesLocks::RolesLocks(OperationContext* opCtx,
                                                       const boost::optional<TenantId>& tenant) {
    if (!storageGlobalParams.disableLockFreeReads) {
        _readLockFree = std::make_unique<AutoReadLockFree>(opCtx);
    } else {
        _adminLock = std::make_unique<Lock::DBLock>(opCtx, DatabaseName::kAdmin, LockMode::MODE_IS);
        _rolesLock = std::make_unique<Lock::CollectionLock>(
            opCtx, getRolesCollection(tenant), LockMode::MODE_S);
    }
}

AuthzManagerExternalStateLocal::RolesLocks::~RolesLocks() {
    _readLockFree.reset(nullptr);
    _rolesLock.reset(nullptr);
    _adminLock.reset(nullptr);
}

AuthzManagerExternalStateLocal::RolesLocks AuthzManagerExternalStateLocal::_lockRoles(
    OperationContext* opCtx, const boost::optional<TenantId>& tenant) {
    return AuthzManagerExternalStateLocal::RolesLocks(opCtx, tenant);
}

StatusWith<User> AuthzManagerExternalStateLocal::getUserObject(
    OperationContext* opCtx,
    const UserRequest& userReq,
    const SharedUserAcquisitionStats& userAcquisitionStats) try {
    const UserName& userName = userReq.name;
    std::vector<RoleName> directRoles;
    User user(userReq);

    auto rolesLock = _lockRoles(opCtx, userName.getTenant());

    if (!userReq.roles) {
        // Normal path: Acquire a user from the local store by UserName.
        BSONObj userDoc;
        auto status =
            findOne(opCtx, getUsersCollection(userName.getTenant()), userName.toBSON(), &userDoc);
        if (!status.isOK()) {
            if (status == ErrorCodes::NoMatchingDocument) {
                return {ErrorCodes::UserNotFound,
                        str::stream() << "Could not find user \"" << userName.getUser()
                                      << "\" for db \"" << userName.getDB() << "\""};
            }
            return status;
        }

        V2UserDocumentParser userDocParser;
        userDocParser.setTenantId(userReq.name.getTenant());
        uassertStatusOK(userDocParser.initializeUserFromUserDocument(userDoc, &user));
        for (auto iter = user.getRoles(); iter.more();) {
            directRoles.push_back(iter.next());
        }
    } else {
        // Proxy path.  Some other external mechanism (e.g. X509 or LDAP) has acquired
        // a base user definition with a set of immediate roles.
        // We're being asked to use the local roles collection to derive privileges,
        // subordinate roles, and authentication restrictions.
        directRoles = std::vector<RoleName>(userReq.roles->cbegin(), userReq.roles->cend());
        User::CredentialData credentials;
        credentials.isExternal = true;
        user.setCredentials(credentials);
        user.setRoles(makeRoleNameIteratorForContainer(directRoles));
    }

    if (auto tenant = userName.getTenant()) {
        // Apply TenantID for user to all roles (which are assumed to be part of the same tenant).
        for (auto& role : directRoles) {
            role = RoleName(role.getRole(), role.getDB(), tenant);
        }
    }

    handleAuthLocalGetUserFailPoint(directRoles);

    auto data = uassertStatusOK(resolveRoles(opCtx, directRoles, ResolveRoleOption::kAll));
    data.roles->insert(directRoles.cbegin(), directRoles.cend());
    user.setIndirectRoles(makeRoleNameIteratorForContainer(data.roles.value()));
    user.addPrivileges(data.privileges.value());
    user.setIndirectRestrictions(data.restrictions.value());

    LOGV2_DEBUG(5517200,
                3,
                "Acquired new user object",
                "userName"_attr = userName,
                "directRoles"_attr = directRoles);

    return std::move(user);
} catch (const AssertionException& ex) {
    return ex.toStatus();
}

Status AuthzManagerExternalStateLocal::getUserDescription(
    OperationContext* opCtx,
    const UserRequest& userReq,
    BSONObj* result,
    const SharedUserAcquisitionStats& userAcquisitionStats) try {
    const UserName& userName = userReq.name;
    std::vector<RoleName> directRoles;
    BSONObjBuilder resultBuilder;

    auto rolesLock = _lockRoles(opCtx, userName.getTenant());

    if (!userReq.roles) {
        BSONObj userDoc;
        auto status =
            findOne(opCtx, getUsersCollection(userName.getTenant()), userName.toBSON(), &userDoc);
        if (!status.isOK()) {
            if (status == ErrorCodes::NoMatchingDocument) {
                return {ErrorCodes::UserNotFound,
                        str::stream() << "Could not find user \"" << userName.getUser()
                                      << "\" for db \"" << userName.getDB() << "\""};
            }
            return status;
        }

        directRoles = filterAndMapRole(
            &resultBuilder, userDoc, ResolveRoleOption::kAll, false, userName.getTenant());
    } else {
        uassert(ErrorCodes::BadValue,
                "Illegal combination of pre-defined roles with tenant identifier",
                userName.getTenant() == boost::none);

        // We are able to artifically construct the external user from the request
        resultBuilder.append("_id", str::stream() << userName.getDB() << '.' << userName.getUser());
        resultBuilder.append("user", userName.getUser());
        resultBuilder.append("db", userName.getDB());
        resultBuilder.append("credentials", BSON("external" << true));

        directRoles = std::vector<RoleName>(userReq.roles->cbegin(), userReq.roles->cend());
        BSONArrayBuilder rolesBuilder(resultBuilder.subarrayStart("roles"));
        for (const RoleName& role : directRoles) {
            rolesBuilder.append(role.toBSON());
        }
        rolesBuilder.doneFast();
    }

    if (auto tenant = userName.getTenant()) {
        // Apply TenantID for user to all roles (which are assumed to be part of the same tenant).
        for (auto& role : directRoles) {
            role = RoleName(role.getRole(), role.getDB(), tenant);
        }
    }

    handleAuthLocalGetUserFailPoint(directRoles);

    auto data = uassertStatusOK(resolveRoles(opCtx, directRoles, ResolveRoleOption::kAll));
    data.roles->insert(directRoles.cbegin(), directRoles.cend());
    serializeResolvedRoles(&resultBuilder, data);
    *result = resultBuilder.obj();

    return Status::OK();
} catch (const AssertionException& ex) {
    return ex.toStatus();
}

Status AuthzManagerExternalStateLocal::rolesExist(OperationContext* opCtx,
                                                  const std::vector<RoleName>& roleNames) {
    // Perform DB queries for user-defined roles (skipping builtin roles).
    stdx::unordered_set<RoleName> unknownRoles;
    for (const auto& roleName : roleNames) {
        if (!auth::isBuiltinRole(roleName) &&
            !hasOne(opCtx, getRolesCollection(roleName.getTenant()), roleName.toBSON())) {
            unknownRoles.insert(roleName);
        }
    }

    // If anything remains, raise it as an unknown role error.
    if (!unknownRoles.empty()) {
        return makeRoleNotFoundStatus(unknownRoles);
    }

    return Status::OK();
}

using ResolvedRoleData = AuthzManagerExternalState::ResolvedRoleData;
StatusWith<ResolvedRoleData> AuthzManagerExternalStateLocal::resolveRoles(
    OperationContext* opCtx, const std::vector<RoleName>& roleNames, ResolveRoleOption option) try {
    using RoleNameSet = typename decltype(ResolvedRoleData::roles)::value_type;
    const bool processRoles = option & ResolveRoleOption::kRoles;
    const bool processPrivs = option & ResolveRoleOption::kPrivileges;
    const bool processRests = option & ResolveRoleOption::kRestrictions;
    const bool walkIndirect = (option & ResolveRoleOption::kDirectOnly) == 0;
    IDLParserContext idlctx("resolveRoles");

    RoleNameSet inheritedRoles;
    PrivilegeVector inheritedPrivileges;
    RestrictionDocuments::sequence_type inheritedRestrictions;

    RoleNameSet frontier(roleNames.cbegin(), roleNames.cend());
    RoleNameSet visited;
    while (!frontier.empty()) {
        RoleNameSet nextFrontier;
        for (const auto& role : frontier) {
            visited.insert(role);

            if (auth::isBuiltinRole(role)) {
                if (processPrivs) {
                    invariant(auth::addPrivilegesForBuiltinRole(role, &inheritedPrivileges));
                }
                continue;
            }

            BSONObj roleDoc;
            auto status =
                findOne(opCtx, getRolesCollection(role.getTenant()), role.toBSON(), &roleDoc);
            if (!status.isOK()) {
                if (status.code() == ErrorCodes::NoMatchingDocument) {
                    LOGV2(5029200, "Role does not exist", "role"_attr = role);
                    continue;
                }
                return status;
            }

            BSONElement elem;
            if ((processRoles || walkIndirect) && (elem = roleDoc["roles"])) {
                if (elem.type() != Array) {
                    return {ErrorCodes::BadValue,
                            str::stream()
                                << "Invalid 'roles' field in role document '" << role
                                << "', expected an array but found " << typeName(elem.type())};
                }
                for (const auto& subroleElem : elem.Obj()) {
                    auto subrole = RoleName::parseFromBSON(subroleElem, role.getTenant());
                    if (visited.count(subrole) || nextFrontier.count(subrole)) {
                        continue;
                    }
                    if (walkIndirect) {
                        nextFrontier.insert(subrole);
                    }
                    if (processRoles) {
                        inheritedRoles.insert(std::move(subrole));
                    }
                }
            }

            if (processPrivs && (elem = roleDoc["privileges"])) {
                if (elem.type() != Array) {
                    return {ErrorCodes::UnsupportedFormat,
                            str::stream()
                                << "Invalid 'privileges' field in role document '" << role << "'"};
                }
                for (const auto& privElem : elem.Obj()) {
                    if (privElem.type() != Object) {
                        return {ErrorCodes::UnsupportedFormat,
                                "Expected privilege document as object, got {}"_format(
                                    typeName(privElem.type()))};
                    }
                    auto pp = auth::ParsedPrivilege::parse(idlctx, privElem.Obj());
                    Privilege::addPrivilegeToPrivilegeVector(
                        &inheritedPrivileges,
                        Privilege::resolvePrivilegeWithTenant(role.getTenant(), pp));
                }
            }

            if (processRests && (elem = roleDoc["authenticationRestrictions"])) {
                if (elem.type() != Array) {
                    return {ErrorCodes::UnsupportedFormat,
                            str::stream()
                                << "Invalid 'authenticationRestrictions' field in role document '"
                                << role << "'"};
                }
                inheritedRestrictions.push_back(
                    uassertStatusOK(parseAuthenticationRestriction(BSONArray(elem.Obj()))));
            }
        }
        frontier = std::move(nextFrontier);
    }

    ResolvedRoleData ret;
    if (processRoles) {
        ret.roles = std::move(inheritedRoles);
    }
    if (processPrivs) {
        ret.privileges = std::move(inheritedPrivileges);
    }
    if (processRests) {
        ret.restrictions = RestrictionDocuments(std::move(inheritedRestrictions));
    }

    return ret;
} catch (const AssertionException& ex) {
    return ex.toStatus();
}

Status AuthzManagerExternalStateLocal::getRolesAsUserFragment(
    OperationContext* opCtx,
    const std::vector<RoleName>& roleNames,
    AuthenticationRestrictionsFormat showRestrictions,
    BSONObj* result) {
    auto option = makeResolveRoleOption(PrivilegeFormat::kShowAsUserFragment, showRestrictions);

    BSONObjBuilder fragment;

    BSONArrayBuilder rolesBuilder(fragment.subarrayStart("roles"));
    for (const auto& roleName : roleNames) {
        roleName.serializeToBSON(&rolesBuilder);
    }
    rolesBuilder.doneFast();

    auto swData = resolveRoles(opCtx, roleNames, option);
    if (!swData.isOK()) {
        return swData.getStatus();
    }
    auto data = std::move(swData.getValue());
    data.roles->insert(roleNames.cbegin(), roleNames.cend());
    serializeResolvedRoles(&fragment, data);

    *result = fragment.obj();
    return Status::OK();
}

Status AuthzManagerExternalStateLocal::getRolesDescription(
    OperationContext* opCtx,
    const std::vector<RoleName>& roleNames,
    PrivilegeFormat showPrivileges,
    AuthenticationRestrictionsFormat showRestrictions,
    std::vector<BSONObj>* result) {

    if (showPrivileges == PrivilegeFormat::kShowAsUserFragment) {
        // Shouldn't be called this way, but cope if we are.
        BSONObj fragment;
        auto status = getRolesAsUserFragment(opCtx, roleNames, showRestrictions, &fragment);
        if (status.isOK()) {
            result->push_back(fragment);
        }
        return status;
    }

    auto option = makeResolveRoleOption(showPrivileges, showRestrictions);

    for (const auto& role : roleNames) {
        try {
            BSONObj roleDoc;

            if (auth::isBuiltinRole(role)) {
                // Synthesize builtin role from definition.
                PrivilegeVector privs;
                uassert(ErrorCodes::OperationFailed,
                        "Failed generating builtin role privileges",
                        auth::addPrivilegesForBuiltinRole(role, &privs));

                BSONObjBuilder builtinBuilder;
                builtinBuilder.append("db", role.getDB());
                builtinBuilder.append("role", role.getRole());
                builtinBuilder.append("roles", BSONArray());
                if (showPrivileges == PrivilegeFormat::kShowSeparate) {
                    BSONArrayBuilder builtinPrivs(builtinBuilder.subarrayStart("privileges"));
                    for (const auto& priv : privs) {
                        builtinPrivs.append(priv.toBSON());
                    }
                    builtinPrivs.doneFast();
                }

                roleDoc = builtinBuilder.obj();
            } else {
                auto status =
                    findOne(opCtx, getRolesCollection(role.getTenant()), role.toBSON(), &roleDoc);
                if (status.code() == ErrorCodes::NoMatchingDocument) {
                    continue;
                }
                uassertStatusOK(status);  // throws
            }

            BSONObjBuilder roleBuilder;
            auto subRoles = filterAndMapRole(&roleBuilder, roleDoc, option, true, role.getTenant());
            auto data = uassertStatusOK(resolveRoles(opCtx, subRoles, option));
            data.roles->insert(subRoles.cbegin(), subRoles.cend());
            serializeResolvedRoles(&roleBuilder, data, roleDoc);
            roleBuilder.append("isBuiltin", auth::isBuiltinRole(role));

            result->push_back(roleBuilder.obj());
        } catch (const AssertionException& ex) {
            return {ex.code(),
                    str::stream() << "Failed fetching role '" << role << "': " << ex.reason()};
        }
    }

    return Status::OK();
}

Status AuthzManagerExternalStateLocal::getRoleDescriptionsForDB(
    OperationContext* opCtx,
    const DatabaseName& dbname,
    PrivilegeFormat showPrivileges,
    AuthenticationRestrictionsFormat showRestrictions,
    bool showBuiltinRoles,
    std::vector<BSONObj>* result) {
    auto option = makeResolveRoleOption(showPrivileges, showRestrictions);

    if (showPrivileges == PrivilegeFormat::kShowAsUserFragment) {
        return {ErrorCodes::IllegalOperation,
                "Cannot get user fragment for all roles in a database"};
    }

    if (showBuiltinRoles) {
        for (const auto& roleName : auth::getBuiltinRoleNamesForDB(dbname)) {
            BSONObjBuilder roleBuilder;

            roleBuilder.append(AuthorizationManager::ROLE_NAME_FIELD_NAME, roleName.getRole());
            roleBuilder.append(AuthorizationManager::ROLE_DB_FIELD_NAME, roleName.getDB());
            roleBuilder.append("isBuiltin", true);

            roleBuilder.append("roles", BSONArray());
            roleBuilder.append("inheritedRoles", BSONArray());

            if (showPrivileges == PrivilegeFormat::kShowSeparate) {
                BSONArrayBuilder privsBuilder(roleBuilder.subarrayStart("privileges"));
                PrivilegeVector privs;
                invariant(auth::addPrivilegesForBuiltinRole(roleName, &privs));
                for (const auto& privilege : privs) {
                    privsBuilder.append(privilege.toBSON());
                }
                privsBuilder.doneFast();

                // Builtin roles have identival privs/inheritedPrivs
                BSONArrayBuilder ipBuilder(roleBuilder.subarrayStart("inheritedPrivileges"));
                for (const auto& privilege : privs) {
                    ipBuilder.append(privilege.toBSON());
                }
                ipBuilder.doneFast();
            }

            if (showRestrictions == AuthenticationRestrictionsFormat::kShow) {
                roleBuilder.append("authenticationRestrictions", BSONArray());
                roleBuilder.append("inheritedAuthenticationRestrictions", BSONArray());
            }

            result->push_back(roleBuilder.obj());
        }
    }

    return query(opCtx,
                 getRolesCollection(dbname.tenantId()),
                 BSON(AuthorizationManager::ROLE_DB_FIELD_NAME
                      << dbname.serializeWithoutTenantPrefix_UNSAFE()),
                 BSONObj(),
                 [&](const BSONObj& roleDoc) {
                     try {
                         BSONObjBuilder roleBuilder;

                         auto subRoles = filterAndMapRole(
                             &roleBuilder, roleDoc, option, true, dbname.tenantId());
                         roleBuilder.append("isBuiltin", false);
                         auto data = uassertStatusOK(resolveRoles(opCtx, subRoles, option));
                         data.roles->insert(subRoles.cbegin(), subRoles.cend());
                         serializeResolvedRoles(&roleBuilder, data, roleDoc);
                         result->push_back(roleBuilder.obj());
                         return Status::OK();
                     } catch (const AssertionException& ex) {
                         return ex.toStatus();
                     }
                 });
}

/**
 * Below this point is the implementation of our OpObserver handler.
 *
 * Ops which mutate user documents will invalidate those specific users
 * from the UserCache.
 *
 * Any other privilege related op (mutation to roles or version collection,
 * or command issued on the admin namespace) will invalidate the entire
 * user cache.
 */

namespace {
class AuthzCollection {
public:
    enum class AuthzCollectionType {
        kNone,
        kUsers,
        kRoles,
        kVersion,
        kAdmin,
    };

    AuthzCollection() = default;
    explicit AuthzCollection(const NamespaceString& nss) : _tenant(nss.tenantId()) {
        // Capture events regardless of what Tenant they occured in,
        // invalidators will purge cache on a per-tenant basis as needed.
        auto db = nss.dbName();
        auto coll = nss.coll();
        if (!db.isAdminDB()) {
            return;
        }

        // System-only collections.
        if (coll == NamespaceString::kServerConfigurationNamespace.coll()) {
            _type = AuthzCollectionType::kVersion;
            return;
        }

        if (coll == NamespaceString::kAdminCommandNamespace.coll()) {
            _type = AuthzCollectionType::kAdmin;
            return;
        }

        if (coll == NamespaceString::kSystemUsers) {
            // admin.system.users or {tenantID}_admin.system.users
            _type = AuthzCollectionType::kUsers;
            return;
        }

        if (coll == NamespaceString::kSystemRoles) {
            // admin.system.roles or {tenantID}_admin.system.roles
            _type = AuthzCollectionType::kRoles;
            return;
        }
    }

    operator bool() const {
        return _type != AuthzCollectionType::kNone;
    }

    bool isPrivilegeCollection() const {
        return (_type == AuthzCollectionType::kUsers) || (_type == AuthzCollectionType::kRoles);
    }

    AuthzCollectionType getType() const {
        return _type;
    }

    const boost::optional<TenantId>& getTenant() const {
        return _tenant;
    }

private:
    AuthzCollectionType _type = AuthzCollectionType::kNone;
    boost::optional<TenantId> _tenant;
};

constexpr auto kOpInsert = "i"_sd;
constexpr auto kOpUpdate = "u"_sd;
constexpr auto kOpDelete = "d"_sd;

void _invalidateUserCache(OperationContext* opCtx,
                          AuthorizationManagerImpl* authzManager,
                          StringData op,
                          AuthzCollection coll,
                          const BSONObj& o,
                          const BSONObj* o2) {
    if ((coll.getType() == AuthzCollection::AuthzCollectionType::kUsers) &&
        ((op == kOpInsert) || (op == kOpUpdate) || (op == kOpDelete))) {
        const BSONObj* src = (op == kOpUpdate) ? o2 : &o;
        auto id = (*src)["_id"].str();
        auto splitPoint = id.find('.');
        if (splitPoint == std::string::npos) {
            LOGV2_WARNING(23749,
                          "Invalidating user cache based on user being updated failed, will "
                          "invalidate the entire cache instead",
                          "error"_attr =
                              Status(ErrorCodes::FailedToParse,
                                     str::stream() << "_id entries for user documents must be of "
                                                      "the form <dbname>.<username>.  Found: "
                                                   << id));
            authzManager->invalidateUserCache(opCtx);
            return;
        }
        UserName userName(id.substr(splitPoint + 1), id.substr(0, splitPoint), coll.getTenant());
        authzManager->invalidateUserByName(opCtx, userName);
    } else if (const auto& tenant = coll.getTenant()) {
        authzManager->invalidateUsersByTenant(opCtx, tenant.value());
    } else {
        authzManager->invalidateUserCache(opCtx);
    }
}
}  // namespace

void AuthzManagerExternalStateLocal::logOp(OperationContext* opCtx,
                                           AuthorizationManagerImpl* authzManager,
                                           StringData op,
                                           const NamespaceString& nss,
                                           const BSONObj& o,
                                           const BSONObj* o2) {
    AuthzCollection coll(nss);
    if (!coll) {
        return;
    }

    _invalidateUserCache(opCtx, authzManager, op, coll, o, o2);

    if (coll.isPrivilegeCollection() && (op == kOpInsert)) {
        _hasAnyPrivilegeDocuments.store(true);
    }
}

}  // namespace mongo
