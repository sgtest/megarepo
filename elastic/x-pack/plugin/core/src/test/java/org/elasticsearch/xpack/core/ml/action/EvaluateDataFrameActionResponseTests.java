/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.core.ml.action;

import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.test.AbstractWireSerializingTestCase;
import org.elasticsearch.xpack.core.ml.action.EvaluateDataFrameAction.Response;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.EvaluationMetricResult;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.MlEvaluationNamedXContentProvider;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.classification.AucRocResultTests;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.classification.AccuracyResultTests;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.classification.MulticlassConfusionMatrixResultTests;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.classification.PrecisionResultTests;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.classification.RecallResultTests;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.regression.Huber;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.regression.MeanSquaredError;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.regression.MeanSquaredLogarithmicError;
import org.elasticsearch.xpack.core.ml.dataframe.evaluation.regression.RSquared;

import java.util.List;

public class EvaluateDataFrameActionResponseTests extends AbstractWireSerializingTestCase<Response> {

    private static final String OUTLIER_DETECTION = "outlier_detection";
    private static final String CLASSIFICATION = "classification";
    private static final String REGRESSION = "regression";

    @Override
    protected NamedWriteableRegistry getNamedWriteableRegistry() {
        return new NamedWriteableRegistry(MlEvaluationNamedXContentProvider.getNamedWriteables());
    }

    @Override
    protected Response createTestInstance() {
        String evaluationName = randomFrom(OUTLIER_DETECTION, CLASSIFICATION, REGRESSION);
        List<EvaluationMetricResult> metrics;
        switch (evaluationName) {
            case OUTLIER_DETECTION:
                metrics = randomSubsetOf(
                    List.of(
                        AucRocResultTests.createRandom()));
                break;
            case CLASSIFICATION:
                metrics = randomSubsetOf(
                    List.of(
                        AucRocResultTests.createRandom(),
                        AccuracyResultTests.createRandom(),
                        PrecisionResultTests.createRandom(),
                        RecallResultTests.createRandom(),
                        MulticlassConfusionMatrixResultTests.createRandom()));
                    break;
            case REGRESSION:
                metrics = randomSubsetOf(
                    List.of(
                        new MeanSquaredError.Result(randomDouble()),
                        new MeanSquaredLogarithmicError.Result(randomDouble()),
                        new Huber.Result(randomDouble()),
                        new RSquared.Result(randomDouble())));
                break;
            default:
                throw new AssertionError("Please add missing \"case\" variant to the \"switch\" statement");
        }
        return new Response(evaluationName, metrics);
    }

    @Override
    protected Writeable.Reader<Response> instanceReader() {
        return Response::new;
    }
}
