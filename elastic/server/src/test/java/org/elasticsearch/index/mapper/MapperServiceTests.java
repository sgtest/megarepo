/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.index.mapper;

import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.compress.CompressedXContent;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.index.mapper.MapperService.MergeReason;

import java.io.IOException;

import static org.hamcrest.CoreMatchers.containsString;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;

public class MapperServiceTests extends MapperServiceTestCase {

    public void testPreflightUpdateDoesNotChangeMapping() throws Throwable {
        final MapperService mapperService = createMapperService(mapping(b -> {}));
        merge(mapperService, MergeReason.MAPPING_UPDATE_PREFLIGHT, mapping(b -> createMappingSpecifyingNumberOfFields(b, 1)));
        assertThat("field was not created by preflight check", mapperService.fieldType("field0"), nullValue());
        merge(mapperService, MergeReason.MAPPING_UPDATE, mapping(b -> createMappingSpecifyingNumberOfFields(b, 1)));
        assertThat("field was not created by mapping update", mapperService.fieldType("field0"), notNullValue());
    }

    /**
     * Test that we can have at least the number of fields in new mappings that are defined by "index.mapping.total_fields.limit".
     * Any additional field should trigger an IllegalArgumentException.
     */
    public void testTotalFieldsLimit() throws Throwable {
        int totalFieldsLimit = randomIntBetween(1, 10);
        Settings settings = Settings.builder().put(MapperService.INDEX_MAPPING_TOTAL_FIELDS_LIMIT_SETTING.getKey(), totalFieldsLimit)
            .build();
        MapperService mapperService
            = createMapperService(settings, mapping(b -> createMappingSpecifyingNumberOfFields(b, totalFieldsLimit)));

        // adding one more field should trigger exception
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
            () -> merge(mapperService, mapping(b -> b.startObject("newfield").field("type", "long").endObject()))
        );
        assertTrue(e.getMessage(),
                e.getMessage().contains("Limit of total fields [" + totalFieldsLimit + "] has been exceeded"));
    }

    private void createMappingSpecifyingNumberOfFields(XContentBuilder b, int numberOfFields) throws IOException {
        for (int i = 0; i < numberOfFields; i++) {
            b.startObject("field" + i);
            b.field("type", randomFrom("long", "integer", "date", "keyword", "text"));
            b.endObject();
        }
    }

    public void testMappingDepthExceedsLimit() throws Throwable {

        Settings settings = Settings.builder().put(MapperService.INDEX_MAPPING_DEPTH_LIMIT_SETTING.getKey(), 1).build();
        MapperService mapperService = createMapperService(settings, mapping(b -> {}));

        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> merge(mapperService, mapping((b -> {
            b.startObject("object1");
            b.field("type", "object");
            b.endObject();
        }))));
        assertThat(e.getMessage(), containsString("Limit of mapping depth [1] has been exceeded"));
    }

    public void testPartitionedConstraints() throws IOException {
        // partitioned index must have routing
        Settings settings = Settings.builder()
            .put("index.number_of_shards", 4)
            .put("index.routing_partition_size", 2)
            .build();
        Exception e = expectThrows(IllegalArgumentException.class, () -> createMapperService(settings, mapping(b -> {})));
        assertThat(e.getMessage(), containsString("must have routing"));

        // valid partitioned index
        createMapperService(settings, topMapping(b -> b.startObject("_routing").field("required", true).endObject()));
    }

    public void testIndexSortWithNestedFields() throws IOException {
        Settings settings = Settings.builder()
            .put("index.sort.field", "foo")
            .build();
        IllegalArgumentException invalidNestedException = expectThrows(IllegalArgumentException.class,
            () -> createMapperService(settings, mapping(b -> {
               b.startObject("nested_field").field("type", "nested").endObject();
               b.startObject("foo").field("type", "keyword").endObject();
            })));

        assertThat(invalidNestedException.getMessage(),
            containsString("cannot have nested fields when index sort is activated"));

        MapperService mapperService
            = createMapperService(settings, mapping(b -> b.startObject("foo").field("type", "keyword").endObject()));
        invalidNestedException = expectThrows(IllegalArgumentException.class, () -> merge(mapperService, mapping(b -> {
            b.startObject("nested_field");
            b.field("type", "nested");
            b.endObject();
        })));
        assertThat(invalidNestedException.getMessage(),
            containsString("cannot have nested fields when index sort is activated"));
    }

     public void testFieldAliasWithMismatchedNestedScope() throws Throwable {
        MapperService mapperService = createMapperService(mapping(b -> {
            b.startObject("nested");
            {
                b.field("type", "nested");
                b.startObject("properties");
                {
                    b.startObject("field").field("type", "text").endObject();
                }
                b.endObject();
            }
            b.endObject();
        }));

        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> merge(mapperService, mapping(b -> {
            b.startObject("alias");
            {
                b.field("type", "alias");
                b.field("path", "nested.field");
            }
            b.endObject();
        })));
        assertThat(e.getMessage(), containsString("Invalid [path] value [nested.field] for field alias [alias]"));
    }

    public void testTotalFieldsLimitWithFieldAlias() throws Throwable {

        int numberOfFieldsIncludingAlias = 2;

        Settings settings = Settings.builder()
            .put(MapperService.INDEX_MAPPING_TOTAL_FIELDS_LIMIT_SETTING.getKey(), numberOfFieldsIncludingAlias).build();
        createMapperService(settings, mapping(b -> {
            b.startObject("alias").field("type", "alias").field("path", "field").endObject();
            b.startObject("field").field("type", "text").endObject();
        }));

        // Set the total fields limit to the number of non-alias fields, to verify that adding
        // a field alias pushes the mapping over the limit.
        int numberOfNonAliasFields = 1;
        Settings errorSettings = Settings.builder()
            .put(MapperService.INDEX_MAPPING_TOTAL_FIELDS_LIMIT_SETTING.getKey(), numberOfNonAliasFields).build();
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> createMapperService(errorSettings, mapping(b -> {
            b.startObject("alias").field("type", "alias").field("path", "field").endObject();
            b.startObject("field").field("type", "text").endObject();
        })));
        assertEquals("Limit of total fields [" + numberOfNonAliasFields + "] has been exceeded", e.getMessage());
    }

    public void testFieldNameLengthLimit() throws Throwable {
        int maxFieldNameLength = randomIntBetween(15, 20);
        String testString = new String(new char[maxFieldNameLength + 1]).replace("\0", "a");
        Settings settings = Settings.builder().put(MapperService.INDEX_MAPPING_FIELD_NAME_LENGTH_LIMIT_SETTING.getKey(), maxFieldNameLength)
            .build();
        MapperService mapperService = createMapperService(settings, fieldMapping(b -> b.field("type", "text")));

        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
            () -> merge(mapperService, mapping(b -> b.startObject(testString).field("type", "text").endObject())));

        assertEquals("Field name [" + testString + "] is longer than the limit of [" + maxFieldNameLength + "] characters",
            e.getMessage());
    }

    public void testObjectNameLengthLimit() throws Throwable {
        int maxFieldNameLength = randomIntBetween(15, 20);
        String testString = new String(new char[maxFieldNameLength + 1]).replace("\0", "a");
        Settings settings = Settings.builder().put(MapperService.INDEX_MAPPING_FIELD_NAME_LENGTH_LIMIT_SETTING.getKey(), maxFieldNameLength)
            .build();
        MapperService mapperService = createMapperService(settings, mapping(b -> {}));

        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
            () -> merge(mapperService, mapping(b -> b.startObject(testString).field("type", "object").endObject())));

        assertEquals("Field name [" + testString + "] is longer than the limit of [" + maxFieldNameLength + "] characters",
            e.getMessage());
    }

    public void testAliasFieldNameLengthLimit() throws Throwable {
        int maxFieldNameLength = randomIntBetween(15, 20);
        String testString = new String(new char[maxFieldNameLength + 1]).replace("\0", "a");
        Settings settings = Settings.builder().put(MapperService.INDEX_MAPPING_FIELD_NAME_LENGTH_LIMIT_SETTING.getKey(), maxFieldNameLength)
            .build();
        MapperService mapperService = createMapperService(settings, mapping(b -> {}));

        IllegalArgumentException e = expectThrows(IllegalArgumentException.class, () -> merge(mapperService, mapping(b -> {
            b.startObject(testString).field("type", "alias").field("path", "field").endObject();
            b.startObject("field").field("type", "text").endObject();
        })));

        assertEquals("Field name [" + testString + "] is longer than the limit of [" + maxFieldNameLength + "] characters",
            e.getMessage());
    }

    public void testMappingRecoverySkipFieldNameLengthLimit() throws Throwable {
        int maxFieldNameLength = randomIntBetween(15, 20);
        String testString = new String(new char[maxFieldNameLength + 1]).replace("\0", "a");
        Settings settings = Settings.builder().put(MapperService.INDEX_MAPPING_FIELD_NAME_LENGTH_LIMIT_SETTING.getKey(), maxFieldNameLength)
            .build();
        MapperService mapperService = createMapperService(settings, mapping(b -> {}));

        CompressedXContent mapping = new CompressedXContent(BytesReference.bytes(
            XContentFactory.jsonBuilder().startObject().startObject("_doc")
                .startObject("properties")
                    .startObject(testString)
                        .field("type", "text")
                    .endObject()
                .endObject()
            .endObject().endObject()));

        DocumentMapper documentMapper = mapperService.merge("_doc", mapping, MergeReason.MAPPING_RECOVERY);

        assertEquals(testString, documentMapper.mappers().getMapper(testString).simpleName());
    }

}
