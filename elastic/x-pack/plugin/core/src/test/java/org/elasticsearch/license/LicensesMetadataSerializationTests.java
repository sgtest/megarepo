/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.license;

import org.elasticsearch.Version;
import org.elasticsearch.cluster.ClusterModule;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.metadata.RepositoriesMetadata;
import org.elasticsearch.cluster.metadata.RepositoryMetadata;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.ToXContent.Params;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.test.ESTestCase;

import java.util.Collections;
import java.util.UUID;
import java.util.stream.Collectors;
import java.util.stream.Stream;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;

public class LicensesMetadataSerializationTests extends ESTestCase {

    public void testXContentSerializationOneSignedLicense() throws Exception {
        License license = TestUtils.generateSignedLicense(TimeValue.timeValueHours(2));
        LicensesMetadata licensesMetadata = new LicensesMetadata(license, null);
        XContentBuilder builder = XContentFactory.jsonBuilder();
        builder.startObject();
        builder.startObject("licenses");
        licensesMetadata.toXContent(builder, ToXContent.EMPTY_PARAMS);
        builder.endObject();
        builder.endObject();
        LicensesMetadata licensesMetadataFromXContent = getLicensesMetadataFromXContent(createParser(builder));
        assertThat(licensesMetadataFromXContent.getLicense(), equalTo(license));
        assertNull(licensesMetadataFromXContent.getMostRecentTrialVersion());
    }

    public void testXContentSerializationOneSignedLicenseWithUsedTrial() throws Exception {
        License license = TestUtils.generateSignedLicense(TimeValue.timeValueHours(2));
        LicensesMetadata licensesMetadata = new LicensesMetadata(license, Version.CURRENT);
        XContentBuilder builder = XContentFactory.jsonBuilder();
        builder.startObject();
        builder.startObject("licenses");
        licensesMetadata.toXContent(builder, ToXContent.EMPTY_PARAMS);
        builder.endObject();
        builder.endObject();
        LicensesMetadata licensesMetadataFromXContent = getLicensesMetadataFromXContent(createParser(builder));
        assertThat(licensesMetadataFromXContent.getLicense(), equalTo(license));
        assertEquals(licensesMetadataFromXContent.getMostRecentTrialVersion(), Version.CURRENT);
    }

    public void testLicenseMetadataParsingDoesNotSwallowOtherMetadata() throws Exception {
        new Licensing(Settings.EMPTY); // makes sure LicensePlugin is registered in Custom Metadata
        License license = TestUtils.generateSignedLicense(TimeValue.timeValueHours(2));
        LicensesMetadata licensesMetadata = new LicensesMetadata(license, Version.CURRENT);
        RepositoryMetadata repositoryMetadata = new RepositoryMetadata("repo", "fs", Settings.EMPTY);
        RepositoriesMetadata repositoriesMetadata = new RepositoriesMetadata(Collections.singletonList(repositoryMetadata));
        final Metadata.Builder metadataBuilder = Metadata.builder();
        if (randomBoolean()) { // random order of insertion
            metadataBuilder.putCustom(licensesMetadata.getWriteableName(), licensesMetadata);
            metadataBuilder.putCustom(repositoriesMetadata.getWriteableName(), repositoriesMetadata);
        } else {
            metadataBuilder.putCustom(repositoriesMetadata.getWriteableName(), repositoriesMetadata);
            metadataBuilder.putCustom(licensesMetadata.getWriteableName(), licensesMetadata);
        }
        // serialize metadata
        XContentBuilder builder = XContentFactory.jsonBuilder();
        Params params = new ToXContent.MapParams(Collections.singletonMap(Metadata.CONTEXT_MODE_PARAM, Metadata.CONTEXT_MODE_GATEWAY));
        builder.startObject();
        builder = metadataBuilder.build().toXContent(builder, params);
        builder.endObject();
        // deserialize metadata again
        Metadata metadata = Metadata.Builder.fromXContent(createParser(builder));
        // check that custom metadata still present
        assertThat(metadata.custom(licensesMetadata.getWriteableName()), notNullValue());
        assertThat(metadata.custom(repositoriesMetadata.getWriteableName()), notNullValue());
    }

    public void testXContentSerializationOneTrial() throws Exception {
        long issueDate = System.currentTimeMillis();
        License.Builder specBuilder = License.builder()
                .uid(UUID.randomUUID().toString())
                .issuedTo("customer")
                .maxNodes(5)
                .issueDate(issueDate)
                .type(randomBoolean() ? "trial" : "basic")
                .expiryDate(issueDate + TimeValue.timeValueHours(2).getMillis());
        final License trialLicense = SelfGeneratedLicense.create(specBuilder, License.VERSION_CURRENT);
        LicensesMetadata licensesMetadata = new LicensesMetadata(trialLicense, Version.CURRENT);
        XContentBuilder builder = XContentFactory.jsonBuilder();
        builder.startObject();
        builder.startObject("licenses");
        licensesMetadata.toXContent(builder, ToXContent.EMPTY_PARAMS);
        builder.endObject();
        builder.endObject();
        LicensesMetadata licensesMetadataFromXContent = getLicensesMetadataFromXContent(createParser(builder));
        assertThat(licensesMetadataFromXContent.getLicense(), equalTo(trialLicense));
        assertEquals(licensesMetadataFromXContent.getMostRecentTrialVersion(), Version.CURRENT);
    }

    public void testLicenseTombstoneFromXContext() throws Exception {
        final XContentBuilder builder = XContentFactory.jsonBuilder();
        builder.startObject();
        builder.startObject("licenses");
        builder.nullField("license");
        builder.endObject();
        builder.endObject();
        LicensesMetadata metadataFromXContent = getLicensesMetadataFromXContent(createParser(builder));
        assertThat(metadataFromXContent.getLicense(), equalTo(LicensesMetadata.LICENSE_TOMBSTONE));
    }

    public void testLicenseTombstoneWithUsedTrialFromXContext() throws Exception {
        final XContentBuilder builder = XContentFactory.jsonBuilder();
        builder.startObject();
        builder.startObject("licenses");
        builder.nullField("license");
        builder.field("trial_license", Version.CURRENT.toString());
        builder.endObject();
        builder.endObject();
        LicensesMetadata metadataFromXContent = getLicensesMetadataFromXContent(createParser(builder));
        assertThat(metadataFromXContent.getLicense(), equalTo(LicensesMetadata.LICENSE_TOMBSTONE));
        assertEquals(metadataFromXContent.getMostRecentTrialVersion(), Version.CURRENT);
    }

    private static LicensesMetadata getLicensesMetadataFromXContent(XContentParser parser) throws Exception {
        parser.nextToken(); // consume null
        parser.nextToken(); // consume "licenses"
        LicensesMetadata licensesMetadataFromXContent = LicensesMetadata.fromXContent(parser);
        parser.nextToken(); // consume endObject
        assertThat(parser.nextToken(), nullValue());
        return licensesMetadataFromXContent;
    }

    @Override
    protected NamedXContentRegistry xContentRegistry() {
        return new NamedXContentRegistry(Stream.concat(
                new Licensing(Settings.EMPTY).getNamedXContent().stream(),
                ClusterModule.getNamedXWriteables().stream()
        ).collect(Collectors.toList()));
    }
}
