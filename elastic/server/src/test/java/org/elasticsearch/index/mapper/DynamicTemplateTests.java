/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.elasticsearch.common.Strings;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.index.mapper.DynamicTemplate.XContentFieldType;
import org.elasticsearch.test.ESTestCase;

import java.io.IOException;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

public class DynamicTemplateTests extends ESTestCase {

    public void testMappingTypeTypeNotSet() {
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match_mapping_type", "string");
        templateDef.put("mapping", Collections.emptyMap());
        DynamicTemplate template = DynamicTemplate.parse("my_template", templateDef);
        //when type is not set, the provided dynamic type is returned
        assertEquals("input", template.mappingType("input"));
    }

    public void testMappingTypeTypeNotSetRuntime() {
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match_mapping_type", "string");
        templateDef.put("runtime", Collections.emptyMap());
        DynamicTemplate template = DynamicTemplate.parse("my_template", templateDef);
        //when type is not set, the provided dynamic type is returned
        assertEquals("input", template.mappingType("input"));
    }

    public void testMappingType() {
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match_mapping_type", "string");
        templateDef.put("mapping", Collections.singletonMap("type", "type_set"));
        DynamicTemplate template = DynamicTemplate.parse("my_template", templateDef);
        //when type is set, the set type is returned
        assertEquals("type_set", template.mappingType("input"));
    }

    public void testMappingTypeRuntime() {
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match_mapping_type", "string");
        templateDef.put("runtime", Collections.singletonMap("type", "type_set"));
        DynamicTemplate template = DynamicTemplate.parse("my_template", templateDef);
        //when type is set, the set type is returned
        assertEquals("type_set", template.mappingType("input"));
    }

    public void testMappingTypeDynamicTypeReplace() {
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match_mapping_type", "string");
        templateDef.put("mapping", Collections.singletonMap("type", "type_set_{dynamic_type}_{dynamicType}"));
        DynamicTemplate template = DynamicTemplate.parse("my_template", templateDef);
        //when type is set, the set type is returned
        assertEquals("type_set_input_input", template.mappingType("input"));
    }

    public void testMappingTypeDynamicTypeReplaceRuntime() {
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match_mapping_type", "string");
        templateDef.put("runtime", Collections.singletonMap("type", "type_set_{dynamic_type}_{dynamicType}"));
        DynamicTemplate template = DynamicTemplate.parse("my_template", templateDef);
        //when type is set, the set type is returned
        assertEquals("type_set_input_input", template.mappingType("input"));
    }

    public void testMappingForName() throws IOException {
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match_mapping_type", "string");
        templateDef.put("mapping", Map.of("field1_{name}", "{dynamic_type}", "test", List.of("field2_{name}_{dynamicType}")));
        DynamicTemplate template = DynamicTemplate.parse("my_template", templateDef);
        Map<String, Object> stringObjectMap = template.mappingForName("my_name", "my_type");
        assertEquals("{\"field1_my_name\":\"my_type\",\"test\":[\"field2_my_name_my_type\"]}",
            Strings.toString(JsonXContent.contentBuilder().map(stringObjectMap)));
    }

    public void testMappingForNameRuntime() throws IOException {
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match_mapping_type", "string");
        templateDef.put("runtime", Map.of("field1_{name}", "{dynamic_type}", "test", List.of("field2_{name}_{dynamicType}")));
        DynamicTemplate template = DynamicTemplate.parse("my_template", templateDef);
        Map<String, Object> stringObjectMap = template.mappingForName("my_name", "my_type");
        assertEquals("{\"field1_my_name\":\"my_type\",\"test\":[\"field2_my_name_my_type\"]}",
            Strings.toString(JsonXContent.contentBuilder().map(stringObjectMap)));
    }

    public void testParseUnknownParam() {
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match_mapping_type", "string");
        templateDef.put("mapping", Collections.singletonMap("store", true));
        templateDef.put("random_param", "random_value");

        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
                () -> DynamicTemplate.parse("my_template", templateDef));
        assertEquals("Illegal dynamic template parameter: [random_param]", e.getMessage());
    }

    public void testParseUnknownMatchType() {
        Map<String, Object> templateDef2 = new HashMap<>();
        templateDef2.put("match_mapping_type", "text");
        templateDef2.put("mapping", Collections.singletonMap("store", true));
        // if a wrong match type is specified, we ignore the template
        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
                () -> DynamicTemplate.parse("my_template", templateDef2));
        assertEquals("No field type matched on [text], possible values are [object, string, long, double, boolean, date, binary]",
                e.getMessage());
    }

    public void testParseInvalidRegex() {
        for (String param : new String[] { "path_match", "match", "path_unmatch", "unmatch" }) {
            Map<String, Object> templateDef = new HashMap<>();
            templateDef.put("match", "foo");
            templateDef.put(param, "*a");
            templateDef.put("match_pattern", "regex");
            templateDef.put("mapping", Collections.singletonMap("store", true));
            IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
                    () -> DynamicTemplate.parse("my_template", templateDef));
            assertEquals("Pattern [*a] of type [regex] is invalid. Cannot create dynamic template [my_template].", e.getMessage());
        }
    }

    public void testParseMappingAndRuntime() {
        for (String param : new String[] { "path_match", "match", "path_unmatch", "unmatch" }) {
            Map<String, Object> templateDef = new HashMap<>();
            templateDef.put("match", "foo");
            templateDef.put(param, "*a");
            templateDef.put("match_pattern", "regex");
            templateDef.put("mapping", Collections.emptyMap());
            templateDef.put("runtime", Collections.emptyMap());
            MapperParsingException e = expectThrows(MapperParsingException.class, () -> DynamicTemplate.parse("my_template", templateDef));
            assertEquals("mapping and runtime cannot be both specified in the same dynamic template [my_template]", e.getMessage());
        }
    }

    public void testParseMissingMapping() {
        for (String param : new String[] { "path_match", "match", "path_unmatch", "unmatch" }) {
            Map<String, Object> templateDef = new HashMap<>();
            templateDef.put("match", "foo");
            templateDef.put(param, "*a");
            templateDef.put("match_pattern", "regex");
            MapperParsingException e = expectThrows(MapperParsingException.class, () -> DynamicTemplate.parse("my_template", templateDef));
            assertEquals("template [my_template] must have either mapping or runtime set", e.getMessage());
        }
    }

    public void testMatchAllTemplate() {
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match_mapping_type", "*");
        templateDef.put("mapping", Collections.singletonMap("store", true));
        DynamicTemplate template = DynamicTemplate.parse("my_template", templateDef);
        assertTrue(template.match("a.b", "b", randomFrom(XContentFieldType.values())));
        assertFalse(template.isRuntimeMapping());
    }

    public void testMatchAllTemplateRuntime() {
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match_mapping_type", "*");
        templateDef.put("runtime", Collections.emptyMap());
        DynamicTemplate template = DynamicTemplate.parse("my_template", templateDef);
        assertTrue(template.isRuntimeMapping());
        assertTrue(template.match("a.b", "b", XContentFieldType.BOOLEAN));
        assertTrue(template.match("a.b", "b", XContentFieldType.DATE));
        assertTrue(template.match("a.b", "b", XContentFieldType.STRING));
        assertTrue(template.match("a.b", "b", XContentFieldType.DOUBLE));
        assertTrue(template.match("a.b", "b", XContentFieldType.LONG));
        assertFalse(template.match("a.b", "b", XContentFieldType.OBJECT));
        assertFalse(template.match("a.b", "b", XContentFieldType.BINARY));
    }

    public void testMatchAllTypesTemplateRuntime() {
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match", "b");
        templateDef.put("runtime", Collections.emptyMap());
        DynamicTemplate template = DynamicTemplate.parse("my_template", templateDef);
        assertTrue(template.isRuntimeMapping());
        assertTrue(template.match("a.b", "b", XContentFieldType.BOOLEAN));
        assertTrue(template.match("a.b", "b", XContentFieldType.DATE));
        assertTrue(template.match("a.b", "b", XContentFieldType.STRING));
        assertTrue(template.match("a.b", "b", XContentFieldType.DOUBLE));
        assertTrue(template.match("a.b", "b", XContentFieldType.LONG));
        assertFalse(template.match("a.b", "b", XContentFieldType.OBJECT));
        assertFalse(template.match("a.b", "b", XContentFieldType.BINARY));
    }

    public void testMatchTypeTemplate() {
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match_mapping_type", "string");
        templateDef.put("mapping", Collections.singletonMap("store", true));
        DynamicTemplate template = DynamicTemplate.parse("my_template", templateDef);
        assertTrue(template.match("a.b", "b", XContentFieldType.STRING));
        assertFalse(template.match("a.b", "b", XContentFieldType.BOOLEAN));
        assertFalse(template.isRuntimeMapping());
    }

    public void testMatchTypeTemplateRuntime() {
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match_mapping_type", "string");
        templateDef.put("runtime", Collections.emptyMap());
        DynamicTemplate template = DynamicTemplate.parse("my_template", templateDef);
        assertTrue(template.match("a.b", "b", XContentFieldType.STRING));
        assertFalse(template.match("a.b", "b", XContentFieldType.BOOLEAN));
        assertTrue(template.isRuntimeMapping());
    }

    public void testSupportedMatchMappingTypesRuntime() {
        //binary and object are not supported as runtime fields
        List<String> nonSupported = Arrays.asList("binary", "object");
        for (String type : nonSupported) {
            Map<String, Object> templateDef = new HashMap<>();
            templateDef.put("match_mapping_type", type);
            templateDef.put("runtime", Collections.emptyMap());
            MapperParsingException e = expectThrows(MapperParsingException.class, () -> DynamicTemplate.parse("my_template", templateDef));
            assertEquals("Dynamic template [my_template] defines a runtime field but type [" + type + "] is not supported as runtime field",
                e.getMessage());
        }
        XContentFieldType[] supported = Arrays.stream(XContentFieldType.values())
            .filter(XContentFieldType::supportsRuntimeField).toArray(XContentFieldType[]::new);
        for (XContentFieldType type : supported) {
            Map<String, Object> templateDef = new HashMap<>();
            templateDef.put("match_mapping_type", type);
            templateDef.put("runtime", Collections.emptyMap());
            assertNotNull(DynamicTemplate.parse("my_template", templateDef));
        }
    }

    public void testSerialization() throws Exception {
        // type-based template
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match_mapping_type", "string");
        templateDef.put("mapping", Collections.singletonMap("store", true));
        DynamicTemplate template = DynamicTemplate.parse("my_template", templateDef);
        XContentBuilder builder = JsonXContent.contentBuilder();
        template.toXContent(builder, ToXContent.EMPTY_PARAMS);
        assertEquals("{\"match_mapping_type\":\"string\",\"mapping\":{\"store\":true}}", Strings.toString(builder));

        // name-based template
        templateDef = new HashMap<>();
        templateDef.put("match", "*name");
        templateDef.put("unmatch", "first_name");
        templateDef.put("mapping", Collections.singletonMap("store", true));
        template = DynamicTemplate.parse("my_template", templateDef);
        builder = JsonXContent.contentBuilder();
        template.toXContent(builder, ToXContent.EMPTY_PARAMS);
        assertEquals("{\"match\":\"*name\",\"unmatch\":\"first_name\",\"mapping\":{\"store\":true}}", Strings.toString(builder));

        // path-based template
        templateDef = new HashMap<>();
        templateDef.put("path_match", "*name");
        templateDef.put("path_unmatch", "first_name");
        templateDef.put("mapping", Collections.singletonMap("store", true));
        template = DynamicTemplate.parse("my_template", templateDef);
        builder = JsonXContent.contentBuilder();
        template.toXContent(builder, ToXContent.EMPTY_PARAMS);
        assertEquals("{\"path_match\":\"*name\",\"path_unmatch\":\"first_name\",\"mapping\":{\"store\":true}}",
                Strings.toString(builder));

        // regex matching
        templateDef = new HashMap<>();
        templateDef.put("match", "^a$");
        templateDef.put("match_pattern", "regex");
        templateDef.put("mapping", Collections.singletonMap("store", true));
        template = DynamicTemplate.parse("my_template", templateDef);
        builder = JsonXContent.contentBuilder();
        template.toXContent(builder, ToXContent.EMPTY_PARAMS);
        assertEquals("{\"match\":\"^a$\",\"match_pattern\":\"regex\",\"mapping\":{\"store\":true}}", Strings.toString(builder));
    }

    public void testSerializationRuntimeMappings() throws Exception {
        // type-based template
        Map<String, Object> templateDef = new HashMap<>();
        templateDef.put("match_mapping_type", "string");
        templateDef.put("runtime", Collections.emptyMap());
        DynamicTemplate template = DynamicTemplate.parse("my_template", templateDef);
        XContentBuilder builder = JsonXContent.contentBuilder();
        template.toXContent(builder, ToXContent.EMPTY_PARAMS);
        assertEquals("{\"match_mapping_type\":\"string\",\"runtime\":{}}", Strings.toString(builder));

        // name-based template
        templateDef = new HashMap<>();
        templateDef.put("match", "*name");
        templateDef.put("unmatch", "first_name");
        templateDef.put("runtime", Collections.singletonMap("type", "new_type"));
        template = DynamicTemplate.parse("my_template", templateDef);
        builder = JsonXContent.contentBuilder();
        template.toXContent(builder, ToXContent.EMPTY_PARAMS);
        assertEquals("{\"match\":\"*name\",\"unmatch\":\"first_name\",\"runtime\":{\"type\":\"new_type\"}}", Strings.toString(builder));

        // path-based template
        templateDef = new HashMap<>();
        templateDef.put("path_match", "*name");
        templateDef.put("path_unmatch", "first_name");
        templateDef.put("runtime", Collections.emptyMap());
        template = DynamicTemplate.parse("my_template", templateDef);
        builder = JsonXContent.contentBuilder();
        template.toXContent(builder, ToXContent.EMPTY_PARAMS);
        assertEquals("{\"path_match\":\"*name\",\"path_unmatch\":\"first_name\",\"runtime\":{}}",
            Strings.toString(builder));

        // regex matching
        templateDef = new HashMap<>();
        templateDef.put("match", "^a$");
        templateDef.put("match_pattern", "regex");
        templateDef.put("runtime", Collections.emptyMap());
        template = DynamicTemplate.parse("my_template", templateDef);
        builder = JsonXContent.contentBuilder();
        template.toXContent(builder, ToXContent.EMPTY_PARAMS);
        assertEquals("{\"match\":\"^a$\",\"match_pattern\":\"regex\",\"runtime\":{}}", Strings.toString(builder));
    }
}
