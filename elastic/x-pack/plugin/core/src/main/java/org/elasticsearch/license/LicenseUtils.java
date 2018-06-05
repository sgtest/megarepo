/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.license;

import org.elasticsearch.ElasticsearchSecurityException;
import org.elasticsearch.Version;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.rest.RestStatus;

import java.util.stream.StreamSupport;

public class LicenseUtils {

    public static final String EXPIRED_FEATURE_METADATA = "es.license.expired.feature";

    /**
     * Exception to be thrown when a feature action requires a valid license, but license
     * has expired
     *
     * <code>feature</code> accessible through {@link #EXPIRED_FEATURE_METADATA} in the
     * exception's rest header
     */
    public static ElasticsearchSecurityException newComplianceException(String feature) {
        ElasticsearchSecurityException e = new ElasticsearchSecurityException("current license is non-compliant for [{}]",
                RestStatus.FORBIDDEN, feature);
        e.addMetadata(EXPIRED_FEATURE_METADATA, feature);
        return e;
    }

    /**
     * Checks if a given {@link ElasticsearchSecurityException} refers to a feature that
     * requires a valid license, but the license has expired.
     */
    public static boolean isLicenseExpiredException(ElasticsearchSecurityException exception) {
        return (exception != null) && (exception.getMetadata(EXPIRED_FEATURE_METADATA) != null);
    }

    public static boolean licenseNeedsExtended(License license) {
        return "basic".equals(license.type()) && license.expiryDate() != LicenseService.BASIC_SELF_GENERATED_LICENSE_EXPIRATION_MILLIS;
    }

    /**
     * Checks if the signature of a self generated license with older version needs to be
     * recreated with the new key
     */
    public static boolean signatureNeedsUpdate(License license, DiscoveryNodes currentNodes) {
        assert License.VERSION_CRYPTO_ALGORITHMS == License.VERSION_CURRENT : "update this method when adding a new version";

        return ("basic".equals(license.type()) || "trial".equals(license.type())) &&
                // only upgrade signature when all nodes are ready to deserialize the new signature
                (license.version() < License.VERSION_CRYPTO_ALGORITHMS &&
                    compatibleLicenseVersion(currentNodes) == License.VERSION_CRYPTO_ALGORITHMS
                );
    }

    public static int compatibleLicenseVersion(DiscoveryNodes currentNodes) {
        assert License.VERSION_CRYPTO_ALGORITHMS == License.VERSION_CURRENT : "update this method when adding a new version";

        if (StreamSupport.stream(currentNodes.spliterator(), false)
            .allMatch(node -> node.getVersion().onOrAfter(Version.V_6_4_0))) {
            // License.VERSION_CRYPTO_ALGORITHMS was introduced in 6.4.0
            return License.VERSION_CRYPTO_ALGORITHMS;
        } else {
            return License.VERSION_START_DATE;
        }
    }
}
