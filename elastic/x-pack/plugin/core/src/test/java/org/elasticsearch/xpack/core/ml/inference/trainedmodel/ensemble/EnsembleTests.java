/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.inference.trainedmodel.ensemble;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.search.SearchModule;
import org.elasticsearch.test.AbstractSerializingTestCase;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.core.ml.inference.MlInferenceNamedXContentProvider;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.TargetType;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.TrainedModel;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.tree.Tree;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.tree.TreeNode;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.tree.TreeTests;
import org.junit.Before;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.function.Predicate;
import java.util.stream.Collectors;
import java.util.stream.IntStream;
import java.util.stream.Stream;

import static org.hamcrest.Matchers.closeTo;
import static org.hamcrest.Matchers.equalTo;

public class EnsembleTests extends AbstractSerializingTestCase<Ensemble> {

    private boolean lenient;

    @Before
    public void chooseStrictOrLenient() {
        lenient = randomBoolean();
    }

    @Override
    protected boolean supportsUnknownFields() {
        return lenient;
    }

    @Override
    protected Predicate<String> getRandomFieldsExcludeFilter() {
        return field -> !field.isEmpty();
    }

    @Override
    protected Ensemble doParseInstance(XContentParser parser) throws IOException {
        return lenient ? Ensemble.fromXContentLenient(parser) : Ensemble.fromXContentStrict(parser);
    }

    public static Ensemble createRandom() {
        int numberOfFeatures = randomIntBetween(1, 10);
        List<String> featureNames = Stream.generate(() -> randomAlphaOfLength(10)).limit(numberOfFeatures).collect(Collectors.toList());
        int numberOfModels = randomIntBetween(1, 10);
        List<TrainedModel> models = Stream.generate(() -> TreeTests.buildRandomTree(featureNames, 6))
            .limit(numberOfModels)
            .collect(Collectors.toList());
        List<Double> weights = randomBoolean() ?
            null :
            Stream.generate(ESTestCase::randomDouble).limit(numberOfModels).collect(Collectors.toList());
        OutputAggregator outputAggregator = randomFrom(new WeightedMode(weights), new WeightedSum(weights));
        List<String> categoryLabels = null;
        if (randomBoolean()) {
            categoryLabels = Arrays.asList(generateRandomStringArray(randomIntBetween(1, 10), randomIntBetween(1, 10), false, false));
        }

        return new Ensemble(featureNames,
            models,
            outputAggregator,
            randomFrom(TargetType.values()),
            categoryLabels);
    }

    @Override
    protected Ensemble createTestInstance() {
        return createRandom();
    }

    @Override
    protected Writeable.Reader<Ensemble> instanceReader() {
        return Ensemble::new;
    }

    @Override
    protected NamedXContentRegistry xContentRegistry() {
        List<NamedXContentRegistry.Entry> namedXContent = new ArrayList<>();
        namedXContent.addAll(new MlInferenceNamedXContentProvider().getNamedXContentParsers());
        namedXContent.addAll(new SearchModule(Settings.EMPTY, Collections.emptyList()).getNamedXContents());
        return new NamedXContentRegistry(namedXContent);
    }

    @Override
    protected NamedWriteableRegistry getNamedWriteableRegistry() {
        List<NamedWriteableRegistry.Entry> entries = new ArrayList<>();
        entries.addAll(new MlInferenceNamedXContentProvider().getNamedWriteables());
        return new NamedWriteableRegistry(entries);
    }

    public void testEnsembleWithModelsThatHaveDifferentFeatureNames() {
        List<String> featureNames = Arrays.asList("foo", "bar", "baz", "farequote");
        ElasticsearchException ex = expectThrows(ElasticsearchException.class, () -> {
            Ensemble.builder().setFeatureNames(featureNames)
                .setTrainedModels(Arrays.asList(TreeTests.buildRandomTree(Arrays.asList("bar", "foo", "baz", "farequote"), 6)))
                .build()
                .validate();
        });
        assertThat(ex.getMessage(), equalTo("[feature_names] must be the same and in the same order for each of the trained_models"));

        ex = expectThrows(ElasticsearchException.class, () -> {
            Ensemble.builder().setFeatureNames(featureNames)
                .setTrainedModels(Arrays.asList(TreeTests.buildRandomTree(Arrays.asList("completely_different"), 6)))
                .build()
                .validate();
        });
        assertThat(ex.getMessage(), equalTo("[feature_names] must be the same and in the same order for each of the trained_models"));
    }

    public void testEnsembleWithAggregatedOutputDifferingFromTrainedModels() {
        List<String> featureNames = Arrays.asList("foo", "bar");
        int numberOfModels = 5;
        List<Double> weights = new ArrayList<>(numberOfModels + 2);
        for (int i = 0; i < numberOfModels + 2; i++) {
            weights.add(randomDouble());
        }
        OutputAggregator outputAggregator = randomFrom(new WeightedMode(weights), new WeightedSum(weights));

        List<TrainedModel> models = new ArrayList<>(numberOfModels);
        for (int i = 0; i < numberOfModels; i++) {
            models.add(TreeTests.buildRandomTree(featureNames, 6));
        }
        ElasticsearchException ex = expectThrows(ElasticsearchException.class, () -> {
            Ensemble.builder()
                .setTrainedModels(models)
                .setOutputAggregator(outputAggregator)
                .setFeatureNames(featureNames)
                .build()
                .validate();
        });
        assertThat(ex.getMessage(), equalTo("[aggregate_output] expects value array of size [7] but number of models is [5]"));
    }

    public void testEnsembleWithInvalidModel() {
        List<String> featureNames = Arrays.asList("foo", "bar");
        expectThrows(ElasticsearchException.class, () -> {
            Ensemble.builder()
                .setFeatureNames(featureNames)
                .setTrainedModels(Arrays.asList(
                // Tree with loop
                Tree.builder()
                    .setNodes(TreeNode.builder(0)
                    .setLeftChild(1)
                    .setSplitFeature(1)
                    .setThreshold(randomDouble()),
                TreeNode.builder(0)
                    .setLeftChild(0)
                    .setSplitFeature(1)
                    .setThreshold(randomDouble()))
                    .setFeatureNames(featureNames)
                    .build()))
                .build()
                .validate();
        });
    }

    public void testEnsembleWithTargetTypeAndLabelsMismatch() {
        List<String> featureNames = Arrays.asList("foo", "bar");
        String msg = "[target_type] should be [classification] if [classification_labels] is provided, and vice versa";
        ElasticsearchException ex = expectThrows(ElasticsearchException.class, () -> {
            Ensemble.builder()
                .setFeatureNames(featureNames)
                .setTrainedModels(Arrays.asList(
                    Tree.builder()
                        .setNodes(TreeNode.builder(0)
                                .setLeftChild(1)
                                .setSplitFeature(1)
                                .setThreshold(randomDouble()))
                        .setFeatureNames(featureNames)
                        .build()))
                .setClassificationLabels(Arrays.asList("label1", "label2"))
                .build()
                .validate();
        });
        assertThat(ex.getMessage(), equalTo(msg));
        ex = expectThrows(ElasticsearchException.class, () -> {
            Ensemble.builder()
                .setFeatureNames(featureNames)
                .setTrainedModels(Arrays.asList(
                    Tree.builder()
                        .setNodes(TreeNode.builder(0)
                            .setLeftChild(1)
                            .setSplitFeature(1)
                            .setThreshold(randomDouble()))
                        .setFeatureNames(featureNames)
                        .build()))
                .setTargetType(TargetType.CLASSIFICATION)
                .build()
                .validate();
        });
        assertThat(ex.getMessage(), equalTo(msg));
    }

    public void testClassificationProbability() {
        List<String> featureNames = Arrays.asList("foo", "bar");
        Tree tree1 = Tree.builder()
            .setFeatureNames(featureNames)
            .setRoot(TreeNode.builder(0)
                .setLeftChild(1)
                .setRightChild(2)
                .setSplitFeature(0)
                .setThreshold(0.5))
            .addNode(TreeNode.builder(1).setLeafValue(1.0))
            .addNode(TreeNode.builder(2)
                .setThreshold(0.8)
                .setSplitFeature(1)
                .setLeftChild(3)
                .setRightChild(4))
            .addNode(TreeNode.builder(3).setLeafValue(0.0))
            .addNode(TreeNode.builder(4).setLeafValue(1.0)).build();
        Tree tree2 = Tree.builder()
            .setFeatureNames(featureNames)
            .setRoot(TreeNode.builder(0)
                .setLeftChild(1)
                .setRightChild(2)
                .setSplitFeature(0)
                .setThreshold(0.5))
            .addNode(TreeNode.builder(1).setLeafValue(0.0))
            .addNode(TreeNode.builder(2).setLeafValue(1.0))
            .build();
        Tree tree3 = Tree.builder()
            .setFeatureNames(featureNames)
            .setRoot(TreeNode.builder(0)
                .setLeftChild(1)
                .setRightChild(2)
                .setSplitFeature(1)
                .setThreshold(1.0))
            .addNode(TreeNode.builder(1).setLeafValue(1.0))
            .addNode(TreeNode.builder(2).setLeafValue(0.0))
            .build();
        Ensemble ensemble = Ensemble.builder()
            .setTargetType(TargetType.CLASSIFICATION)
            .setFeatureNames(featureNames)
            .setTrainedModels(Arrays.asList(tree1, tree2, tree3))
            .setOutputAggregator(new WeightedMode(Arrays.asList(0.7, 0.5, 1.0)))
            .build();

        List<Double> featureVector = Arrays.asList(0.4, 0.0);
        Map<String, Object> featureMap = zipObjMap(featureNames, featureVector);
        List<Double> expected = Arrays.asList(0.231475216, 0.768524783);
        double eps = 0.000001;
        List<Double> probabilities = ensemble.classificationProbability(featureMap);
        for(int i = 0; i < expected.size(); i++) {
            assertThat(probabilities.get(i), closeTo(expected.get(i), eps));
        }

        featureVector = Arrays.asList(2.0, 0.7);
        featureMap = zipObjMap(featureNames, featureVector);
        expected = Arrays.asList(0.3100255188, 0.689974481);
        probabilities = ensemble.classificationProbability(featureMap);
        for(int i = 0; i < expected.size(); i++) {
            assertThat(probabilities.get(i), closeTo(expected.get(i), eps));
        }

        featureVector = Arrays.asList(0.0, 1.0);
        featureMap = zipObjMap(featureNames, featureVector);
        expected = Arrays.asList(0.231475216, 0.768524783);
        probabilities = ensemble.classificationProbability(featureMap);
        for(int i = 0; i < expected.size(); i++) {
            assertThat(probabilities.get(i), closeTo(expected.get(i), eps));
        }
    }

    public void testClassificationInference() {
        List<String> featureNames = Arrays.asList("foo", "bar");
        Tree tree1 = Tree.builder()
            .setFeatureNames(featureNames)
            .setRoot(TreeNode.builder(0)
                .setLeftChild(1)
                .setRightChild(2)
                .setSplitFeature(0)
                .setThreshold(0.5))
            .addNode(TreeNode.builder(1).setLeafValue(1.0))
            .addNode(TreeNode.builder(2)
                .setThreshold(0.8)
                .setSplitFeature(1)
                .setLeftChild(3)
                .setRightChild(4))
            .addNode(TreeNode.builder(3).setLeafValue(0.0))
            .addNode(TreeNode.builder(4).setLeafValue(1.0)).build();
        Tree tree2 = Tree.builder()
            .setFeatureNames(featureNames)
            .setRoot(TreeNode.builder(0)
                .setLeftChild(1)
                .setRightChild(2)
                .setSplitFeature(0)
                .setThreshold(0.5))
            .addNode(TreeNode.builder(1).setLeafValue(0.0))
            .addNode(TreeNode.builder(2).setLeafValue(1.0))
            .build();
        Tree tree3 = Tree.builder()
            .setFeatureNames(featureNames)
            .setRoot(TreeNode.builder(0)
                .setLeftChild(1)
                .setRightChild(2)
                .setSplitFeature(1)
                .setThreshold(1.0))
            .addNode(TreeNode.builder(1).setLeafValue(1.0))
            .addNode(TreeNode.builder(2).setLeafValue(0.0))
            .build();
        Ensemble ensemble = Ensemble.builder()
            .setTargetType(TargetType.CLASSIFICATION)
            .setFeatureNames(featureNames)
            .setTrainedModels(Arrays.asList(tree1, tree2, tree3))
            .setOutputAggregator(new WeightedMode(Arrays.asList(0.7, 0.5, 1.0)))
            .build();

        List<Double> featureVector = Arrays.asList(0.4, 0.0);
        Map<String, Object> featureMap = zipObjMap(featureNames, featureVector);
        assertEquals(1.0, ensemble.infer(featureMap), 0.00001);

        featureVector = Arrays.asList(2.0, 0.7);
        featureMap = zipObjMap(featureNames, featureVector);
        assertEquals(1.0, ensemble.infer(featureMap), 0.00001);

        featureVector = Arrays.asList(0.0, 1.0);
        featureMap = zipObjMap(featureNames, featureVector);
        assertEquals(1.0, ensemble.infer(featureMap), 0.00001);
    }

    public void testRegressionInference() {
        List<String> featureNames = Arrays.asList("foo", "bar");
        Tree tree1 = Tree.builder()
            .setFeatureNames(featureNames)
            .setRoot(TreeNode.builder(0)
                .setLeftChild(1)
                .setRightChild(2)
                .setSplitFeature(0)
                .setThreshold(0.5))
            .addNode(TreeNode.builder(1).setLeafValue(0.3))
            .addNode(TreeNode.builder(2)
                .setThreshold(0.8)
                .setSplitFeature(1)
                .setLeftChild(3)
                .setRightChild(4))
            .addNode(TreeNode.builder(3).setLeafValue(0.1))
            .addNode(TreeNode.builder(4).setLeafValue(0.2)).build();
        Tree tree2 = Tree.builder()
            .setFeatureNames(featureNames)
            .setRoot(TreeNode.builder(0)
                .setLeftChild(1)
                .setRightChild(2)
                .setSplitFeature(0)
                .setThreshold(0.5))
            .addNode(TreeNode.builder(1).setLeafValue(1.5))
            .addNode(TreeNode.builder(2).setLeafValue(0.9))
            .build();
        Ensemble ensemble = Ensemble.builder()
            .setTargetType(TargetType.REGRESSION)
            .setFeatureNames(featureNames)
            .setTrainedModels(Arrays.asList(tree1, tree2))
            .setOutputAggregator(new WeightedSum(Arrays.asList(0.5, 0.5)))
            .build();

        List<Double> featureVector = Arrays.asList(0.4, 0.0);
        Map<String, Object> featureMap = zipObjMap(featureNames, featureVector);
        assertEquals(0.9, ensemble.infer(featureMap), 0.00001);

        featureVector = Arrays.asList(2.0, 0.7);
        featureMap = zipObjMap(featureNames, featureVector);
        assertEquals(0.5, ensemble.infer(featureMap), 0.00001);

        // Test with NO aggregator supplied, verifies default behavior of non-weighted sum
        ensemble = Ensemble.builder()
            .setTargetType(TargetType.REGRESSION)
            .setFeatureNames(featureNames)
            .setTrainedModels(Arrays.asList(tree1, tree2))
            .build();

        featureVector = Arrays.asList(0.4, 0.0);
        featureMap = zipObjMap(featureNames, featureVector);
        assertEquals(1.8, ensemble.infer(featureMap), 0.00001);

        featureVector = Arrays.asList(2.0, 0.7);
        featureMap = zipObjMap(featureNames, featureVector);
        assertEquals(1.0, ensemble.infer(featureMap), 0.00001);
    }

    private static Map<String, Object> zipObjMap(List<String> keys, List<Double> values) {
        return IntStream.range(0, keys.size()).boxed().collect(Collectors.toMap(keys::get, values::get));
    }
}
