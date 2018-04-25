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

package org.elasticsearch.cluster.metadata;

import org.elasticsearch.Version;
import org.elasticsearch.cluster.ClusterModule;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.UUIDs;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.collect.ImmutableOpenMap;
import org.elasticsearch.common.io.stream.BytesStreamOutput;
import org.elasticsearch.common.io.stream.NamedWriteableAwareStreamInput;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentHelper;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.index.Index;
import org.elasticsearch.plugins.MapperPlugin;
import org.elasticsearch.test.ESTestCase;

import java.io.IOException;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.startsWith;

public class MetaDataTests extends ESTestCase {

    public void testIndexAndAliasWithSameName() {
        IndexMetaData.Builder builder = IndexMetaData.builder("index")
                .settings(Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT))
                .numberOfShards(1)
                .numberOfReplicas(0)
                .putAlias(AliasMetaData.builder("index").build());
        try {
            MetaData.builder().put(builder).build();
            fail("exception should have been thrown");
        } catch (IllegalStateException e) {
            assertThat(e.getMessage(), equalTo("index and alias names need to be unique, but the following duplicates were found [index (alias of [index])]"));
        }
    }

    public void testAliasCollidingWithAnExistingIndex() {
        int indexCount = randomIntBetween(10, 100);
        Set<String> indices = new HashSet<>(indexCount);
        for (int i = 0; i < indexCount; i++) {
            indices.add(randomAlphaOfLength(10));
        }
        Map<String, Set<String>> aliasToIndices = new HashMap<>();
        for (String alias: randomSubsetOf(randomIntBetween(1, 10), indices)) {
            aliasToIndices.put(alias, new HashSet<>(randomSubsetOf(randomIntBetween(1, 3), indices)));
        }
        int properAliases = randomIntBetween(0, 3);
        for (int i = 0; i < properAliases; i++) {
            aliasToIndices.put(randomAlphaOfLength(5), new HashSet<>(randomSubsetOf(randomIntBetween(1, 3), indices)));
        }
        MetaData.Builder metaDataBuilder = MetaData.builder();
        for (String index : indices) {
            IndexMetaData.Builder indexBuilder = IndexMetaData.builder(index)
                .settings(Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT))
                .numberOfShards(1)
                .numberOfReplicas(0);
            aliasToIndices.forEach((key, value) -> {
                if (value.contains(index)) {
                    indexBuilder.putAlias(AliasMetaData.builder(key).build());
                }
            });
            metaDataBuilder.put(indexBuilder);
        }
        try {
            metaDataBuilder.build();
            fail("exception should have been thrown");
        } catch (IllegalStateException e) {
            assertThat(e.getMessage(), startsWith("index and alias names need to be unique"));
        }
    }

    public void testResolveIndexRouting() {
        IndexMetaData.Builder builder = IndexMetaData.builder("index")
                .settings(Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT))
                .numberOfShards(1)
                .numberOfReplicas(0)
                .putAlias(AliasMetaData.builder("alias0").build())
                .putAlias(AliasMetaData.builder("alias1").routing("1").build())
                .putAlias(AliasMetaData.builder("alias2").routing("1,2").build());
        MetaData metaData = MetaData.builder().put(builder).build();

        // no alias, no index
        assertEquals(metaData.resolveIndexRouting(null, null), null);
        assertEquals(metaData.resolveIndexRouting("0", null), "0");

        // index, no alias
        assertEquals(metaData.resolveIndexRouting(null, "index"), null);
        assertEquals(metaData.resolveIndexRouting("0", "index"), "0");

        // alias with no index routing
        assertEquals(metaData.resolveIndexRouting(null, "alias0"), null);
        assertEquals(metaData.resolveIndexRouting("0", "alias0"), "0");

        // alias with index routing.
        assertEquals(metaData.resolveIndexRouting(null, "alias1"), "1");
        try {
            metaData.resolveIndexRouting("0", "alias1");
            fail("should fail");
        } catch (IllegalArgumentException ex) {
            assertThat(ex.getMessage(), is("Alias [alias1] has index routing associated with it [1], and was provided with routing value [0], rejecting operation"));
        }

        // alias with invalid index routing.
        try {
            metaData.resolveIndexRouting(null, "alias2");
            fail("should fail");
        } catch (IllegalArgumentException ex) {
            assertThat(ex.getMessage(), is("index/alias [alias2] provided with routing value [1,2] that resolved to several routing values, rejecting operation"));
        }

        try {
            metaData.resolveIndexRouting("1", "alias2");
            fail("should fail");
        } catch (IllegalArgumentException ex) {
            assertThat(ex.getMessage(), is("index/alias [alias2] provided with routing value [1,2] that resolved to several routing values, rejecting operation"));
        }
    }

    public void testUnknownFieldClusterMetaData() throws IOException {
        BytesReference metadata = BytesReference.bytes(JsonXContent.contentBuilder()
            .startObject()
                .startObject("meta-data")
                    .field("random", "value")
                .endObject()
            .endObject());
        XContentParser parser = createParser(JsonXContent.jsonXContent, metadata);
        try {
            MetaData.Builder.fromXContent(parser);
            fail();
        } catch (IllegalArgumentException e) {
            assertEquals("Unexpected field [random]", e.getMessage());
        }
    }

    public void testUnknownFieldIndexMetaData() throws IOException {
        BytesReference metadata = BytesReference.bytes(JsonXContent.contentBuilder()
            .startObject()
                .startObject("index_name")
                    .field("random", "value")
                .endObject()
            .endObject());
        XContentParser parser = createParser(JsonXContent.jsonXContent, metadata);
        try {
            IndexMetaData.Builder.fromXContent(parser);
            fail();
        } catch (IllegalArgumentException e) {
            assertEquals("Unexpected field [random]", e.getMessage());
        }
    }

    public void testMetaDataGlobalStateChangesOnIndexDeletions() {
        IndexGraveyard.Builder builder = IndexGraveyard.builder();
        builder.addTombstone(new Index("idx1", UUIDs.randomBase64UUID()));
        final MetaData metaData1 = MetaData.builder().indexGraveyard(builder.build()).build();
        builder = IndexGraveyard.builder(metaData1.indexGraveyard());
        builder.addTombstone(new Index("idx2", UUIDs.randomBase64UUID()));
        final MetaData metaData2 = MetaData.builder(metaData1).indexGraveyard(builder.build()).build();
        assertFalse("metadata not equal after adding index deletions", MetaData.isGlobalStateEquals(metaData1, metaData2));
        final MetaData metaData3 = MetaData.builder(metaData2).build();
        assertTrue("metadata equal when not adding index deletions", MetaData.isGlobalStateEquals(metaData2, metaData3));
    }

    public void testXContentWithIndexGraveyard() throws IOException {
        final IndexGraveyard graveyard = IndexGraveyardTests.createRandom();
        final MetaData originalMeta = MetaData.builder().indexGraveyard(graveyard).build();
        final XContentBuilder builder = JsonXContent.contentBuilder();
        builder.startObject();
        originalMeta.toXContent(builder, ToXContent.EMPTY_PARAMS);
        builder.endObject();
        XContentParser parser = createParser(JsonXContent.jsonXContent, BytesReference.bytes(builder));
        final MetaData fromXContentMeta = MetaData.fromXContent(parser);
        assertThat(fromXContentMeta.indexGraveyard(), equalTo(originalMeta.indexGraveyard()));
    }

    public void testSerializationWithIndexGraveyard() throws IOException {
        final IndexGraveyard graveyard = IndexGraveyardTests.createRandom();
        final MetaData originalMeta = MetaData.builder().indexGraveyard(graveyard).build();
        final BytesStreamOutput out = new BytesStreamOutput();
        originalMeta.writeTo(out);
        NamedWriteableRegistry namedWriteableRegistry = new NamedWriteableRegistry(ClusterModule.getNamedWriteables());
        final MetaData fromStreamMeta = MetaData.readFrom(
            new NamedWriteableAwareStreamInput(out.bytes().streamInput(), namedWriteableRegistry)
        );
        assertThat(fromStreamMeta.indexGraveyard(), equalTo(fromStreamMeta.indexGraveyard()));
    }

    public void testFindMappings() throws IOException {
        MetaData metaData = MetaData.builder()
                .put(IndexMetaData.builder("index1")
                    .settings(Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
                        .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1).put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0))
                    .putMapping("_doc", FIND_MAPPINGS_TEST_ITEM))
                .put(IndexMetaData.builder("index2")
                    .settings(Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
                        .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1).put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0))
                    .putMapping("_doc", FIND_MAPPINGS_TEST_ITEM)).build();

        {
            ImmutableOpenMap<String, ImmutableOpenMap<String, MappingMetaData>> mappings = metaData.findMappings(Strings.EMPTY_ARRAY,
                    Strings.EMPTY_ARRAY, MapperPlugin.NOOP_FIELD_FILTER);
            assertEquals(0, mappings.size());
        }
        {
            ImmutableOpenMap<String, ImmutableOpenMap<String, MappingMetaData>> mappings = metaData.findMappings(new String[]{"index1"},
                     new String[]{"notfound"}, MapperPlugin.NOOP_FIELD_FILTER);
            assertEquals(0, mappings.size());
        }
        {
            ImmutableOpenMap<String, ImmutableOpenMap<String, MappingMetaData>> mappings = metaData.findMappings(new String[]{"index1"},
                    Strings.EMPTY_ARRAY, MapperPlugin.NOOP_FIELD_FILTER);
            assertEquals(1, mappings.size());
            assertIndexMappingsNotFiltered(mappings, "index1");
        }
        {
            ImmutableOpenMap<String, ImmutableOpenMap<String, MappingMetaData>> mappings = metaData.findMappings(
                    new String[]{"index1", "index2"},
                    new String[]{randomBoolean() ? "_doc" : "_all"}, MapperPlugin.NOOP_FIELD_FILTER);
            assertEquals(2, mappings.size());
            assertIndexMappingsNotFiltered(mappings, "index1");
            assertIndexMappingsNotFiltered(mappings, "index2");
        }
    }

    public void testFindMappingsNoOpFilters() throws IOException {
        MappingMetaData originalMappingMetaData = new MappingMetaData("_doc",
                XContentHelper.convertToMap(JsonXContent.jsonXContent, FIND_MAPPINGS_TEST_ITEM, true));

        MetaData metaData = MetaData.builder()
                .put(IndexMetaData.builder("index1")
                        .settings(Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
                                .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1).put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0))
                        .putMapping(originalMappingMetaData)).build();

        {
            ImmutableOpenMap<String, ImmutableOpenMap<String, MappingMetaData>> mappings = metaData.findMappings(new String[]{"index1"},
                    randomBoolean() ? Strings.EMPTY_ARRAY : new String[]{"_all"}, MapperPlugin.NOOP_FIELD_FILTER);
            ImmutableOpenMap<String, MappingMetaData> index1 = mappings.get("index1");
            MappingMetaData mappingMetaData = index1.get("_doc");
            assertSame(originalMappingMetaData, mappingMetaData);
        }
        {
            ImmutableOpenMap<String, ImmutableOpenMap<String, MappingMetaData>> mappings = metaData.findMappings(new String[]{"index1"},
                    randomBoolean() ? Strings.EMPTY_ARRAY : new String[]{"_all"}, index -> field -> randomBoolean());
            ImmutableOpenMap<String, MappingMetaData> index1 = mappings.get("index1");
            MappingMetaData mappingMetaData = index1.get("_doc");
            assertNotSame(originalMappingMetaData, mappingMetaData);
        }
        {
            ImmutableOpenMap<String, ImmutableOpenMap<String, MappingMetaData>> mappings = metaData.findMappings(new String[]{"index1"},
                    new String[]{"_doc"}, MapperPlugin.NOOP_FIELD_FILTER);
            ImmutableOpenMap<String, MappingMetaData> index1 = mappings.get("index1");
            MappingMetaData mappingMetaData = index1.get("_doc");
            assertSame(originalMappingMetaData, mappingMetaData);
        }
        {
            ImmutableOpenMap<String, ImmutableOpenMap<String, MappingMetaData>> mappings = metaData.findMappings(new String[]{"index1"},
                    new String[]{"_doc"}, index -> field -> randomBoolean());
            ImmutableOpenMap<String, MappingMetaData> index1 = mappings.get("index1");
            MappingMetaData mappingMetaData = index1.get("_doc");
            assertNotSame(originalMappingMetaData, mappingMetaData);
        }
    }

    @SuppressWarnings("unchecked")
    public void testFindMappingsWithFilters() throws IOException {
        String mapping = FIND_MAPPINGS_TEST_ITEM;
        if (randomBoolean()) {
            Map<String, Object> stringObjectMap = XContentHelper.convertToMap(JsonXContent.jsonXContent, FIND_MAPPINGS_TEST_ITEM, false);
            Map<String, Object> doc = (Map<String, Object>)stringObjectMap.get("_doc");
            try (XContentBuilder builder = JsonXContent.contentBuilder()) {
                builder.map(doc);
                mapping = Strings.toString(builder);
            }
        }

        MetaData metaData = MetaData.builder()
                .put(IndexMetaData.builder("index1")
                        .settings(Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
                        .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1).put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0))
                .putMapping("_doc", mapping))
                .put(IndexMetaData.builder("index2")
                        .settings(Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
                                .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1).put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0))
                        .putMapping("_doc", mapping))
                .put(IndexMetaData.builder("index3")
                        .settings(Settings.builder().put(IndexMetaData.SETTING_VERSION_CREATED, Version.CURRENT)
                                .put(IndexMetaData.SETTING_NUMBER_OF_SHARDS, 1).put(IndexMetaData.SETTING_NUMBER_OF_REPLICAS, 0))
                        .putMapping("_doc", mapping)).build();

        {
            ImmutableOpenMap<String, ImmutableOpenMap<String, MappingMetaData>> mappings = metaData.findMappings(
                    new String[]{"index1", "index2", "index3"},
                    new String[]{"_doc"}, index -> {
                        if (index.equals("index1")) {
                            return field -> field.startsWith("name.") == false && field.startsWith("properties.key.") == false
                                    && field.equals("age") == false && field.equals("address.location") == false;
                        }
                        if (index.equals("index2")) {
                            return field -> false;
                        }
                        return MapperPlugin.NOOP_FIELD_PREDICATE;
                    });



            assertIndexMappingsNoFields(mappings, "index2");
            assertIndexMappingsNotFiltered(mappings, "index3");

            ImmutableOpenMap<String, MappingMetaData> index1Mappings = mappings.get("index1");
            assertNotNull(index1Mappings);

            assertEquals(1, index1Mappings.size());
            MappingMetaData docMapping = index1Mappings.get("_doc");
            assertNotNull(docMapping);

            Map<String, Object> sourceAsMap = docMapping.getSourceAsMap();
            assertEquals(3, sourceAsMap.size());
            assertTrue(sourceAsMap.containsKey("_routing"));
            assertTrue(sourceAsMap.containsKey("_source"));

            Map<String, Object> typeProperties = (Map<String, Object>) sourceAsMap.get("properties");
            assertEquals(6, typeProperties.size());
            assertTrue(typeProperties.containsKey("birth"));
            assertTrue(typeProperties.containsKey("ip"));
            assertTrue(typeProperties.containsKey("suggest"));

            Map<String, Object> name = (Map<String, Object>) typeProperties.get("name");
            assertNotNull(name);
            assertEquals(1, name.size());
            Map<String, Object> nameProperties = (Map<String, Object>) name.get("properties");
            assertNotNull(nameProperties);
            assertEquals(0, nameProperties.size());

            Map<String, Object> address = (Map<String, Object>) typeProperties.get("address");
            assertNotNull(address);
            assertEquals(2, address.size());
            assertTrue(address.containsKey("type"));
            Map<String, Object> addressProperties = (Map<String, Object>) address.get("properties");
            assertNotNull(addressProperties);
            assertEquals(2, addressProperties.size());
            assertLeafs(addressProperties, "street", "area");

            Map<String, Object> properties = (Map<String, Object>) typeProperties.get("properties");
            assertNotNull(properties);
            assertEquals(2, properties.size());
            assertTrue(properties.containsKey("type"));
            Map<String, Object> propertiesProperties = (Map<String, Object>) properties.get("properties");
            assertNotNull(propertiesProperties);
            assertEquals(2, propertiesProperties.size());
            assertLeafs(propertiesProperties, "key");
            assertMultiField(propertiesProperties, "value", "keyword");
        }

        {
            ImmutableOpenMap<String, ImmutableOpenMap<String, MappingMetaData>> mappings = metaData.findMappings(
                    new String[]{"index1", "index2" , "index3"},
                    new String[]{"_doc"}, index -> field -> (index.equals("index3") && field.endsWith("keyword")));

            assertIndexMappingsNoFields(mappings, "index1");
            assertIndexMappingsNoFields(mappings, "index2");
            ImmutableOpenMap<String, MappingMetaData> index3 = mappings.get("index3");
            assertEquals(1, index3.size());
            MappingMetaData mappingMetaData = index3.get("_doc");
            Map<String, Object> sourceAsMap = mappingMetaData.getSourceAsMap();
            assertEquals(3, sourceAsMap.size());
            assertTrue(sourceAsMap.containsKey("_routing"));
            assertTrue(sourceAsMap.containsKey("_source"));
            Map<String, Object> typeProperties = (Map<String, Object>) sourceAsMap.get("properties");
            assertNotNull(typeProperties);
            assertEquals(1, typeProperties.size());
            Map<String, Object> properties = (Map<String, Object>) typeProperties.get("properties");
            assertNotNull(properties);
            assertEquals(2, properties.size());
            assertTrue(properties.containsKey("type"));
            Map<String, Object> propertiesProperties = (Map<String, Object>) properties.get("properties");
            assertNotNull(propertiesProperties);
            assertEquals(2, propertiesProperties.size());
            Map<String, Object> key = (Map<String, Object>) propertiesProperties.get("key");
            assertEquals(1, key.size());
            Map<String, Object> keyProperties = (Map<String, Object>) key.get("properties");
            assertEquals(1, keyProperties.size());
            assertLeafs(keyProperties, "keyword");
            Map<String, Object> value = (Map<String, Object>) propertiesProperties.get("value");
            assertEquals(1, value.size());
            Map<String, Object> valueProperties = (Map<String, Object>) value.get("properties");
            assertEquals(1, valueProperties.size());
            assertLeafs(valueProperties, "keyword");
        }

        {
            ImmutableOpenMap<String, ImmutableOpenMap<String, MappingMetaData>> mappings = metaData.findMappings(
                    new String[]{"index1", "index2" , "index3"},
                    new String[]{"_doc"}, index -> field -> (index.equals("index2")));

            assertIndexMappingsNoFields(mappings, "index1");
            assertIndexMappingsNoFields(mappings, "index3");
            assertIndexMappingsNotFiltered(mappings, "index2");
        }
    }

    @SuppressWarnings("unchecked")
    private static void assertIndexMappingsNoFields(ImmutableOpenMap<String, ImmutableOpenMap<String, MappingMetaData>> mappings,
                                                    String index) {
        ImmutableOpenMap<String, MappingMetaData> indexMappings = mappings.get(index);
        assertNotNull(indexMappings);
        assertEquals(1, indexMappings.size());
        MappingMetaData docMapping = indexMappings.get("_doc");
        assertNotNull(docMapping);
        Map<String, Object> sourceAsMap = docMapping.getSourceAsMap();
        assertEquals(3, sourceAsMap.size());
        assertTrue(sourceAsMap.containsKey("_routing"));
        assertTrue(sourceAsMap.containsKey("_source"));
        Map<String, Object> typeProperties = (Map<String, Object>) sourceAsMap.get("properties");
        assertEquals(0, typeProperties.size());
    }

    @SuppressWarnings("unchecked")
    private static void assertIndexMappingsNotFiltered(ImmutableOpenMap<String, ImmutableOpenMap<String, MappingMetaData>> mappings,
                                                       String index) {
        ImmutableOpenMap<String, MappingMetaData> indexMappings = mappings.get(index);
        assertNotNull(indexMappings);

        assertEquals(1, indexMappings.size());
        MappingMetaData docMapping = indexMappings.get("_doc");
        assertNotNull(docMapping);

        Map<String, Object> sourceAsMap = docMapping.getSourceAsMap();
        assertEquals(3, sourceAsMap.size());
        assertTrue(sourceAsMap.containsKey("_routing"));
        assertTrue(sourceAsMap.containsKey("_source"));

        Map<String, Object> typeProperties = (Map<String, Object>) sourceAsMap.get("properties");
        assertEquals(7, typeProperties.size());
        assertTrue(typeProperties.containsKey("birth"));
        assertTrue(typeProperties.containsKey("age"));
        assertTrue(typeProperties.containsKey("ip"));
        assertTrue(typeProperties.containsKey("suggest"));

        Map<String, Object> name = (Map<String, Object>) typeProperties.get("name");
        assertNotNull(name);
        assertEquals(1, name.size());
        Map<String, Object> nameProperties = (Map<String, Object>) name.get("properties");
        assertNotNull(nameProperties);
        assertEquals(2, nameProperties.size());
        assertLeafs(nameProperties, "first", "last");

        Map<String, Object> address = (Map<String, Object>) typeProperties.get("address");
        assertNotNull(address);
        assertEquals(2, address.size());
        assertTrue(address.containsKey("type"));
        Map<String, Object> addressProperties = (Map<String, Object>) address.get("properties");
        assertNotNull(addressProperties);
        assertEquals(3, addressProperties.size());
        assertLeafs(addressProperties, "street", "location", "area");

        Map<String, Object> properties = (Map<String, Object>) typeProperties.get("properties");
        assertNotNull(properties);
        assertEquals(2, properties.size());
        assertTrue(properties.containsKey("type"));
        Map<String, Object> propertiesProperties = (Map<String, Object>) properties.get("properties");
        assertNotNull(propertiesProperties);
        assertEquals(2, propertiesProperties.size());
        assertMultiField(propertiesProperties, "key", "keyword");
        assertMultiField(propertiesProperties, "value", "keyword");
    }

    @SuppressWarnings("unchecked")
    public static void assertLeafs(Map<String, Object> properties, String... fields) {
        for (String field : fields) {
            assertTrue(properties.containsKey(field));
            @SuppressWarnings("unchecked")
            Map<String, Object> fieldProp = (Map<String, Object>)properties.get(field);
            assertNotNull(fieldProp);
            assertFalse(fieldProp.containsKey("properties"));
            assertFalse(fieldProp.containsKey("fields"));
        }
    }

    public static void assertMultiField(Map<String, Object> properties, String field, String... subFields) {
        assertTrue(properties.containsKey(field));
        @SuppressWarnings("unchecked")
        Map<String, Object> fieldProp = (Map<String, Object>)properties.get(field);
        assertNotNull(fieldProp);
        assertTrue(fieldProp.containsKey("fields"));
        @SuppressWarnings("unchecked")
        Map<String, Object> subFieldsDef = (Map<String, Object>) fieldProp.get("fields");
        assertLeafs(subFieldsDef, subFields);
    }

    private static final String FIND_MAPPINGS_TEST_ITEM = "{\n" +
            "  \"_doc\": {\n" +
            "      \"_routing\": {\n" +
            "        \"required\":true\n" +
            "      }," +
            "      \"_source\": {\n" +
            "        \"enabled\":false\n" +
            "      }," +
            "      \"properties\": {\n" +
            "        \"name\": {\n" +
            "          \"properties\": {\n" +
            "            \"first\": {\n" +
            "              \"type\": \"keyword\"\n" +
            "            },\n" +
            "            \"last\": {\n" +
            "              \"type\": \"keyword\"\n" +
            "            }\n" +
            "          }\n" +
            "        },\n" +
            "        \"birth\": {\n" +
            "          \"type\": \"date\"\n" +
            "        },\n" +
            "        \"age\": {\n" +
            "          \"type\": \"integer\"\n" +
            "        },\n" +
            "        \"ip\": {\n" +
            "          \"type\": \"ip\"\n" +
            "        },\n" +
            "        \"suggest\" : {\n" +
            "          \"type\": \"completion\"\n" +
            "        },\n" +
            "        \"address\": {\n" +
            "          \"type\": \"object\",\n" +
            "          \"properties\": {\n" +
            "            \"street\": {\n" +
            "              \"type\": \"keyword\"\n" +
            "            },\n" +
            "            \"location\": {\n" +
            "              \"type\": \"geo_point\"\n" +
            "            },\n" +
            "            \"area\": {\n" +
            "              \"type\": \"geo_shape\",  \n" +
            "              \"tree\": \"quadtree\",\n" +
            "              \"precision\": \"1m\"\n" +
            "            }\n" +
            "          }\n" +
            "        },\n" +
            "        \"properties\": {\n" +
            "          \"type\": \"nested\",\n" +
            "          \"properties\": {\n" +
            "            \"key\" : {\n" +
            "              \"type\": \"text\",\n" +
            "              \"fields\": {\n" +
            "                \"keyword\" : {\n" +
            "                  \"type\" : \"keyword\"\n" +
            "                }\n" +
            "              }\n" +
            "            },\n" +
            "            \"value\" : {\n" +
            "              \"type\": \"text\",\n" +
            "              \"fields\": {\n" +
            "                \"keyword\" : {\n" +
            "                  \"type\" : \"keyword\"\n" +
            "                }\n" +
            "              }\n" +
            "            }\n" +
            "          }\n" +
            "        }\n" +
            "      }\n" +
            "    }\n" +
            "  }\n" +
            "}";
}
