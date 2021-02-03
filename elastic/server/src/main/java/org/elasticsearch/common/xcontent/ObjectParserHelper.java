/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.common.xcontent;

import org.elasticsearch.common.CheckedFunction;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.xcontent.ObjectParser.ValueType;
import org.elasticsearch.common.xcontent.json.JsonXContent;

import java.io.IOException;
import java.util.function.BiConsumer;

/**
 * This class provides helpers for {@link ObjectParser} that allow dealing with
 * classes outside of the xcontent dependencies.
 */
public final class ObjectParserHelper<Value, Context> {

    /**
     * Helper to declare an object that will be parsed into a {@link BytesReference}
     */
    public void declareRawObject(final AbstractObjectParser<Value, Context> parser,
                                 final BiConsumer<Value, BytesReference> consumer,
                                 final ParseField field) {
        final CheckedFunction<XContentParser, BytesReference, IOException> bytesParser = p -> {
            try (XContentBuilder builder = JsonXContent.contentBuilder()) {
                builder.copyCurrentStructure(p);
                return BytesReference.bytes(builder);
            }
        };
        parser.declareField(consumer, bytesParser, field, ValueType.OBJECT);
    }

}
