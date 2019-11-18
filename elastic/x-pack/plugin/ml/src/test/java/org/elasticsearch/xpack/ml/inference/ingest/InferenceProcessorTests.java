/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.inference.ingest;

import org.elasticsearch.client.Client;
import org.elasticsearch.ingest.IngestDocument;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.core.ml.action.InferModelAction;
import org.elasticsearch.xpack.core.ml.inference.results.ClassificationInferenceResults;
import org.elasticsearch.xpack.core.ml.inference.results.RegressionInferenceResults;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.ClassificationConfig;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.RegressionConfig;
import org.junit.Before;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.hamcrest.Matchers.contains;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.core.Is.is;
import static org.mockito.Mockito.mock;

public class InferenceProcessorTests extends ESTestCase {

    private Client client;

    @Before
    public void setUpVariables() {
        client = mock(Client.class);
    }

    public void testMutateDocumentWithClassification() {
        String targetField = "classification_value";
        InferenceProcessor inferenceProcessor = new InferenceProcessor(client,
            "my_processor",
            targetField,
            "classification_model",
            new ClassificationConfig(0),
            Collections.emptyMap(),
            "ml.my_processor",
            true);

        Map<String, Object> source = new HashMap<>();
        Map<String, Object> ingestMetadata = new HashMap<>();
        IngestDocument document = new IngestDocument(source, ingestMetadata);

        InferModelAction.Response response = new InferModelAction.Response(
            Collections.singletonList(new ClassificationInferenceResults(1.0, "foo", null)));
        inferenceProcessor.mutateDocument(response, document);

        assertThat(document.getFieldValue(targetField, String.class), equalTo("foo"));
        assertThat(document.getFieldValue("ml", Map.class),
            equalTo(Collections.singletonMap("my_processor", Collections.singletonMap("model_id", "classification_model"))));
    }

    @SuppressWarnings("unchecked")
    public void testMutateDocumentClassificationTopNClasses() {
        String targetField = "classification_value_probabilities";
        InferenceProcessor inferenceProcessor = new InferenceProcessor(client,
            "my_processor",
            targetField,
            "classification_model",
            new ClassificationConfig(2),
            Collections.emptyMap(),
            "ml.my_processor",
            true);

        Map<String, Object> source = new HashMap<>();
        Map<String, Object> ingestMetadata = new HashMap<>();
        IngestDocument document = new IngestDocument(source, ingestMetadata);

        List<ClassificationInferenceResults.TopClassEntry> classes = new ArrayList<>(2);
        classes.add(new ClassificationInferenceResults.TopClassEntry("foo", 0.6));
        classes.add(new ClassificationInferenceResults.TopClassEntry("bar", 0.4));

        InferModelAction.Response response = new InferModelAction.Response(
            Collections.singletonList(new ClassificationInferenceResults(1.0, "foo", classes)));
        inferenceProcessor.mutateDocument(response, document);

        assertThat((List<Map<?,?>>)document.getFieldValue(targetField, List.class),
            contains(classes.stream().map(ClassificationInferenceResults.TopClassEntry::asValueMap).toArray(Map[]::new)));
        assertThat(document.getFieldValue("ml", Map.class),
            equalTo(Collections.singletonMap("my_processor", Collections.singletonMap("model_id", "classification_model"))));
    }

    public void testMutateDocumentRegression() {
        String targetField = "regression_value";
        InferenceProcessor inferenceProcessor = new InferenceProcessor(client,
            "my_processor",
            targetField,
            "regression_model",
            new RegressionConfig(),
            Collections.emptyMap(),
            "ml.my_processor",
            true);

        Map<String, Object> source = new HashMap<>();
        Map<String, Object> ingestMetadata = new HashMap<>();
        IngestDocument document = new IngestDocument(source, ingestMetadata);

        InferModelAction.Response response = new InferModelAction.Response(
            Collections.singletonList(new RegressionInferenceResults(0.7)));
        inferenceProcessor.mutateDocument(response, document);

        assertThat(document.getFieldValue(targetField, Double.class), equalTo(0.7));
        assertThat(document.getFieldValue("ml", Map.class),
            equalTo(Collections.singletonMap("my_processor", Collections.singletonMap("model_id", "regression_model"))));
    }

    public void testMutateDocumentNoModelMetaData() {
        String targetField = "regression_value";
        InferenceProcessor inferenceProcessor = new InferenceProcessor(client,
            "my_processor",
            targetField,
            "regression_model",
            new RegressionConfig(),
            Collections.emptyMap(),
            "ml.my_processor",
            false);

        Map<String, Object> source = new HashMap<>();
        Map<String, Object> ingestMetadata = new HashMap<>();
        IngestDocument document = new IngestDocument(source, ingestMetadata);

        InferModelAction.Response response = new InferModelAction.Response(
            Collections.singletonList(new RegressionInferenceResults(0.7)));
        inferenceProcessor.mutateDocument(response, document);

        assertThat(document.getFieldValue(targetField, Double.class), equalTo(0.7));
        assertThat(document.hasField("ml"), is(false));
    }

    public void testMutateDocumentModelMetaDataExistingField() {
        String targetField = "regression_value";
        InferenceProcessor inferenceProcessor = new InferenceProcessor(client,
            "my_processor",
            targetField,
            "regression_model",
            new RegressionConfig(),
            Collections.emptyMap(),
            "ml.my_processor",
            true);

        //cannot use singleton map as attempting to mutate later
        Map<String, Object> ml = new HashMap<>(){{
            put("regression_prediction", 0.55);
        }};
        Map<String, Object> source = new HashMap<>(){{
            put("ml", ml);
        }};
        Map<String, Object> ingestMetadata = new HashMap<>();
        IngestDocument document = new IngestDocument(source, ingestMetadata);

        InferModelAction.Response response = new InferModelAction.Response(
            Collections.singletonList(new RegressionInferenceResults(0.7)));
        inferenceProcessor.mutateDocument(response, document);

        assertThat(document.getFieldValue(targetField, Double.class), equalTo(0.7));
        assertThat(document.getFieldValue("ml", Map.class),
            equalTo(new HashMap<>(){{
                put("my_processor", Collections.singletonMap("model_id", "regression_model"));
                put("regression_prediction", 0.55);
            }}));
    }

    public void testGenerateRequestWithEmptyMapping() {
        String modelId = "model";
        Integer topNClasses = randomBoolean() ? null : randomIntBetween(1, 10);

        InferenceProcessor processor = new InferenceProcessor(client,
            "my_processor",
            "my_field",
            modelId,
            new ClassificationConfig(topNClasses),
            Collections.emptyMap(),
            "ml.my_processor",
            false);

        Map<String, Object> source = new HashMap<>(){{
            put("value1", 1);
            put("value2", 4);
            put("categorical", "foo");
        }};
        Map<String, Object> ingestMetadata = new HashMap<>();
        IngestDocument document = new IngestDocument(source, ingestMetadata);

        assertThat(processor.buildRequest(document).getObjectsToInfer().get(0), equalTo(source));
    }

    public void testGenerateWithMapping() {
        String modelId = "model";
        Integer topNClasses = randomBoolean() ? null : randomIntBetween(1, 10);

        Map<String, String> fieldMapping = new HashMap<>(3) {{
            put("value1", "new_value1");
            put("value2", "new_value2");
            put("categorical", "new_categorical");
        }};

        InferenceProcessor processor = new InferenceProcessor(client,
            "my_processor",
            "my_field",
            modelId,
            new ClassificationConfig(topNClasses),
            fieldMapping,
            "ml.my_processor",
            false);

        Map<String, Object> source = new HashMap<>(3){{
            put("value1", 1);
            put("categorical", "foo");
            put("un_touched", "bar");
        }};
        Map<String, Object> ingestMetadata = new HashMap<>();
        IngestDocument document = new IngestDocument(source, ingestMetadata);

        Map<String, Object> expectedMap = new HashMap<>(2) {{
            put("new_value1", 1);
            put("new_categorical", "foo");
            put("un_touched", "bar");
        }};
        assertThat(processor.buildRequest(document).getObjectsToInfer().get(0), equalTo(expectedMap));
    }
}
