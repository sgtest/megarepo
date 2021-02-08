/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.client.ml.inference.trainedmodel.ensemble;

import org.elasticsearch.client.ml.inference.MlInferenceNamedXContentProvider;
import org.elasticsearch.client.ml.inference.trainedmodel.TargetType;
import org.elasticsearch.client.ml.inference.trainedmodel.TrainedModel;
import org.elasticsearch.client.ml.inference.trainedmodel.tree.TreeTests;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.search.SearchModule;
import org.elasticsearch.test.AbstractXContentTestCase;
import org.elasticsearch.test.ESTestCase;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.function.Predicate;
import java.util.stream.Collectors;
import java.util.stream.Stream;


public class EnsembleTests extends AbstractXContentTestCase<Ensemble> {

    @Override
    protected boolean supportsUnknownFields() {
        return true;
    }

    @Override
    protected Predicate<String> getRandomFieldsExcludeFilter() {
        return field -> field.isEmpty() == false;
    }

    @Override
    protected Ensemble doParseInstance(XContentParser parser) throws IOException {
        return Ensemble.fromXContent(parser);
    }

    public static Ensemble createRandom() {
        return createRandom(randomFrom(TargetType.values()));
    }

    public static Ensemble createRandom(TargetType targetType) {
        int numberOfFeatures = randomIntBetween(1, 10);
        List<String> featureNames = Stream.generate(() -> randomAlphaOfLength(10))
            .limit(numberOfFeatures)
            .collect(Collectors.toList());
        int numberOfModels = randomIntBetween(1, 10);
        List<TrainedModel> models = Stream.generate(() -> TreeTests.buildRandomTree(featureNames, 6, targetType))
            .limit(numberOfModels)
            .collect(Collectors.toList());
        List<String> categoryLabels = null;
        if (randomBoolean() && targetType.equals(TargetType.CLASSIFICATION)) {
            categoryLabels = randomList(2, randomIntBetween(3, 10), () -> randomAlphaOfLength(10));
        }
        List<Double> weights = Stream.generate(ESTestCase::randomDouble).limit(numberOfModels).collect(Collectors.toList());
        OutputAggregator outputAggregator = targetType == TargetType.REGRESSION ?
            randomFrom(new WeightedSum(weights), new Exponent(weights)) :
            randomFrom(
                new WeightedMode(
                    categoryLabels != null ? categoryLabels.size() : randomIntBetween(2, 10),
                    weights),
                new LogisticRegression(weights));
        double[] thresholds = randomBoolean() && targetType == TargetType.CLASSIFICATION  ?
            Stream.generate(ESTestCase::randomDouble)
                .limit(categoryLabels == null ? randomIntBetween(1, 10) : categoryLabels.size())
                .mapToDouble(Double::valueOf)
                .toArray() :
            null;

        return new Ensemble(randomBoolean() ? featureNames : Collections.emptyList(),
            models,
            outputAggregator,
            targetType,
            categoryLabels,
            thresholds);
    }

    @Override
    protected Ensemble createTestInstance() {
        return createRandom();
    }

    @Override
    protected NamedXContentRegistry xContentRegistry() {
        List<NamedXContentRegistry.Entry> namedXContent = new ArrayList<>();
        namedXContent.addAll(new MlInferenceNamedXContentProvider().getNamedXContentParsers());
        namedXContent.addAll(new SearchModule(Settings.EMPTY, Collections.emptyList()).getNamedXContents());
        return new NamedXContentRegistry(namedXContent);
    }

}
