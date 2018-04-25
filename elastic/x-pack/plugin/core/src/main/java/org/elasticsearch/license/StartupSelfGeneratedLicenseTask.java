/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.license;

import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.apache.logging.log4j.util.Supplier;
import org.elasticsearch.Version;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateUpdateTask;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.logging.Loggers;
import org.elasticsearch.common.settings.Settings;

import java.time.Clock;
import java.util.UUID;

public class StartupSelfGeneratedLicenseTask extends ClusterStateUpdateTask {

    /**
     * Max number of nodes licensed by generated trial license
     */
    private int selfGeneratedLicenseMaxNodes = 1000;

    private final Settings settings;
    private final Clock clock;
    private final ClusterService clusterService;
    private final Logger logger;

    public StartupSelfGeneratedLicenseTask(Settings settings, Clock clock, ClusterService clusterService) {
        this.settings = settings;
        this.clock = clock;
        this.clusterService = clusterService;
        this.logger = Loggers.getLogger(getClass(), settings);
    }

    @Override
    public void clusterStateProcessed(String source, ClusterState oldState, ClusterState newState) {
        LicensesMetaData licensesMetaData = newState.metaData().custom(LicensesMetaData.TYPE);
        if (logger.isDebugEnabled()) {
            logger.debug("registered self generated license: {}", licensesMetaData);
        }
    }

    @Override
    public ClusterState execute(ClusterState currentState) throws Exception {
        final MetaData metaData = currentState.metaData();
        final LicensesMetaData currentLicensesMetaData = metaData.custom(LicensesMetaData.TYPE);
        // do not generate a license if any license is present
        if (currentLicensesMetaData == null) {
            String type = LicenseService.SELF_GENERATED_LICENSE_TYPE.get(settings);
            if (SelfGeneratedLicense.validSelfGeneratedType(type) == false) {
                throw new IllegalArgumentException("Illegal self generated license type [" + type +
                        "]. Must be trial or basic.");
            }

            return updateWithLicense(currentState, type);
        } else if (LicenseUtils.licenseNeedsExtended(currentLicensesMetaData.getLicense())) {
            return extendBasic(currentState, currentLicensesMetaData);
        } else {
            return currentState;
        }
    }

    @Override
    public void onFailure(String source, @Nullable Exception e) {
        logger.error((Supplier<?>) () -> new ParameterizedMessage("unexpected failure during [{}]", source), e);
    }

    private ClusterState extendBasic(ClusterState currentState, LicensesMetaData currentLicenseMetadata) {
        License license = currentLicenseMetadata.getLicense();
        MetaData.Builder mdBuilder = MetaData.builder(currentState.metaData());
        LicensesMetaData newLicenseMetadata = createBasicLicenseFromExistingLicense(currentLicenseMetadata);
        mdBuilder.putCustom(LicensesMetaData.TYPE, newLicenseMetadata);
        logger.info("Existing basic license has an expiration. Basic licenses no longer expire." +
                "Regenerating license.\n\nOld license:\n {}\n\n New license:\n{}", license, newLicenseMetadata.getLicense());
        return ClusterState.builder(currentState).metaData(mdBuilder).build();
    }

    private LicensesMetaData createBasicLicenseFromExistingLicense(LicensesMetaData currentLicenseMetadata) {
        License currentLicense = currentLicenseMetadata.getLicense();
        License.Builder specBuilder = License.builder()
                .uid(currentLicense.uid())
                .issuedTo(currentLicense.issuedTo())
                .maxNodes(selfGeneratedLicenseMaxNodes)
                .issueDate(currentLicense.issueDate())
                .type("basic")
                .expiryDate(LicenseService.BASIC_SELF_GENERATED_LICENSE_EXPIRATION_MILLIS);
        License selfGeneratedLicense = SelfGeneratedLicense.create(specBuilder);
        Version trialVersion = currentLicenseMetadata.getMostRecentTrialVersion();
        return new LicensesMetaData(selfGeneratedLicense, trialVersion);
    }

    private ClusterState updateWithLicense(ClusterState currentState, String type) {
        long issueDate = clock.millis();
        MetaData.Builder mdBuilder = MetaData.builder(currentState.metaData());
        long expiryDate;
        if ("basic".equals(type)) {
            expiryDate = LicenseService.BASIC_SELF_GENERATED_LICENSE_EXPIRATION_MILLIS;
        } else {
            expiryDate = issueDate + LicenseService.NON_BASIC_SELF_GENERATED_LICENSE_DURATION.getMillis();
        }
        License.Builder specBuilder = License.builder()
                .uid(UUID.randomUUID().toString())
                .issuedTo(clusterService.getClusterName().value())
                .maxNodes(selfGeneratedLicenseMaxNodes)
                .issueDate(issueDate)
                .type(type)
                .expiryDate(expiryDate);
        License selfGeneratedLicense = SelfGeneratedLicense.create(specBuilder);
        LicensesMetaData licensesMetaData;
        if ("trial".equals(type)) {
            licensesMetaData = new LicensesMetaData(selfGeneratedLicense, Version.CURRENT);
        } else {
            licensesMetaData = new LicensesMetaData(selfGeneratedLicense, null);
        }
        mdBuilder.putCustom(LicensesMetaData.TYPE, licensesMetaData);
        return ClusterState.builder(currentState).metaData(mdBuilder).build();
    }
}
