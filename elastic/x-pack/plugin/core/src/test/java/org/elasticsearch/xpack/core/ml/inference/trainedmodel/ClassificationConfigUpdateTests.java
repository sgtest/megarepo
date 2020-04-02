/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.inference.trainedmodel;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.Version;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.xpack.core.ml.AbstractBWCSerializationTestCase;

import java.io.IOException;
import java.util.Collections;
import java.util.HashMap;
import java.util.Map;

import static org.hamcrest.Matchers.equalTo;

public class ClassificationConfigUpdateTests extends AbstractBWCSerializationTestCase<ClassificationConfigUpdate> {

    public static ClassificationConfigUpdate randomClassificationConfig() {
        return new ClassificationConfigUpdate(randomBoolean() ? null : randomIntBetween(-1, 10),
            randomBoolean() ? null : randomAlphaOfLength(10),
            randomBoolean() ? null : randomAlphaOfLength(10),
            randomBoolean() ? null : randomIntBetween(0, 10)
            );
    }

    public void testFromMap() {
        ClassificationConfigUpdate expected = new ClassificationConfigUpdate(null, null, null, null);
        assertThat(ClassificationConfigUpdate.fromMap(Collections.emptyMap()), equalTo(expected));

        expected = new ClassificationConfigUpdate(3, "foo", "bar", 2);
        Map<String, Object> configMap = new HashMap<>();
        configMap.put(ClassificationConfig.NUM_TOP_CLASSES.getPreferredName(), 3);
        configMap.put(ClassificationConfig.RESULTS_FIELD.getPreferredName(), "foo");
        configMap.put(ClassificationConfig.TOP_CLASSES_RESULTS_FIELD.getPreferredName(), "bar");
        configMap.put(ClassificationConfig.NUM_TOP_FEATURE_IMPORTANCE_VALUES.getPreferredName(), 2);
        assertThat(ClassificationConfigUpdate.fromMap(configMap), equalTo(expected));
    }

    public void testFromMapWithUnknownField() {
        ElasticsearchException ex = expectThrows(ElasticsearchException.class,
            () -> ClassificationConfigUpdate.fromMap(Collections.singletonMap("some_key", 1)));
        assertThat(ex.getMessage(), equalTo("Unrecognized fields [some_key]."));
    }

    @Override
    protected ClassificationConfigUpdate createTestInstance() {
        return randomClassificationConfig();
    }

    @Override
    protected Writeable.Reader<ClassificationConfigUpdate> instanceReader() {
        return ClassificationConfigUpdate::new;
    }

    @Override
    protected ClassificationConfigUpdate doParseInstance(XContentParser parser) throws IOException {
        return ClassificationConfigUpdate.fromXContentStrict(parser);
    }

    @Override
    protected ClassificationConfigUpdate mutateInstanceForVersion(ClassificationConfigUpdate instance, Version version) {
        return instance;
    }
}
