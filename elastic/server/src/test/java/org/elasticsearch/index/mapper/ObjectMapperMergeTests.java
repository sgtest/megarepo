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

import org.elasticsearch.Version;
import org.elasticsearch.common.Explicit;
import org.elasticsearch.test.ESTestCase;

import java.util.Collections;
import java.util.HashMap;
import java.util.Map;

import static java.util.Collections.emptyMap;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.notNullValue;

public class ObjectMapperMergeTests extends ESTestCase {

    private final FieldMapper barFieldMapper = createTextFieldMapper("bar");
    private final FieldMapper bazFieldMapper = createTextFieldMapper("baz");

    private final RootObjectMapper rootObjectMapper = createMapping(false, true, true, false);

    private RootObjectMapper createMapping(boolean disabledFieldEnabled, boolean fooFieldEnabled,
                                                  boolean includeBarField, boolean includeBazField) {
        Map<String, Mapper> mappers = new HashMap<>();
        mappers.put("disabled", createObjectMapper("disabled", disabledFieldEnabled, emptyMap()));
        Map<String, Mapper> fooMappers = new HashMap<>();
        if (includeBarField) {
            fooMappers.put("bar", barFieldMapper);
        }
        if (includeBazField) {
            fooMappers.put("baz", bazFieldMapper);
        }
        mappers.put("foo", createObjectMapper("foo", fooFieldEnabled,  Collections.unmodifiableMap(fooMappers)));
        return createRootObjectMapper("type1", true, Collections.unmodifiableMap(mappers));
    }

    public void testMerge() {
        // GIVEN an enriched mapping with "baz" new field
        ObjectMapper mergeWith = createMapping(false, true, true, true);

        // WHEN merging mappings
        final ObjectMapper merged = rootObjectMapper.merge(mergeWith);

        // THEN "baz" new field is added to merged mapping
        final ObjectMapper mergedFoo = (ObjectMapper) merged.getMapper("foo");
        assertThat(mergedFoo.getMapper("bar"), notNullValue());
        assertThat(mergedFoo.getMapper("baz"), notNullValue());
    }

    public void testMergeWhenDisablingField() {
        // GIVEN a mapping with "foo" field disabled
        ObjectMapper mergeWith = createMapping(false, false, false, false);

        // WHEN merging mappings
        // THEN a MapperException is thrown with an excepted message
        MapperException e = expectThrows(MapperException.class, () -> rootObjectMapper.merge(mergeWith));
        assertEquals("the [enabled] parameter can't be updated for the object mapping [foo]", e.getMessage());
    }

    public void testMergeDisabledField() {
        // GIVEN a mapping with "foo" field disabled
        Map<String, Mapper> mappers = new HashMap<>();
        //the field is disabled, and we are not trying to re-enable it, hence merge should work
        mappers.put("disabled", new ObjectMapper.Builder("disabled", Version.CURRENT).build(new ContentPath()));
        RootObjectMapper mergeWith = createRootObjectMapper("type1", true, Collections.unmodifiableMap(mappers));

        RootObjectMapper merged = (RootObjectMapper)rootObjectMapper.merge(mergeWith);
        assertFalse(((ObjectMapper)merged.getMapper("disabled")).isEnabled());
    }

    public void testMergeEnabled() {
        ObjectMapper mergeWith = createMapping(true, true, true, false);

        MapperException e = expectThrows(MapperException.class, () -> rootObjectMapper.merge(mergeWith));
        assertEquals("the [enabled] parameter can't be updated for the object mapping [disabled]", e.getMessage());

        ObjectMapper result = rootObjectMapper.merge(mergeWith, MapperService.MergeReason.INDEX_TEMPLATE);
        assertTrue(result.isEnabled());
    }

    public void testMergeEnabledForRootMapper() {
        String type = MapperService.SINGLE_MAPPING_NAME;
        ObjectMapper firstMapper = createRootObjectMapper(type, true, Collections.emptyMap());
        ObjectMapper secondMapper = createRootObjectMapper(type, false, Collections.emptyMap());

        MapperException e = expectThrows(MapperException.class, () -> firstMapper.merge(secondMapper));
        assertEquals("the [enabled] parameter can't be updated for the object mapping [" + type + "]", e.getMessage());

        ObjectMapper result = firstMapper.merge(secondMapper, MapperService.MergeReason.INDEX_TEMPLATE);
        assertFalse(result.isEnabled());
    }

    public void testMergeDisabledRootMapper() {
        String type = MapperService.SINGLE_MAPPING_NAME;
        final RootObjectMapper rootObjectMapper =
            (RootObjectMapper) new RootObjectMapper.Builder(type, Version.CURRENT).enabled(false).build(new ContentPath());
        //the root is disabled, and we are not trying to re-enable it, but we do want to be able to add runtime fields
        final RootObjectMapper mergeWith =
            new RootObjectMapper.Builder(type, Version.CURRENT).addRuntime(new TestRuntimeField("test", "long")).build(new ContentPath());

        RootObjectMapper merged = (RootObjectMapper) rootObjectMapper.merge(mergeWith);
        assertFalse(merged.isEnabled());
        assertEquals(1, merged.runtimeFieldTypes().size());
        assertEquals("test", merged.runtimeFieldTypes().iterator().next().name());
    }

    public void testMergeNested() {
        String type = MapperService.SINGLE_MAPPING_NAME;
        ObjectMapper firstMapper = createNestedMapper(type,
            ObjectMapper.Nested.newNested(new Explicit<>(true, true), new Explicit<>(true, true)));
        ObjectMapper secondMapper = createNestedMapper(type,
            ObjectMapper.Nested.newNested(new Explicit<>(false, true), new Explicit<>(false, false)));

        MapperException e = expectThrows(MapperException.class, () -> firstMapper.merge(secondMapper));
        assertThat(e.getMessage(), containsString("[include_in_parent] parameter can't be updated on a nested object mapping"));

        ObjectMapper result = firstMapper.merge(secondMapper, MapperService.MergeReason.INDEX_TEMPLATE);
        assertFalse(result.nested().isIncludeInParent());
        assertTrue(result.nested().isIncludeInRoot());
    }

    private static RootObjectMapper createRootObjectMapper(String name, boolean enabled, Map<String, Mapper> mappers) {
        final RootObjectMapper rootObjectMapper
            = (RootObjectMapper) new RootObjectMapper.Builder(name, Version.CURRENT).enabled(enabled).build(new ContentPath());

        mappers.values().forEach(rootObjectMapper::putMapper);

        return rootObjectMapper;
    }

    private static ObjectMapper createObjectMapper(String name, boolean enabled, Map<String, Mapper> mappers) {
        final ObjectMapper mapper = new ObjectMapper.Builder(name, Version.CURRENT).enabled(enabled).build(new ContentPath());

        mappers.values().forEach(mapper::putMapper);

        return mapper;
    }

    private static ObjectMapper createNestedMapper(String name, ObjectMapper.Nested nested) {
        return new ObjectMapper.Builder(name, Version.CURRENT)
            .nested(nested)
            .build(new ContentPath());
    }

    private TextFieldMapper createTextFieldMapper(String name) {
        return new TextFieldMapper.Builder(name, createDefaultIndexAnalyzers()).build(new ContentPath());
    }
}
