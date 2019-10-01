/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.inference.trainedmodel.tree;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.test.AbstractSerializingTestCase;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.TargetType;
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

import static org.hamcrest.Matchers.equalTo;


public class TreeTests extends AbstractSerializingTestCase<Tree> {

    private boolean lenient;

    @Before
    public void chooseStrictOrLenient() {
        lenient = randomBoolean();
    }

    @Override
    protected Tree doParseInstance(XContentParser parser) throws IOException {
        return lenient ? Tree.fromXContentLenient(parser) : Tree.fromXContentStrict(parser);
    }

    @Override
    protected boolean supportsUnknownFields() {
        return lenient;
    }

    @Override
    protected Predicate<String> getRandomFieldsExcludeFilter() {
        return field -> field.startsWith("feature_names");
    }

    @Override
    protected Tree createTestInstance() {
        return createRandom();
    }

    public static Tree createRandom() {
        int numberOfFeatures = randomIntBetween(1, 10);
        List<String> featureNames = new ArrayList<>();
        for (int i = 0; i < numberOfFeatures; i++) {
            featureNames.add(randomAlphaOfLength(10));
        }
        return buildRandomTree(featureNames,  6);
    }

    public static Tree buildRandomTree(List<String> featureNames, int depth) {
        Tree.Builder builder = Tree.builder();
        int numFeatures = featureNames.size() - 1;
        builder.setFeatureNames(featureNames);

        TreeNode.Builder node = builder.addJunction(0, randomInt(numFeatures), true, randomDouble());
        List<Integer> childNodes = List.of(node.getLeftChild(), node.getRightChild());

        for (int i = 0; i < depth -1; i++) {

            List<Integer> nextNodes = new ArrayList<>();
            for (int nodeId : childNodes) {
                if (i == depth -2) {
                    builder.addLeaf(nodeId, randomDouble());
                } else {
                    TreeNode.Builder childNode =
                        builder.addJunction(nodeId, randomInt(numFeatures), true, randomDouble());
                    nextNodes.add(childNode.getLeftChild());
                    nextNodes.add(childNode.getRightChild());
                }
            }
            childNodes = nextNodes;
        }
        List<String> categoryLabels = null;
        if (randomBoolean()) {
            categoryLabels = Arrays.asList(generateRandomStringArray(randomIntBetween(1, 10), randomIntBetween(1, 10), false, false));
        }

        return builder.setTargetType(randomFrom(TargetType.values()))
            .setClassificationLabels(categoryLabels)
            .build();
    }

    @Override
    protected Writeable.Reader<Tree> instanceReader() {
        return Tree::new;
    }

    public void testInfer() {
        // Build a tree with 2 nodes and 3 leaves using 2 features
        // The leaves have unique values 0.1, 0.2, 0.3
        Tree.Builder builder = Tree.builder().setTargetType(TargetType.REGRESSION);
        TreeNode.Builder rootNode = builder.addJunction(0, 0, true, 0.5);
        builder.addLeaf(rootNode.getRightChild(), 0.3);
        TreeNode.Builder leftChildNode = builder.addJunction(rootNode.getLeftChild(), 1, true, 0.8);
        builder.addLeaf(leftChildNode.getLeftChild(), 0.1);
        builder.addLeaf(leftChildNode.getRightChild(), 0.2);

        List<String> featureNames = Arrays.asList("foo", "bar");
        Tree tree = builder.setFeatureNames(featureNames).build();

        // This feature vector should hit the right child of the root node
        List<Double> featureVector = Arrays.asList(0.6, 0.0);
        Map<String, Object> featureMap = zipObjMap(featureNames, featureVector);
        assertEquals(0.3, tree.infer(featureMap), 0.00001);

        // This should hit the left child of the left child of the root node
        // i.e. it takes the path left, left
        featureVector = Arrays.asList(0.3, 0.7);
        featureMap = zipObjMap(featureNames, featureVector);
        assertEquals(0.1, tree.infer(featureMap), 0.00001);

        // This should hit the right child of the left child of the root node
        // i.e. it takes the path left, right
        featureVector = Arrays.asList(0.3, 0.9);
        featureMap = zipObjMap(featureNames, featureVector);
        assertEquals(0.2, tree.infer(featureMap), 0.00001);
    }

    public void testTreeClassificationProbability() {
        // Build a tree with 2 nodes and 3 leaves using 2 features
        // The leaves have unique values 0.1, 0.2, 0.3
        Tree.Builder builder = Tree.builder().setTargetType(TargetType.CLASSIFICATION);
        TreeNode.Builder rootNode = builder.addJunction(0, 0, true, 0.5);
        builder.addLeaf(rootNode.getRightChild(), 1.0);
        TreeNode.Builder leftChildNode = builder.addJunction(rootNode.getLeftChild(), 1, true, 0.8);
        builder.addLeaf(leftChildNode.getLeftChild(), 1.0);
        builder.addLeaf(leftChildNode.getRightChild(), 0.0);

        List<String> featureNames = Arrays.asList("foo", "bar");
        Tree tree = builder.setFeatureNames(featureNames).build();

        // This feature vector should hit the right child of the root node
        List<Double> featureVector = Arrays.asList(0.6, 0.0);
        Map<String, Object> featureMap = zipObjMap(featureNames, featureVector);
        assertEquals(Arrays.asList(0.0, 1.0), tree.classificationProbability(featureMap));

        // This should hit the left child of the left child of the root node
        // i.e. it takes the path left, left
        featureVector = Arrays.asList(0.3, 0.7);
        featureMap = zipObjMap(featureNames, featureVector);
        assertEquals(Arrays.asList(0.0, 1.0), tree.classificationProbability(featureMap));

        // This should hit the right child of the left child of the root node
        // i.e. it takes the path left, right
        featureVector = Arrays.asList(0.3, 0.9);
        featureMap = zipObjMap(featureNames, featureVector);
        assertEquals(Arrays.asList(1.0, 0.0), tree.classificationProbability(featureMap));
    }

    public void testTreeWithNullRoot() {
        ElasticsearchStatusException ex = expectThrows(ElasticsearchStatusException.class,
            () -> Tree.builder()
                .setNodes(Collections.singletonList(null))
                .setFeatureNames(Arrays.asList("foo", "bar"))
                .build());
        assertThat(ex.getMessage(), equalTo("[tree] cannot contain null nodes"));
    }

    public void testTreeWithInvalidNode() {
        ElasticsearchStatusException ex = expectThrows(ElasticsearchStatusException.class,
            () -> Tree.builder()
                .setNodes(TreeNode.builder(0)
                .setLeftChild(1)
                .setSplitFeature(1)
                .setThreshold(randomDouble()))
                .setFeatureNames(Arrays.asList("foo", "bar"))
                .build().validate());
        assertThat(ex.getMessage(), equalTo("[tree] contains missing nodes [1]"));
    }

    public void testTreeWithNullNode() {
        ElasticsearchStatusException ex = expectThrows(ElasticsearchStatusException.class,
            () -> Tree.builder()
                .setNodes(TreeNode.builder(0)
                .setLeftChild(1)
                .setSplitFeature(1)
                .setThreshold(randomDouble()),
                null)
                .setFeatureNames(Arrays.asList("foo", "bar"))
                .build()
                .validate());
        assertThat(ex.getMessage(), equalTo("[tree] cannot contain null nodes"));
    }

    public void testTreeWithCycle() {
        ElasticsearchStatusException ex = expectThrows(ElasticsearchStatusException.class,
            () -> Tree.builder()
                .setNodes(TreeNode.builder(0)
                    .setLeftChild(1)
                    .setSplitFeature(1)
                    .setThreshold(randomDouble()),
                TreeNode.builder(0)
                    .setLeftChild(0)
                    .setSplitFeature(1)
                    .setThreshold(randomDouble()))
                .setFeatureNames(Arrays.asList("foo", "bar"))
                .build()
                .validate());
        assertThat(ex.getMessage(), equalTo("[tree] contains cycle at node 0"));
    }

    public void testTreeWithTargetTypeAndLabelsMismatch() {
        List<String> featureNames = Arrays.asList("foo", "bar");
        String msg = "[target_type] should be [classification] if [classification_labels] is provided, and vice versa";
        ElasticsearchException ex = expectThrows(ElasticsearchException.class, () -> {
            Tree.builder()
                .setRoot(TreeNode.builder(0)
                        .setLeftChild(1)
                        .setSplitFeature(1)
                        .setThreshold(randomDouble()))
                .setFeatureNames(featureNames)
                .setClassificationLabels(Arrays.asList("label1", "label2"))
                .build()
                .validate();
        });
        assertThat(ex.getMessage(), equalTo(msg));
        ex = expectThrows(ElasticsearchException.class, () -> {
            Tree.builder()
                .setRoot(TreeNode.builder(0)
                    .setLeftChild(1)
                    .setSplitFeature(1)
                    .setThreshold(randomDouble()))
                .setFeatureNames(featureNames)
                .setTargetType(TargetType.CLASSIFICATION)
                .build()
                .validate();
        });
        assertThat(ex.getMessage(), equalTo(msg));
    }

    private static Map<String, Object> zipObjMap(List<String> keys, List<Double> values) {
        return IntStream.range(0, keys.size()).boxed().collect(Collectors.toMap(keys::get, values::get));
    }
}
