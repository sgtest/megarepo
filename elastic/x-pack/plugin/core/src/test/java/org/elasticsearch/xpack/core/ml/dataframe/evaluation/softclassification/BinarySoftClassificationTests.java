/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.dataframe.evaluation.softclassification;

import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.test.AbstractSerializingTestCase;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.MlEvaluationNamedXContentProvider;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

import static org.hamcrest.Matchers.equalTo;

public class BinarySoftClassificationTests extends AbstractSerializingTestCase<BinarySoftClassification> {

    @Override
    protected NamedWriteableRegistry getNamedWriteableRegistry() {
        return new NamedWriteableRegistry(new MlEvaluationNamedXContentProvider().getNamedWriteables());
    }

    @Override
    protected NamedXContentRegistry xContentRegistry() {
        return new NamedXContentRegistry(new MlEvaluationNamedXContentProvider().getNamedXContentParsers());
    }

    public static BinarySoftClassification createRandom() {
        List<SoftClassificationMetric> metrics = new ArrayList<>();
        if (randomBoolean()) {
            metrics.add(AucRocTests.createRandom());
        }
        if (randomBoolean()) {
            metrics.add(PrecisionTests.createRandom());
        }
        if (randomBoolean()) {
            metrics.add(RecallTests.createRandom());
        }
        if (randomBoolean()) {
            metrics.add(ConfusionMatrixTests.createRandom());
        }
        if (metrics.isEmpty()) {
            // not a good day to play in the lottery; let's add them all
            metrics.add(AucRocTests.createRandom());
            metrics.add(PrecisionTests.createRandom());
            metrics.add(RecallTests.createRandom());
            metrics.add(ConfusionMatrixTests.createRandom());
        }
        return new BinarySoftClassification(randomAlphaOfLength(10), randomAlphaOfLength(10), metrics);
    }

    @Override
    protected BinarySoftClassification doParseInstance(XContentParser parser) throws IOException {
        return BinarySoftClassification.fromXContent(parser);
    }

    @Override
    protected BinarySoftClassification createTestInstance() {
        return createRandom();
    }

    @Override
    protected Writeable.Reader<BinarySoftClassification> instanceReader() {
        return BinarySoftClassification::new;
    }

    public void testConstructor_GivenEmptyMetrics() {
        ElasticsearchStatusException e = expectThrows(ElasticsearchStatusException.class,
            () -> new BinarySoftClassification("foo", "bar", Collections.emptyList()));
        assertThat(e.getMessage(), equalTo("[binary_soft_classification] must have one or more metrics"));
    }
}
