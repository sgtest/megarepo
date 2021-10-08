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
import org.elasticsearch.xpack.core.ml.AbstractBWCSerializationTestCase;

import java.io.IOException;

public class VocabularyConfigTests extends AbstractBWCSerializationTestCase<VocabularyConfig> {

    @Override
    protected VocabularyConfig doParseInstance(XContentParser parser) throws IOException {
        return VocabularyConfig.fromXContentLenient(parser);
    }

    @Override
    protected Writeable.Reader<VocabularyConfig> instanceReader() {
        return VocabularyConfig::new;
    }

    @Override
    protected VocabularyConfig createTestInstance() {
        return createRandom();
    }

    @Override
    protected VocabularyConfig mutateInstanceForVersion(VocabularyConfig instance, Version version) {
        return instance;
    }

    public static VocabularyConfig createRandom() {
        return new VocabularyConfig(randomAlphaOfLength(10));
    }
}
