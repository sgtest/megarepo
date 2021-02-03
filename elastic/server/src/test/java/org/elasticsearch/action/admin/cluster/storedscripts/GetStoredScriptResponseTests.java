package org.elasticsearch.action.admin.cluster.storedscripts;/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.script.Script;
import org.elasticsearch.script.StoredScriptSource;
import org.elasticsearch.test.AbstractSerializingTestCase;

import java.io.IOException;
import java.util.Collections;
import java.util.Map;
import java.util.function.Predicate;

public class GetStoredScriptResponseTests extends AbstractSerializingTestCase<GetStoredScriptResponse> {

    @Override
    protected GetStoredScriptResponse doParseInstance(XContentParser parser) throws IOException {
        return GetStoredScriptResponse.fromXContent(parser);
    }

    @Override
    protected GetStoredScriptResponse createTestInstance() {
        return new GetStoredScriptResponse(randomAlphaOfLengthBetween(1, 10), randomScriptSource());
    }

    @Override
    protected Writeable.Reader<GetStoredScriptResponse> instanceReader() {
        return GetStoredScriptResponse::new;
    }

    @Override
    protected Predicate<String> getRandomFieldsExcludeFilter() {
        return s -> "script.options".equals(s);
    }

    private static StoredScriptSource randomScriptSource() {
        final String lang = randomFrom("lang", "painless", "mustache");
        final String source = randomAlphaOfLengthBetween(1, 10);
        final Map<String, String> options = randomBoolean()
            ? Collections.singletonMap(Script.CONTENT_TYPE_OPTION, XContentType.JSON.mediaType())
            : Collections.emptyMap();
        return new StoredScriptSource(lang, source, options);
    }
}
