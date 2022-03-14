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

public class FillMaskConfigTests extends InferenceConfigItemTestCase<FillMaskConfig> {

    @Override
    protected boolean supportsUnknownFields() {
        return true;
    }

    @Override
    protected Predicate<String> getRandomFieldsExcludeFilter() {
        return field -> field.isEmpty() == false;
    }

    @Override
    protected FillMaskConfig doParseInstance(XContentParser parser) throws IOException {
        return FillMaskConfig.fromXContentLenient(parser);
    }

    @Override
    protected Writeable.Reader<FillMaskConfig> instanceReader() {
        return FillMaskConfig::new;
    }

    @Override
    protected FillMaskConfig createTestInstance() {
        return createRandom();
    }

    @Override
    protected FillMaskConfig mutateInstanceForVersion(FillMaskConfig instance, Version version) {
        return instance;
    }

    public static FillMaskConfig createRandom() {
        return new FillMaskConfig(
            randomBoolean() ? null : VocabularyConfigTests.createRandom(),
            randomBoolean()
                ? null
                : randomFrom(
                    BertTokenizationTests.createRandom(),
                    MPNetTokenizationTests.createRandom(),
                    RobertaTokenizationTests.createRandom()
                ),
            randomBoolean() ? null : randomInt(),
            randomBoolean() ? null : randomAlphaOfLength(5)
        );
    }
}
