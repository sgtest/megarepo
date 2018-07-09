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

package org.elasticsearch.action.admin.indices.mapping.get;

import com.carrotsearch.hppc.cursors.ObjectCursor;
import org.elasticsearch.cluster.metadata.MappingMetaData;
import org.elasticsearch.common.collect.ImmutableOpenMap;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.test.AbstractStreamableXContentTestCase;
import org.elasticsearch.test.EqualsHashCodeTestUtils;

import java.io.IOException;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Objects;

public class GetMappingsResponseTests extends AbstractStreamableXContentTestCase<GetMappingsResponse> {

    @Override
    protected boolean supportsUnknownFields() {
        return false;
    }

    public void testCheckEqualsAndHashCode() {
        GetMappingsResponse resp = createTestInstance();
        EqualsHashCodeTestUtils.checkEqualsAndHashCode(resp, r -> new GetMappingsResponse(r.mappings()), GetMappingsResponseTests::mutate);
    }

    @Override
    protected GetMappingsResponse doParseInstance(XContentParser parser) throws IOException {
        return GetMappingsResponse.fromXContent(parser);
    }

    @Override
    protected GetMappingsResponse createBlankInstance() {
        return new GetMappingsResponse();
    }

    private static GetMappingsResponse mutate(GetMappingsResponse original) throws IOException {
        ImmutableOpenMap.Builder<String, ImmutableOpenMap<String, MappingMetaData>> builder = ImmutableOpenMap.builder(original.mappings());
        String indexKey = original.mappings().keys().iterator().next().value;

        ImmutableOpenMap.Builder<String, MappingMetaData> typeBuilder = ImmutableOpenMap.builder(original.mappings().get(indexKey));
        final String typeKey;
        Iterator<ObjectCursor<String>> iter = original.mappings().get(indexKey).keys().iterator();
        if (iter.hasNext()) {
            typeKey = iter.next().value;
        } else {
            typeKey = "new-type";
        }

        typeBuilder.put(typeKey, new MappingMetaData("type-" + randomAlphaOfLength(6), randomFieldMapping()));

        builder.put(indexKey, typeBuilder.build());
        return new GetMappingsResponse(builder.build());
    }

    @Override
    protected GetMappingsResponse mutateInstance(GetMappingsResponse instance) throws IOException {
        return mutate(instance);
    }

    public static ImmutableOpenMap<String, MappingMetaData> createMappingsForIndex() {
        // rarely have no types
        int typeCount = rarely() ? 0 : scaledRandomIntBetween(1, 3);
        List<MappingMetaData> typeMappings = new ArrayList<>(typeCount);

        for (int i = 0; i < typeCount; i++) {
            Map<String, Object> mappings = new HashMap<>();
            if (rarely() == false) { // rarely have no fields
                mappings.put("field-" + i, randomFieldMapping());
                if (randomBoolean()) {
                    mappings.put("field2-" + i, randomFieldMapping());
                }
            }

            try {
                MappingMetaData mmd = new MappingMetaData("type-" + randomAlphaOfLength(5), mappings);
                typeMappings.add(mmd);
            } catch (IOException e) {
                fail("shouldn't have failed " + e);
            }
        }
        ImmutableOpenMap.Builder<String, MappingMetaData> typeBuilder = ImmutableOpenMap.builder();
        typeMappings.forEach(mmd -> typeBuilder.put(mmd.type(), mmd));
        return typeBuilder.build();
    }

    @Override
    protected GetMappingsResponse createTestInstance() {
        ImmutableOpenMap.Builder<String, ImmutableOpenMap<String, MappingMetaData>> indexBuilder = ImmutableOpenMap.builder();
        indexBuilder.put("index-" + randomAlphaOfLength(5), createMappingsForIndex());
        GetMappingsResponse resp = new GetMappingsResponse(indexBuilder.build());
        logger.debug("--> created: {}", resp);
        return resp;
    }

    // Not meant to be exhaustive
    private static Map<String, Object> randomFieldMapping() {
        Map<String, Object> mappings = new HashMap<>();
        if (randomBoolean()) {
            Map<String, Object> regularMapping = new HashMap<>();
            regularMapping.put("type", randomBoolean() ? "text" : "keyword");
            regularMapping.put("index", "analyzed");
            regularMapping.put("analyzer", "english");
            return regularMapping;
        } else if (randomBoolean()) {
            Map<String, Object> numberMapping = new HashMap<>();
            numberMapping.put("type", randomFrom("integer", "float", "long", "double"));
            numberMapping.put("index", Objects.toString(randomBoolean()));
            return numberMapping;
        } else if (randomBoolean()) {
            Map<String, Object> objMapping = new HashMap<>();
            objMapping.put("type", "object");
            objMapping.put("dynamic", "strict");
            Map<String, Object> properties = new HashMap<>();
            Map<String, Object> props1 = new HashMap<>();
            props1.put("type", randomFrom("text", "keyword"));
            props1.put("analyzer", "keyword");
            properties.put("subtext", props1);
            Map<String, Object> props2 = new HashMap<>();
            props2.put("type", "object");
            Map<String, Object> prop2properties = new HashMap<>();
            Map<String, Object> props3 = new HashMap<>();
            props3.put("type", "integer");
            props3.put("index", "false");
            prop2properties.put("subsubfield", props3);
            props2.put("properties", prop2properties);
            objMapping.put("properties", properties);
            return objMapping;
        } else {
            Map<String, Object> plainMapping = new HashMap<>();
            plainMapping.put("type", "keyword");
            return plainMapping;
        }
    }
}
