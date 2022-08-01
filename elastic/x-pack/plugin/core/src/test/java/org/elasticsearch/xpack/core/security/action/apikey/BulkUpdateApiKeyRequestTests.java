/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.core.security.action.apikey;

import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.core.security.authz.RoleDescriptor;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.containsStringIgnoringCase;
import static org.hamcrest.Matchers.equalTo;

public class BulkUpdateApiKeyRequestTests extends ESTestCase {

    public void testSerialization() throws IOException {
        final boolean roleDescriptorsPresent = randomBoolean();
        final List<RoleDescriptor> descriptorList;
        if (roleDescriptorsPresent == false) {
            descriptorList = null;
        } else {
            final int numDescriptors = randomIntBetween(0, 4);
            descriptorList = new ArrayList<>();
            for (int i = 0; i < numDescriptors; i++) {
                descriptorList.add(new RoleDescriptor("role_" + i, new String[] { "all" }, null, null));
            }
        }

        final List<String> ids = randomList(1, 5, () -> randomAlphaOfLength(10));
        final Map<String, Object> metadata = ApiKeyTests.randomMetadata();
        final var request = new BulkUpdateApiKeyRequest(ids, descriptorList, metadata);

        try (BytesStreamOutput out = new BytesStreamOutput()) {
            request.writeTo(out);
            try (StreamInput in = out.bytes().streamInput()) {
                final var serialized = new BulkUpdateApiKeyRequest(in);
                assertEquals(ids, serialized.getIds());
                assertEquals(descriptorList, serialized.getRoleDescriptors());
                assertEquals(metadata, request.getMetadata());
            }
        }
    }

    public void testNullValuesValidForNonIds() {
        final var request = BulkUpdateApiKeyRequest.usingApiKeyIds("id");
        assertNull(request.validate());
    }

    public void testEmptyIdsNotValid() {
        final var request = new BulkUpdateApiKeyRequest(List.of(), null, null);
        final ActionRequestValidationException ve = request.validate();
        assertNotNull(ve);
        assertThat(ve.validationErrors().size(), equalTo(1));
        assertThat(ve.validationErrors().get(0), containsString("Field [ids] cannot be empty"));
    }

    public void testMetadataKeyValidation() {
        final var reservedKey = "_" + randomAlphaOfLengthBetween(0, 10);
        final var metadataValue = randomAlphaOfLengthBetween(1, 10);
        final var request = new BulkUpdateApiKeyRequest(
            randomList(1, 5, () -> randomAlphaOfLength(10)),
            null,
            Map.of(reservedKey, metadataValue)
        );
        final ActionRequestValidationException ve = request.validate();
        assertNotNull(ve);
        assertThat(ve.validationErrors().size(), equalTo(1));
        assertThat(ve.validationErrors().get(0), containsString("API key metadata keys may not start with [_]"));
    }

    public void testRoleDescriptorValidation() {
        final var request = new BulkUpdateApiKeyRequest(
            randomList(1, 5, () -> randomAlphaOfLength(10)),
            List.of(
                new RoleDescriptor(
                    randomAlphaOfLength(5),
                    new String[] { "manage_index_template" },
                    new RoleDescriptor.IndicesPrivileges[] {
                        RoleDescriptor.IndicesPrivileges.builder().indices("*").privileges("rad").build() },
                    new RoleDescriptor.ApplicationResourcePrivileges[] {
                        RoleDescriptor.ApplicationResourcePrivileges.builder()
                            .application(randomFrom("app*tab", "app 1"))
                            .privileges(randomFrom(" ", "\n"))
                            .resources("resource")
                            .build() },
                    null,
                    null,
                    Map.of("_key", "value"),
                    null
                )
            ),
            null
        );
        final ActionRequestValidationException ve = request.validate();
        assertNotNull(ve);
        assertThat(ve.validationErrors().get(0), containsString("unknown cluster privilege"));
        assertThat(ve.validationErrors().get(1), containsString("unknown index privilege"));
        assertThat(ve.validationErrors().get(2), containsStringIgnoringCase("application name"));
        assertThat(ve.validationErrors().get(3), containsStringIgnoringCase("Application privilege names"));
        assertThat(ve.validationErrors().get(4), containsStringIgnoringCase("role descriptor metadata keys may not start with "));
    }
}
