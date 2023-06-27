/**
 *    Copyright (C) 2020-present MongoDB, Inc.
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

#include <boost/optional/optional.hpp>
#include <memory>
#include <string>

#include "mongo/base/status.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/feature_compatibility_version_parser.h"
#include "mongo/db/server_options.h"
#include "mongo/db/server_parameter.h"
#include "mongo/db/tenant_id.h"
#include "mongo/util/version/releases.h"

namespace mongo {

/**
 * FeatureFlag contains information about whether a feature flag is enabled and what version it was
 * released.
 *
 * FeatureFlag represents the state of a feature flag and whether it is associated with a particular
 * version. It is not implicitly convertible to bool to force all call sites to make a decision
 * about what check to use.
 *
 * It is only set at startup.
 */
class FeatureFlag {
    friend class FeatureFlagServerParameter;

public:
    FeatureFlag(bool enabled, StringData versionString, bool shouldBeFCVGated);

    /**
     * Returns true if the flag is set to true and enabled for this FCV version.
     */
    bool isEnabled(const ServerGlobalParams::FeatureCompatibility& fcv) const;

    /**
     * Returns true if the flag is set to true and enabled for this FCV version. If the FCV version
     * is unset, instead checks against the default last LTS FCV version.
     */
    bool isEnabledUseDefaultFCVWhenUninitialized(
        const ServerGlobalParams::FeatureCompatibility& fcv) const;

    /**
     * Returns true if this flag is enabled regardless of the current FCV version. When using this
     * function, you are allowing the feature flag to pass checking during transitional FCV states
     * and downgraded FCV, which means the code gated by this feature flag is allowed to run even if
     * the FCV requirement of this feature flag is not met.
     *
     * isEnabled() is prefered over this function since it will prevent upgrade/downgrade issues.
     *
     * Note: A comment starting with (Ignore FCV check) is required for the use of this function. If
     * the feature flag check is before FCV initialization, use isEnabledAndIgnoreFCVUnsafeAtStartup
     * instead.
     */
    bool isEnabledAndIgnoreFCVUnsafe() const;

    /**
     * Returns true if this flag is enabled regardless of the current FCV version. When using this
     * function, you are allowing the feature flag to pass checking during transitional FCV states
     * and downgraded FCV, which means the code gated by this feature flag is allowed to run even if
     * the FCV requirement of this feature flag is not met.
     *
     * This is same as isEnabledAndIgnoreFCVUnsafe() but doesn't require a comment. This should
     * only be used before FCV initialization.
     */
    bool isEnabledAndIgnoreFCVUnsafeAtStartup() const;

    /**
     * Returns true if the flag is set to true and enabled on the target FCV version.
     *
     * This function is used in the 'setFeatureCompatibilityVersion' command where the in-memory FCV
     * is in flux.
     */
    bool isEnabledOnVersion(multiversion::FeatureCompatibilityVersion targetFCV) const;

    /**
     * Returns true if the feature flag is disabled on targetFCV but enabled on originalFCV.
     */
    bool isDisabledOnTargetFCVButEnabledOnOriginalFCV(
        multiversion::FeatureCompatibilityVersion targetFCV,
        multiversion::FeatureCompatibilityVersion originalFCV) const;

    /**
     * Returns true if the feature flag is enabled on targetFCV but disabled on originalFCV.
     */
    bool isEnabledOnTargetFCVButDisabledOnOriginalFCV(
        multiversion::FeatureCompatibilityVersion targetFCV,
        multiversion::FeatureCompatibilityVersion originalFCV) const;

    /**
     * Return the version associated with this feature flag.
     *
     * Throws if feature is not enabled.
     */
    multiversion::FeatureCompatibilityVersion getVersion() const;

private:
    void set(bool enabled);

private:
    bool _enabled;
    multiversion::FeatureCompatibilityVersion _version;
    bool _shouldBeFCVGated;
};

/**
 * Specialization of ServerParameter for FeatureFlags used by IDL generator.
 */
class FeatureFlagServerParameter : public ServerParameter {
public:
    FeatureFlagServerParameter(StringData name, FeatureFlag& storage);

    /**
     * Encode the setting into BSON object.
     *
     * Typically invoked by {getParameter:...} to produce a dictionary
     * of ServerParameter settings.
     */
    void append(OperationContext* opCtx,
                BSONObjBuilder* b,
                StringData name,
                const boost::optional<TenantId>&) final;

    /**
     * Encode the feature flag value into a BSON object, discarding the version.
     */
    void appendSupportingRoundtrip(OperationContext* opCtx,
                                   BSONObjBuilder* b,
                                   StringData name,
                                   const boost::optional<TenantId>&) override;

    /**
     * Update the underlying value using a BSONElement
     *
     * Allows setting non-basic values (e.g. vector<string>)
     * via the {setParameter: ...} call.
     */
    Status set(const BSONElement& newValueElement, const boost::optional<TenantId>&) final;

    /**
     * Update the underlying value from a string.
     *
     * Typically invoked from commandline --setParameter usage.
     */
    Status setFromString(StringData str, const boost::optional<TenantId>&) final;

private:
    FeatureFlag& _storage;
};

inline FeatureFlagServerParameter* makeFeatureFlagServerParameter(StringData name,
                                                                  FeatureFlag& storage) {
    auto p = std::make_unique<FeatureFlagServerParameter>(name, storage);
    registerServerParameter(&*p);
    return p.release();
}

}  // namespace mongo
