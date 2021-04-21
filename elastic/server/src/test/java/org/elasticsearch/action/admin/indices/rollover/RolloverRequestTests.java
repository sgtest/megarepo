/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.admin.indices.rollover;

import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.admin.indices.create.CreateIndexRequest;
import org.elasticsearch.common.RestApiVersion;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.io.stream.NamedWriteableAwareStreamInput;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.XContentParseException;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.indices.IndicesModule;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.test.XContentTestUtils;
import org.junit.Before;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.function.Consumer;

import static org.hamcrest.CoreMatchers.containsString;
import static org.hamcrest.Matchers.equalTo;

public class RolloverRequestTests extends ESTestCase {
    private NamedWriteableRegistry writeableRegistry;

    @Override
    @Before
    public void setUp() throws Exception {
        super.setUp();
        writeableRegistry = new NamedWriteableRegistry(IndicesModule.getNamedWriteables());
    }

    public void testConditionsParsing() throws Exception {
        final RolloverRequest request = new RolloverRequest(randomAlphaOfLength(10), randomAlphaOfLength(10));
        final XContentBuilder builder = XContentFactory.jsonBuilder()
            .startObject()
                .startObject("conditions")
                    .field("max_age", "10d")
                    .field("max_docs", 100)
                    .field("max_size", "45gb")
                    .field("max_primary_shard_size", "55gb")
                .endObject()
            .endObject();
        request.fromXContent(false, createParser(builder));
        Map<String, Condition<?>> conditions = request.getConditions();
        assertThat(conditions.size(), equalTo(4));
        MaxAgeCondition maxAgeCondition = (MaxAgeCondition)conditions.get(MaxAgeCondition.NAME);
        assertThat(maxAgeCondition.value.getMillis(), equalTo(TimeValue.timeValueHours(24 * 10).getMillis()));
        MaxDocsCondition maxDocsCondition = (MaxDocsCondition)conditions.get(MaxDocsCondition.NAME);
        assertThat(maxDocsCondition.value, equalTo(100L));
        MaxSizeCondition maxSizeCondition = (MaxSizeCondition)conditions.get(MaxSizeCondition.NAME);
        assertThat(maxSizeCondition.value.getBytes(), equalTo(ByteSizeUnit.GB.toBytes(45)));
        MaxPrimaryShardSizeCondition maxPrimaryShardSizeCondition =
            (MaxPrimaryShardSizeCondition)conditions.get(MaxPrimaryShardSizeCondition.NAME);
        assertThat(maxPrimaryShardSizeCondition.value.getBytes(), equalTo(ByteSizeUnit.GB.toBytes(55)));
    }

    public void testParsingWithIndexSettings() throws Exception {
        final RolloverRequest request = new RolloverRequest(randomAlphaOfLength(10), randomAlphaOfLength(10));
        final XContentBuilder builder = XContentFactory.jsonBuilder()
            .startObject()
                .startObject("conditions")
                    .field("max_age", "10d")
                    .field("max_docs", 100)
                .endObject()
                .startObject("mappings")
                    .startObject("properties")
                        .startObject("field1")
                            .field("type", "string")
                            .field("index", "not_analyzed")
                        .endObject()
                    .endObject()
                .endObject()
                .startObject("settings")
                    .field("number_of_shards", 10)
                .endObject()
                .startObject("aliases")
                    .startObject("alias1").endObject()
                .endObject()
            .endObject();
        request.fromXContent(false, createParser(builder));
        Map<String, Condition<?>> conditions = request.getConditions();
        assertThat(conditions.size(), equalTo(2));
        assertThat(request.getCreateIndexRequest().mappings(), containsString("not_analyzed"));
        assertThat(request.getCreateIndexRequest().aliases().size(), equalTo(1));
        assertThat(request.getCreateIndexRequest().settings().getAsInt("number_of_shards", 0), equalTo(10));
    }

    public void testTypelessMappingParsing() throws Exception {
        final RolloverRequest request = new RolloverRequest(randomAlphaOfLength(10), randomAlphaOfLength(10));
        final XContentBuilder builder = XContentFactory.jsonBuilder()
            .startObject()
                .startObject("mappings")
                    .startObject("properties")
                        .startObject("field1")
                            .field("type", "keyword")
                        .endObject()
                    .endObject()
                .endObject()
            .endObject();

        request.fromXContent(false, createParser(builder));

        CreateIndexRequest createIndexRequest = request.getCreateIndexRequest();
        String mapping = createIndexRequest.mappings();
        assertNotNull(mapping);

        Map<String, Object> parsedMapping = XContentHelper.convertToMap(
            new BytesArray(mapping), false, XContentType.JSON).v2();

        @SuppressWarnings("unchecked")
        Map<String, Object> properties = (Map<String, Object>) parsedMapping.get(MapperService.SINGLE_MAPPING_NAME);
        assertNotNull(properties);
        assertFalse(properties.isEmpty());
    }

    public void testSerialize() throws Exception {
        RolloverRequest originalRequest = new RolloverRequest("alias-index", "new-index-name");
        originalRequest.addMaxIndexDocsCondition(randomNonNegativeLong());
        originalRequest.addMaxIndexAgeCondition(TimeValue.timeValueNanos(randomNonNegativeLong()));
        originalRequest.addMaxIndexSizeCondition(new ByteSizeValue(randomNonNegativeLong()));
        try (BytesStreamOutput out = new BytesStreamOutput()) {
            originalRequest.writeTo(out);
            BytesReference bytes = out.bytes();
            try (StreamInput in = new NamedWriteableAwareStreamInput(bytes.streamInput(), writeableRegistry)) {
                RolloverRequest cloneRequest = new RolloverRequest(in);
                assertThat(cloneRequest.getNewIndexName(), equalTo(originalRequest.getNewIndexName()));
                assertThat(cloneRequest.getRolloverTarget(), equalTo(originalRequest.getRolloverTarget()));
                for (Map.Entry<String, Condition<?>> entry : cloneRequest.getConditions().entrySet()) {
                    Condition<?> condition = originalRequest.getConditions().get(entry.getKey());
                    //here we compare the string representation as there is some information loss when serializing
                    //and de-serializing MaxAgeCondition
                    assertEquals(condition.toString(), entry.getValue().toString());
                }
            }
        }
    }

    public void testUnknownFields() throws IOException {
        final RolloverRequest request = new RolloverRequest();
        XContentType xContentType = randomFrom(XContentType.values());
        final XContentBuilder builder = XContentFactory.contentBuilder(xContentType);
        builder.startObject();
        {
            builder.startObject("conditions");
            builder.field("max_age", "10d");
            builder.endObject();
        }
        builder.endObject();
        BytesReference mutated = XContentTestUtils.insertRandomFields(xContentType, BytesReference.bytes(builder), null, random());
        expectThrows(XContentParseException.class, () -> request.fromXContent(false, createParser(xContentType.xContent(), mutated)));
    }

    public void testSameConditionCanOnlyBeAddedOnce() {
        RolloverRequest rolloverRequest = new RolloverRequest();
        Consumer<RolloverRequest> rolloverRequestConsumer = randomFrom(conditionsGenerator);
        rolloverRequestConsumer.accept(rolloverRequest);
        expectThrows(IllegalArgumentException.class, () -> rolloverRequestConsumer.accept(rolloverRequest));
    }

    public void testValidation() {
        RolloverRequest rolloverRequest = new RolloverRequest();
        assertNotNull(rolloverRequest.getCreateIndexRequest());
        ActionRequestValidationException validationException = rolloverRequest.validate();
        assertNotNull(validationException);
        assertEquals(1, validationException.validationErrors().size());
        assertEquals("rollover target is missing", validationException.validationErrors().get(0));
    }

    public void testParsingWithType() throws Exception {
        final XContentBuilder builder = XContentFactory.jsonBuilder()
            .startObject()
                .startObject("conditions")
                    .field("max_age", "10d")
                    .field("max_docs", 100)
                .endObject()
                .startObject("mappings")
                    .startObject("type1")
                        .startObject("properties")
                            .startObject("field1")
                                .field("type", "string")
                                .field("index", "not_analyzed")
                            .endObject()
                        .endObject()
                    .endObject()
                .endObject()
                .startObject("settings")
                    .field("number_of_shards", 10)
                .endObject()
                .startObject("aliases")
                    .startObject("alias1").endObject()
                .endObject()
            .endObject();

        try (XContentParser parser = createParserWithCompatibilityFor(JsonXContent.jsonXContent,
            BytesReference.bytes(builder).utf8ToString(), RestApiVersion.V_7)) {
            final RolloverRequest request = new RolloverRequest(randomAlphaOfLength(10), randomAlphaOfLength(10));
            request.fromXContent(true, parser);
            Map<String, Condition<?>> conditions = request.getConditions();
            assertThat(conditions.size(), equalTo(2));
            assertThat(request.getCreateIndexRequest().mappings(),
                equalTo("{\"_doc\":{\"properties\":{\"field1\":{\"index\":\"not_analyzed\",\"type\":\"string\"}}}}"));
        }
    }

    public void testTypedRequestWithoutIncludeTypeName() throws IOException {
        final XContentBuilder builder = XContentFactory.jsonBuilder()
            .startObject()
                .startObject("mappings")
                    .startObject("_doc")
                        .startObject("properties")
                            .startObject("field1")
                                .field("type", "string")
                                .field("index", "not_analyzed")
                            .endObject()
                        .endObject()
                    .endObject()
                .endObject()
            .endObject();
        try (XContentParser parser = createParserWithCompatibilityFor(JsonXContent.jsonXContent,
            BytesReference.bytes(builder).utf8ToString(), RestApiVersion.V_7)) {
            final RolloverRequest request = new RolloverRequest(randomAlphaOfLength(10), randomAlphaOfLength(10));
            expectThrows(IllegalArgumentException.class, () -> request.fromXContent(false, parser));
        }
    }

    private static List<Consumer<RolloverRequest>> conditionsGenerator = new ArrayList<>();
    static {
        conditionsGenerator.add((request) -> request.addMaxIndexDocsCondition(randomNonNegativeLong()));
        conditionsGenerator.add((request) -> request.addMaxIndexSizeCondition(new ByteSizeValue(randomNonNegativeLong())));
        conditionsGenerator.add((request) -> request.addMaxIndexAgeCondition(new TimeValue(randomNonNegativeLong())));
    }

}
