/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.application.search;

import org.elasticsearch.core.Tuple;
import org.elasticsearch.script.Script;
import org.elasticsearch.script.ScriptType;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.core.action.util.PageParams;

import java.util.Collections;
import java.util.Locale;
import java.util.Map;

import static org.elasticsearch.test.ESTestCase.generateRandomStringArray;
import static org.elasticsearch.test.ESTestCase.randomAlphaOfLengthBetween;
import static org.elasticsearch.test.ESTestCase.randomBoolean;
import static org.elasticsearch.test.ESTestCase.randomFrom;
import static org.elasticsearch.test.ESTestCase.randomIdentifier;
import static org.elasticsearch.test.ESTestCase.randomIntBetween;
import static org.elasticsearch.test.ESTestCase.randomLongBetween;
import static org.elasticsearch.test.ESTestCase.randomMap;

public final class SearchApplicationTestUtils {

    private SearchApplicationTestUtils() {
        throw new UnsupportedOperationException("Don't instantiate this class!");
    }

    public static PageParams randomPageParams() {
        int from = randomIntBetween(0, 10000);
        int size = randomIntBetween(0, 10000);
        PageParams pageParams = new PageParams(from, size);
        return pageParams;
    }

    public static SearchApplication randomSearchApplication() {
        return new SearchApplication(
            ESTestCase.randomAlphaOfLengthBetween(1, 10),
            generateRandomStringArray(10, 10, false, false),
            randomFrom(new String[] { null, randomAlphaOfLengthBetween(1, 10) }),
            randomLongBetween(0, Long.MAX_VALUE),
            randomBoolean() ? getRandomSearchApplicationTemplate() : null
        );
    }

    public static SearchApplicationTemplate getRandomSearchApplicationTemplate() {
        String paramName = randomAlphaOfLengthBetween(8, 10);
        String paramValue = randomAlphaOfLengthBetween(8, 10);
        String query = String.format(Locale.ROOT, """
            "query_string": {
                "query": "{{%s}}"
            }
            """, paramName);
        final Script script = new Script(ScriptType.INLINE, "mustache", query, Collections.singletonMap(paramName, paramValue));
        String paramValidationSource = String.format(Locale.ROOT, """
            {
                "%s": {
                    "type": "string"
                }
            }
            """, paramName);
        final TemplateParamValidator templateParamValidator = new TemplateParamValidator(paramValidationSource);
        return new SearchApplicationTemplate(script, templateParamValidator);
    }

    public static Map<String, Object> randomSearchApplicationQueryParams() {
        return randomMap(0, 10, () -> Tuple.tuple(randomIdentifier(), randomAlphaOfLengthBetween(0, 10)));
    }

}
