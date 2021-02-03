/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ingest;

import org.elasticsearch.ElasticsearchParseException;
import org.elasticsearch.test.ESTestCase;
import org.junit.Before;

import java.util.HashMap;
import java.util.Map;

import static org.hamcrest.CoreMatchers.equalTo;

public class UriPartsProcessorFactoryTests extends ESTestCase {

    private UriPartsProcessor.Factory factory;

    @Before
    public void init() {
        factory = new UriPartsProcessor.Factory();
    }

    public void testCreate() throws Exception {
        Map<String, Object> config = new HashMap<>();
        String field = randomAlphaOfLength(6);
        config.put("field", field);
        String targetField = "url";
        if (randomBoolean()) {
            targetField = randomAlphaOfLength(6);
            config.put("target_field", targetField);
        }
        boolean removeIfSuccessful = randomBoolean();
        config.put("remove_if_successful", removeIfSuccessful);
        boolean keepOriginal = randomBoolean();
        config.put("keep_original", keepOriginal);

        String processorTag = randomAlphaOfLength(10);
        UriPartsProcessor uriPartsProcessor = factory.create(null, processorTag, null, config);
        assertThat(uriPartsProcessor.getTag(), equalTo(processorTag));
        assertThat(uriPartsProcessor.getField(), equalTo(field));
        assertThat(uriPartsProcessor.getTargetField(), equalTo(targetField));
        assertThat(uriPartsProcessor.getRemoveIfSuccessful(), equalTo(removeIfSuccessful));
        assertThat(uriPartsProcessor.getKeepOriginal(), equalTo(keepOriginal));
    }

    public void testCreateNoFieldPresent() throws Exception {
        Map<String, Object> config = new HashMap<>();
        config.put("value", "value1");
        try {
            factory.create(null, null, null, config);
            fail("factory create should have failed");
        } catch (ElasticsearchParseException e) {
            assertThat(e.getMessage(), equalTo("[field] required property is missing"));
        }
    }

    public void testCreateNullField() throws Exception {
        Map<String, Object> config = new HashMap<>();
        config.put("field", null);
        try {
            factory.create(null, null, null, config);
            fail("factory create should have failed");
        } catch (ElasticsearchParseException e) {
            assertThat(e.getMessage(), equalTo("[field] required property is missing"));
        }
    }
}
