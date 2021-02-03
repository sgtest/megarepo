/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.apache.lucene.search.Query;
import org.elasticsearch.common.time.DateFormatter;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.plugins.MapperPlugin;

import java.io.IOException;
import java.util.Collections;
import java.util.Map;

public class TestRuntimeField extends RuntimeFieldType {

    private final String type;

    public TestRuntimeField(String name, String type) {
        super(name, Collections.emptyMap());
        this.type = type;
    }

    @Override
    protected void doXContentBody(XContentBuilder builder, boolean includeDefaults) throws IOException {
    }

    @Override
    public ValueFetcher valueFetcher(SearchExecutionContext context, String format) {
        return null;
    }

    @Override
    public String typeName() {
        return type;
    }

    @Override
    public Query termQuery(Object value, SearchExecutionContext context) {
        return null;
    }

    public static class Plugin extends org.elasticsearch.plugins.Plugin implements MapperPlugin {
        @Override
        public Map<String, Parser> getRuntimeFieldTypes() {
            return Map.of(
                "keyword", (name, node, parserContext) -> new TestRuntimeField(name, "keyword"),
                "double", (name, node, parserContext) -> new TestRuntimeField(name, "double"),
                "long", (name, node, parserContext) -> new TestRuntimeField(name, "long"),
                "boolean", (name, node, parserContext) -> new TestRuntimeField(name, "boolean"),
                "date", (name, node, parserContext) -> new TestRuntimeField(name, "date"));
        }

        @Override
        public DynamicRuntimeFieldsBuilder getDynamicRuntimeFieldsBuilder() {
            return new DynamicRuntimeFieldsBuilder() {
                @Override
                public RuntimeFieldType newDynamicStringField(String name) {
                    return new TestRuntimeField(name, "keyword");
                }

                @Override
                public RuntimeFieldType newDynamicLongField(String name) {
                    return new TestRuntimeField(name, "long");
                }

                @Override
                public RuntimeFieldType newDynamicDoubleField(String name) {
                    return new TestRuntimeField(name, "double");
                }

                @Override
                public RuntimeFieldType newDynamicBooleanField(String name) {
                    return new TestRuntimeField(name, "boolean");
                }

                @Override
                public RuntimeFieldType newDynamicDateField(String name, DateFormatter dateFormatter) {
                    return new TestRuntimeField(name, "date");
                }
            };
        }
    }
}
