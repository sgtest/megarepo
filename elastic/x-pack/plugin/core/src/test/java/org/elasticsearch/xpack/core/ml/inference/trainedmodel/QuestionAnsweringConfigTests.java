/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.core.ml.inference.trainedmodel;

import org.elasticsearch.Version;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.xcontent.XContentParser;
import org.elasticsearch.xpack.core.ml.inference.InferenceConfigItemTestCase;

import java.io.IOException;
import java.util.function.Predicate;

public class QuestionAnsweringConfigTests extends InferenceConfigItemTestCase<QuestionAnsweringConfig> {

    @Override
    protected boolean supportsUnknownFields() {
        return true;
    }

    @Override
    protected Predicate<String> getRandomFieldsExcludeFilter() {
        return field -> field.isEmpty() == false;
    }

    @Override
    protected QuestionAnsweringConfig doParseInstance(XContentParser parser) throws IOException {
        return QuestionAnsweringConfig.fromXContentLenient(parser);
    }

    @Override
    protected Writeable.Reader<QuestionAnsweringConfig> instanceReader() {
        return QuestionAnsweringConfig::new;
    }

    @Override
    protected QuestionAnsweringConfig createTestInstance() {
        return createRandom();
    }

    @Override
    protected QuestionAnsweringConfig mutateInstanceForVersion(QuestionAnsweringConfig instance, Version version) {
        return instance;
    }

    public static QuestionAnsweringConfig createRandom() {
        return new QuestionAnsweringConfig(
            randomBoolean() ? null : randomIntBetween(0, 30),
            randomBoolean() ? null : randomIntBetween(1, 50),
            randomBoolean() ? null : VocabularyConfigTests.createRandom(),
            randomBoolean()
                ? null
                : randomFrom(
                    BertTokenizationTests.createRandomWithSpan(),
                    MPNetTokenizationTests.createRandomWithSpan(),
                    RobertaTokenizationTests.createRandomWithSpan()
                ),
            randomBoolean() ? null : randomAlphaOfLength(7)
        );
    }
}
