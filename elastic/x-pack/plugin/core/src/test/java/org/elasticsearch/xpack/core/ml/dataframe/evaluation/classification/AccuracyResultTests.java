/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.dataframe.evaluation.classification;

import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.test.AbstractWireSerializingTestCase;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.classification.Accuracy.ActualClass;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.classification.Accuracy.Result;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.MlEvaluationNamedXContentProvider;

import java.util.ArrayList;
import java.util.List;
import java.util.stream.Collectors;
import java.util.stream.Stream;

public class AccuracyResultTests extends AbstractWireSerializingTestCase<Accuracy.Result> {

    @Override
    protected NamedWriteableRegistry getNamedWriteableRegistry() {
        return new NamedWriteableRegistry(new MlEvaluationNamedXContentProvider().getNamedWriteables());
    }

    @Override
    protected Accuracy.Result createTestInstance() {
        int numClasses = randomIntBetween(2, 100);
        List<String> classNames = Stream.generate(() -> randomAlphaOfLength(10)).limit(numClasses).collect(Collectors.toList());
        List<ActualClass> actualClasses = new ArrayList<>(numClasses);
        for (int i = 0; i < numClasses; i++) {
            double accuracy = randomDoubleBetween(0.0, 1.0, true);
            actualClasses.add(new ActualClass(classNames.get(i), randomNonNegativeLong(), accuracy));
        }
        double overallAccuracy = randomDoubleBetween(0.0, 1.0, true);
        return new Result(actualClasses, overallAccuracy);
    }

    @Override
    protected Writeable.Reader<Accuracy.Result> instanceReader() {
        return Accuracy.Result::new;
    }
}
