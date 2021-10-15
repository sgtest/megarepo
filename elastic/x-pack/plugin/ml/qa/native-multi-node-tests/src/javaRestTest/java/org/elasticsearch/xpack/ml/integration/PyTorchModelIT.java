/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.integration;

import org.apache.http.util.EntityUtils;
import org.elasticsearch.client.Request;
import org.elasticsearch.client.Response;
import org.elasticsearch.client.ResponseException;
import org.elasticsearch.common.CheckedBiConsumer;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.xcontent.support.XContentMapValues;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.test.SecuritySettingsSourceField;
import org.elasticsearch.test.rest.ESRestTestCase;
import org.elasticsearch.xpack.core.ml.inference.allocation.AllocationStatus;
import org.elasticsearch.xpack.core.ml.integration.MlRestTestStateCleaner;
import org.elasticsearch.xpack.core.ml.utils.MapHelper;
import org.elasticsearch.xpack.core.security.authc.support.UsernamePasswordToken;
import org.elasticsearch.xpack.ml.inference.nlp.tokenizers.BertTokenizer;
import org.junit.After;
import org.junit.Before;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Base64;
import java.util.List;
import java.util.Map;
import java.util.Queue;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.stream.Collectors;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThanOrEqualTo;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.not;
import static org.hamcrest.Matchers.nullValue;

/**
 * This test uses a tiny hardcoded base64 encoded PyTorch TorchScript model.
 * The model was created with the following python script and returns a
 * Tensor of 1s. The simplicity of the model is not important as the aim
 * is to test loading a model into the PyTorch process and evaluating it.
 *
 * ## Start Python
 * import torch
 * class SuperSimple(torch.nn.Module):
 *     def forward(self, input_ids=None, token_type_ids=None, position_ids=None, inputs_embeds=None):
 *         return torch.ones((input_ids.size()[0], 2), dtype=torch.float32)
 *
 * model = SuperSimple()
 * input_ids = torch.tensor([1, 2, 3, 4, 5])
 * the_rest = torch.ones(5)
 * result = model.forward(input_ids, the_rest, the_rest, the_rest)
 * print(result)
 *
 * traced_model =  torch.jit.trace(model, (input_ids, the_rest, the_rest, the_rest))
 * torch.jit.save(traced_model, "simplemodel.pt")
 * ## End Python
 */
public class PyTorchModelIT extends ESRestTestCase {

    private static final String BASIC_AUTH_VALUE_SUPER_USER =
        UsernamePasswordToken.basicAuthHeaderValue("x_pack_rest_user", SecuritySettingsSourceField.TEST_PASSWORD_SECURE_STRING);

    static final String BASE_64_ENCODED_MODEL =
        "UEsDBAAACAgAAAAAAAAAAAAAAAAAAAAAAAAUAA4Ac2ltcGxlbW9kZWwvZGF0YS5wa2xGQgoAWlpaWlpaWlpaWoACY19fdG9yY2hfXwp" +
            "TdXBlclNpbXBsZQpxACmBfShYCAAAAHRyYWluaW5ncQGIdWJxAi5QSwcIXOpBBDQAAAA0AAAAUEsDBBQACAgIAAAAAAAAAAAAAAAAAA" +
            "AAAAAdAEEAc2ltcGxlbW9kZWwvY29kZS9fX3RvcmNoX18ucHlGQj0AWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaW" +
            "lpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWnWOMWvDMBCF9/yKI5MMrnHTQsHgjt2aJdlCEIp9SgWSTpykFvfXV1htaYds0nfv473Jqhjh" +
            "kAPywbhgUbzSnC02wwZAyqBYOUzIUUoY4XRe6SVr/Q8lVsYbf4UBLkS2kBk1aOIPxbOIaPVQtEQ8vUnZ/WlrSxTA+JCTNHMc4Ig+Ele" +
            "s+Jod+iR3N/jDDf74wxu4e/5+DmtE9mUyhdgFNq7bZ3ekehbruC6aTxS/c1rom6Z698WrEfIYxcn4JGTftLA7tzCnJeD41IJVC+U07k" +
            "umUHw3E47Vqh+xnULeFisYLx064mV8UTZibWFMmX0p23wBUEsHCE0EGH3yAAAAlwEAAFBLAwQUAAgICAAAAAAAAAAAAAAAAAAAAAAAJ" +
            "wA5AHNpbXBsZW1vZGVsL2NvZGUvX190b3JjaF9fLnB5LmRlYnVnX3BrbEZCNQBaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpa" +
            "WlpaWlpaWlpaWlpaWlpaWlpaWlpaWrWST0+DMBiHW6bOod/BGS94kKpo2Mwyox5x3pbgiXSAFtdR/nQu3IwHiZ9oX88CaeGu9tL0efq" +
            "+v8P7fmiGA1wgTgoIcECZQqe6vmYD6G4hAJOcB1E8NazTm+ELyzY4C3Q0z8MsRwF+j4JlQUPEEo5wjH0WB9hCNFqgpOCExZY5QnnEw7" +
            "ME+0v8GuaIs8wnKI7RigVrKkBzm0lh2OdjkeHllG28f066vK6SfEypF60S+vuYt4gjj2fYr/uPrSvRv356TepfJ9iWJRN0OaELQSZN3" +
            "FRPNbcP1PTSntMr0x0HzLZQjPYIEo3UaFeiISRKH0Mil+BE/dyT1m7tCBLwVO1MX4DK3bbuTlXuy8r71j5Aoho66udAoseOnrdVzx28" +
            "UFW6ROuO/lT6QKKyo79VU54emj9QSwcInsUTEDMBAAAFAwAAUEsDBAAACAgAAAAAAAAAAAAAAAAAAAAAAAAZAAYAc2ltcGxlbW9kZWw" +
            "vY29uc3RhbnRzLnBrbEZCAgBaWoACKS5QSwcIbS8JVwQAAAAEAAAAUEsDBAAACAgAAAAAAAAAAAAAAAAAAAAAAAATADsAc2ltcGxlbW" +
            "9kZWwvdmVyc2lvbkZCNwBaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaMwpQSwcI0" +
            "Z5nVQIAAAACAAAAUEsBAgAAAAAICAAAAAAAAFzqQQQ0AAAANAAAABQAAAAAAAAAAAAAAAAAAAAAAHNpbXBsZW1vZGVsL2RhdGEucGts" +
            "UEsBAgAAFAAICAgAAAAAAE0EGH3yAAAAlwEAAB0AAAAAAAAAAAAAAAAAhAAAAHNpbXBsZW1vZGVsL2NvZGUvX190b3JjaF9fLnB5UEs" +
            "BAgAAFAAICAgAAAAAAJ7FExAzAQAABQMAACcAAAAAAAAAAAAAAAAAAgIAAHNpbXBsZW1vZGVsL2NvZGUvX190b3JjaF9fLnB5LmRlYn" +
            "VnX3BrbFBLAQIAAAAACAgAAAAAAABtLwlXBAAAAAQAAAAZAAAAAAAAAAAAAAAAAMMDAABzaW1wbGVtb2RlbC9jb25zdGFudHMucGtsU" +
            "EsBAgAAAAAICAAAAAAAANGeZ1UCAAAAAgAAABMAAAAAAAAAAAAAAAAAFAQAAHNpbXBsZW1vZGVsL3ZlcnNpb25QSwYGLAAAAAAAAAAe" +
            "Ay0AAAAAAAAAAAAFAAAAAAAAAAUAAAAAAAAAagEAAAAAAACSBAAAAAAAAFBLBgcAAAAA/AUAAAAAAAABAAAAUEsFBgAAAAAFAAUAagE" +
            "AAJIEAAAAAA==";
    static final long RAW_MODEL_SIZE; // size of the model before base64 encoding
    static {
        RAW_MODEL_SIZE = Base64.getDecoder().decode(BASE_64_ENCODED_MODEL).length;
    }

    private final ExecutorService executorService = Executors.newFixedThreadPool(5);

    @Override
    protected Settings restClientSettings() {
        return Settings.builder().put(ThreadContext.PREFIX + ".Authorization", BASIC_AUTH_VALUE_SUPER_USER).build();
    }

    @Before
    public void setLogging() throws IOException {
        Request loggingSettings = new Request("PUT", "_cluster/settings");
        loggingSettings.setJsonEntity("" +
            "{" +
            "\"persistent\" : {\n" +
            "        \"logger.org.elasticsearch.xpack.ml.inference.allocation\" : \"TRACE\",\n" +
            "        \"logger.org.elasticsearch.xpack.ml.inference.deployment\" : \"TRACE\",\n" +
            "        \"logger.org.elasticsearch.xpack.ml.process.logging\" : \"TRACE\"\n" +
            "    }" +
            "}");
        client().performRequest(loggingSettings);
    }

    @After
    public void cleanup() throws Exception {
        terminate(executorService);

        Request loggingSettings = new Request("PUT", "_cluster/settings");
        loggingSettings.setJsonEntity("" +
            "{" +
            "\"persistent\" : {\n" +
            "        \"logger.org.elasticsearch.xpack.ml.inference.allocation\" :null,\n" +
            "        \"logger.org.elasticsearch.xpack.ml.inference.deployment\" : null,\n" +
            "        \"logger.org.elasticsearch.xpack.ml.process.logging\" : null\n" +
            "    }" +
            "}");
        client().performRequest(loggingSettings);

        new MlRestTestStateCleaner(logger, adminClient()).resetFeatures();
        waitForPendingTasks(adminClient());
    }

    public void testEvaluate() throws IOException, InterruptedException {
        String modelId = "test_evaluate";
        createTrainedModel(modelId);
        putModelDefinition(modelId);
        putVocabulary(List.of("these", "are", "my", "words"), modelId);
        startDeployment(modelId);
        CountDownLatch latch = new CountDownLatch(10);
        Queue<String> failures = new ConcurrentLinkedQueue<>();
        try {
            // Adding multiple inference calls to verify different calls get routed to separate nodes
            for (int i = 0; i < 10; i++) {
                executorService.execute(() -> {
                    try {
                        Response inference = infer("my words", modelId);
                        assertThat(EntityUtils.toString(inference.getEntity()), equalTo("{\"predicted_value\":[[1.0,1.0]]}"));
                    } catch (IOException ex) {
                        failures.add(ex.getMessage());
                    } finally {
                        latch.countDown();
                    }
                });
            }
        } finally {
            assertTrue("timed-out waiting for inference requests after 30s", latch.await(30, TimeUnit.SECONDS));
            stopDeployment(modelId);
        }
        if (failures.isEmpty() == false) {
            fail("Inference calls failed with [" + failures.stream().reduce((s1, s2) -> s1 + ", " + s2) + "]");
        }
    }

    public void testEvaluateWithResultFieldOverride() throws IOException {
        String modelId = "test_evaluate";
        createTrainedModel(modelId);
        putModelDefinition(modelId);
        putVocabulary(List.of("these", "are", "my", "words"), modelId);
        startDeployment(modelId);
        String resultsField = randomAlphaOfLength(10);
        Response inference = infer("my words", modelId, resultsField);
        assertThat(EntityUtils.toString(inference.getEntity()), equalTo("{\"" + resultsField + "\":[[1.0,1.0]]}"));
        stopDeployment(modelId);
    }

    public void testEvaluateWithMinimalTimeout() throws IOException {
        String modelId = "test_evaluate_timeout";
        createTrainedModel(modelId);
        putModelDefinition(modelId);
        putVocabulary(List.of("these", "are", "my", "words"), modelId);
        startDeployment(modelId);
        ResponseException ex = expectThrows(ResponseException.class, () -> infer("my words", modelId, TimeValue.ZERO));
        assertThat(ex.getResponse().getStatusLine().getStatusCode(), equalTo(429));
        stopDeployment(modelId);
    }

    public void testDeleteFailureDueToDeployment() throws IOException {
        String modelId = "test_deployed_model_delete";
        createTrainedModel(modelId);
        putModelDefinition(modelId);
        putVocabulary(List.of("these", "are", "my", "words"), modelId);
        startDeployment(modelId);
        Exception ex = expectThrows(
            Exception.class,
            () -> client().performRequest(new Request("DELETE", "_ml/trained_models/" + modelId))
        );
        assertThat(ex.getMessage(), containsString("Cannot delete model [test_deployed_model_delete] as it is currently deployed"));
        stopDeployment(modelId);
    }

    @SuppressWarnings("unchecked")
    public void testDeploymentStats() throws IOException {
        String model = "model_starting_test";
        String modelPartial = "model_partially_started";
        String modelStarted = "model_started";
        createTrainedModel(model);
        putVocabulary(List.of("once", "twice"), model);
        putModelDefinition(model);
        createTrainedModel(modelPartial);
        putVocabulary(List.of("once", "twice"), modelPartial);
        putModelDefinition(modelPartial);
        createTrainedModel(modelStarted);
        putVocabulary(List.of("once", "twice"), modelStarted);
        putModelDefinition(modelStarted);

        CheckedBiConsumer<String, AllocationStatus.State, IOException> assertAtLeast = (modelId, state) -> {
            startDeployment(modelId, state.toString());
            Response response = getDeploymentStats(modelId);
            List<Map<String, Object>> stats = (List<Map<String, Object>>)entityAsMap(response).get("deployment_stats");
            assertThat(stats, hasSize(1));
            String statusState = (String)XContentMapValues.extractValue("allocation_status.state", stats.get(0));
            assertThat(stats.toString(), statusState, is(not(nullValue())));
            assertThat(AllocationStatus.State.fromString(statusState), greaterThanOrEqualTo(state));
            stopDeployment(model);
        };

        assertAtLeast.accept(model, AllocationStatus.State.STARTING);
        assertAtLeast.accept(modelPartial, AllocationStatus.State.STARTED);
        assertAtLeast.accept(modelStarted, AllocationStatus.State.FULLY_ALLOCATED);
    }

    @AwaitsFix(bugUrl = "https://github.com/elastic/ml-cpp/pull/1961")
    @SuppressWarnings("unchecked")
    public void testLiveDeploymentStats() throws IOException {
        String modelA = "model_a";

        createTrainedModel(modelA);
        putVocabulary(List.of("once", "twice"), modelA);
        putModelDefinition(modelA);
        startDeployment(modelA, AllocationStatus.State.FULLY_ALLOCATED.toString());
        infer("once", modelA);
        infer("twice", modelA);
        Response response = getDeploymentStats(modelA);
        List<Map<String, Object>> stats = (List<Map<String, Object>>)entityAsMap(response).get("deployment_stats");
        assertThat(stats, hasSize(1));
        assertThat(stats.get(0).get("model_id"), equalTo(modelA));
        assertThat(stats.get(0).get("model_size"), equalTo("1.5kb"));
        List<Map<String, Object>> nodes = (List<Map<String, Object>>)stats.get(0).get("nodes");
        // 2 of the 3 nodes in the cluster are ML nodes
        assertThat(nodes, hasSize(2));
        int inferenceCount = sumInferenceCountOnNodes(nodes);
        assertThat(inferenceCount, equalTo(2));
    }

    @SuppressWarnings("unchecked")
    public void testGetDeploymentStats_WithWildcard() throws IOException {

        {
            // No deployments is an error when allow_no_match == false
            expectThrows(ResponseException.class, () -> getDeploymentStats("*", false));
            getDeploymentStats("*", true);
        }

        String modelFoo = "foo";
        createTrainedModel(modelFoo);
        putVocabulary(List.of("once", "twice"), modelFoo);
        putModelDefinition(modelFoo);

        String modelBar = "bar";
        createTrainedModel(modelBar);
        putVocabulary(List.of("once", "twice"), modelBar);
        putModelDefinition(modelBar);

        startDeployment(modelFoo, AllocationStatus.State.FULLY_ALLOCATED.toString());
        startDeployment(modelBar, AllocationStatus.State.FULLY_ALLOCATED.toString());
        infer("once", modelFoo);
        infer("once", modelBar);
        {
            Response response = getDeploymentStats("*");
            Map<String, Object> map = entityAsMap(response);
            List<Map<String, Object>> stats = (List<Map<String, Object>>) map.get("deployment_stats");
            assertThat(stats, hasSize(2));
            assertThat(stats.get(0).get("model_id"), equalTo(modelBar));
            assertThat(stats.get(1).get("model_id"), equalTo(modelFoo));
            List<Map<String, Object>> barNodes = (List<Map<String, Object>>)stats.get(0).get("nodes");
            // 2 of the 3 nodes in the cluster are ML nodes
            assertThat(barNodes, hasSize(2));
            assertThat(sumInferenceCountOnNodes(barNodes), equalTo(1));
            List<Map<String, Object>> fooNodes = (List<Map<String, Object>>)stats.get(0).get("nodes");
            assertThat(fooNodes, hasSize(2));
            assertThat(sumInferenceCountOnNodes(fooNodes), equalTo(1));
        }
        {
            Response response = getDeploymentStats("f*");
            Map<String, Object> map = entityAsMap(response);
            List<Map<String, Object>> stats = (List<Map<String, Object>>) map.get("deployment_stats");
            assertThat(stats, hasSize(1));
            assertThat(stats.get(0).get("model_id"), equalTo(modelFoo));
        }
        {
            Response response = getDeploymentStats("bar");
            Map<String, Object> map = entityAsMap(response);
            List<Map<String, Object>> stats = (List<Map<String, Object>>) map.get("deployment_stats");
            assertThat(stats, hasSize(1));
            assertThat(stats.get(0).get("model_id"), equalTo(modelBar));
        }
        {
            ResponseException e = expectThrows(ResponseException.class, () -> getDeploymentStats("c*", false));
            assertThat(EntityUtils.toString(e.getResponse().getEntity()),
                containsString("No known trained model with deployment with id [c*]"));
        }
        {
            ResponseException e = expectThrows(ResponseException.class, () -> getDeploymentStats("foo,c*", false));
            assertThat(EntityUtils.toString(e.getResponse().getEntity()),
                containsString("No known trained model with deployment with id [c*]"));
        }
    }

    @SuppressWarnings("unchecked")
    public void testGetDeploymentStats_WithStartedStoppedDeployments() throws IOException {
        String modelFoo = "foo";
        String modelBar = "bar";
        createTrainedModel(modelFoo);
        putVocabulary(List.of("once", "twice"), modelFoo);
        putModelDefinition(modelFoo);

        createTrainedModel(modelBar);
        putVocabulary(List.of("once", "twice"), modelBar);
        putModelDefinition(modelBar);

        startDeployment(modelFoo, AllocationStatus.State.FULLY_ALLOCATED.toString());
        startDeployment(modelBar, AllocationStatus.State.FULLY_ALLOCATED.toString());
        infer("once", modelFoo);
        infer("once", modelBar);

        Response response = getDeploymentStats("*");
        Map<String, Object> map = entityAsMap(response);
        List<Map<String, Object>> stats = (List<Map<String, Object>>) map.get("deployment_stats");
        assertThat(stats, hasSize(2));

        // check all nodes are started
        for (int i : new int[]{0, 1}) {
            List<Map<String, Object>> nodes = (List<Map<String, Object>>) stats.get(i).get("nodes");
            // 2 ml nodes
            assertThat(nodes, hasSize(2));
            for (int j : new int[]{0, 1}) {
                Object state = MapHelper.dig("routing_state.routing_state", nodes.get(j));
                assertEquals("started", state);
            }
        }

        stopDeployment(modelFoo);

        response = getDeploymentStats("*");
        map = entityAsMap(response);
        stats = (List<Map<String, Object>>) map.get("deployment_stats");

        assertThat(stats, hasSize(1));

        // check all nodes are started
        List<Map<String, Object>> nodes = (List<Map<String, Object>>) stats.get(0).get("nodes");
        // 2 ml nodes
        assertThat(nodes, hasSize(2));
        for (int j : new int[]{0, 1}) {
            Object state = MapHelper.dig("routing_state.routing_state", nodes.get(j));
            assertEquals("started", state);
        }

        stopDeployment(modelBar);

        response = getDeploymentStats("*");
        map = entityAsMap(response);
        stats = (List<Map<String, Object>>) map.get("deployment_stats");
        assertThat(stats, empty());
    }

    private int sumInferenceCountOnNodes(List<Map<String, Object>> nodes) {
        int inferenceCount = 0;
        for (var node : nodes) {
            inferenceCount += (Integer) node.get("inference_count");
        }
        return inferenceCount;
    }

    private void putModelDefinition(String modelId) throws IOException {
        Request request = new Request("PUT", "_ml/trained_models/" + modelId + "/definition/0");
        request.setJsonEntity("{  " +
            "\"total_definition_length\":" + RAW_MODEL_SIZE + "," +
            "\"definition\": \""  + BASE_64_ENCODED_MODEL + "\"," +
            "\"total_parts\": 1" +
            "}");
        client().performRequest(request);
    }

    private void putVocabulary(List<String> vocabulary, String modelId) throws IOException {
        List<String> vocabularyWithPad = new ArrayList<>();
        vocabularyWithPad.add(BertTokenizer.PAD_TOKEN);
        vocabularyWithPad.addAll(vocabulary);
        String quotedWords = vocabularyWithPad.stream().map(s -> "\"" + s + "\"").collect(Collectors.joining(","));

        Request request = new Request(
            "PUT",
            "_ml/trained_models/" + modelId + "/vocabulary"
        );
        request.setJsonEntity("{  " +
                "\"vocabulary\": [" + quotedWords + "]\n" +
            "}");
        client().performRequest(request);
    }

    private void createTrainedModel(String modelId) throws IOException {
        Request request = new Request("PUT", "/_ml/trained_models/" + modelId);
        request.setJsonEntity("{  " +
            "    \"description\": \"simple model for testing\",\n" +
            "    \"model_type\": \"pytorch\",\n" +
            "    \"inference_config\": {\n" +
            "        \"pass_through\": {\n" +
            "            \"tokenization\": {" +
            "              \"bert\": {\"with_special_tokens\": false}\n" +
            "            }\n" +
            "        }\n" +
            "    }\n" +
            "}");
        client().performRequest(request);
    }

    private Response startDeployment(String modelId) throws IOException {
        return startDeployment(modelId, AllocationStatus.State.STARTED.toString());
    }

    private Response startDeployment(String modelId, String waitForState) throws IOException {
        Request request = new Request("POST", "/_ml/trained_models/" + modelId +
            "/deployment/_start?timeout=40s&wait_for=" + waitForState + "&inference_threads=1&model_threads=1");
        return client().performRequest(request);
    }

    private void stopDeployment(String modelId) throws IOException {
        Request request = new Request("POST", "/_ml/trained_models/" + modelId + "/deployment/_stop");
        client().performRequest(request);
    }

    private Response getDeploymentStats(String modelId) throws IOException {
        return getDeploymentStats(modelId, true);
    }

    private Response getDeploymentStats(String modelId, boolean allowNoMatch) throws IOException {
        Request request = new Request("GET", "/_ml/trained_models/" + modelId + "/deployment/_stats?allow_no_match=" + allowNoMatch);
        return client().performRequest(request);
    }

    private Response infer(String input, String modelId, TimeValue timeout) throws IOException {
        Request request = new Request("POST", "/_ml/trained_models/" + modelId + "/deployment/_infer?timeout=" + timeout.toString());
        request.setJsonEntity("{  " +
            "\"docs\": [{\"input\":\"" + input + "\"}]\n" +
            "}");
        return client().performRequest(request);
    }

    private Response infer(String input, String modelId) throws IOException {
        Request request = new Request("POST", "/_ml/trained_models/" + modelId + "/deployment/_infer");
        request.setJsonEntity("{  " +
            "\"docs\": [{\"input\":\"" + input + "\"}]\n" +
            "}");
        return client().performRequest(request);
    }

    private Response infer(String input, String modelId, String resultsField) throws IOException {
        Request request = new Request("POST", "/_ml/trained_models/" + modelId + "/deployment/_infer");
        request.setJsonEntity("{  " +
            "\"docs\": [{\"input\":\"" + input + "\"}],\n" +
            "\"inference_config\": {\"pass_through\":{\"results_field\": \"" + resultsField + "\"}}\n" +
            "}");
        return client().performRequest(request);
    }

}
