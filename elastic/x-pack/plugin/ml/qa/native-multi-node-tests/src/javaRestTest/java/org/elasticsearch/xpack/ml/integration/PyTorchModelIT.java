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
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.test.SecuritySettingsSourceField;
import org.elasticsearch.test.rest.ESRestTestCase;
import org.elasticsearch.xpack.core.security.authc.support.UsernamePasswordToken;

import java.io.IOException;
import java.util.Base64;

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
import static org.hamcrest.Matchers.equalTo;

public class PyTorchModelIT extends ESRestTestCase {

    private static final String BASIC_AUTH_VALUE_SUPER_USER =
        UsernamePasswordToken.basicAuthHeaderValue("x_pack_rest_user", SecuritySettingsSourceField.TEST_PASSWORD_SECURE_STRING);

    @Override
    protected Settings restClientSettings() {
        return Settings.builder().put(ThreadContext.PREFIX + ".Authorization", BASIC_AUTH_VALUE_SUPER_USER).build();
    }

    private static final String MODEL_INDEX = "model_store";
    private static final String MODEL_ID ="simple_model_to_evaluate";
    private static final String BASE_64_ENCODED_MODEL =
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
    private static final int RAW_MODEL_SIZE; // size of the model before base64 encoding
    static {
        RAW_MODEL_SIZE = Base64.getDecoder().decode(BASE_64_ENCODED_MODEL).length;
    }

    @AwaitsFix(bugUrl = "https://github.com/elastic/elasticsearch/issues/73769")
    public void testEvaluate() throws IOException {
        createModelStoreIndex();
        putTaskConfig();
        putModelDefinition();
        refreshModelStoreIndex();
        createTrainedModel();
        startDeployment();
        try {
            Response inference = infer("my words");
            assertThat(EntityUtils.toString(inference.getEntity()), equalTo("{\"inference\":[[1.0,1.0]]}"));
        } finally {
            stopDeployment();
        }
    }

    private void putModelDefinition() throws IOException {
        Request request = new Request("PUT", "/" + MODEL_INDEX + "/_doc/trained_model_definition_doc-" + MODEL_ID + "-0");
        request.setJsonEntity("{  " +
            "\"doc_type\": \"trained_model_definition_doc\"," +
            "\"model_id\": \"" + MODEL_ID +"\"," +
            "\"doc_num\": 0," +
            "\"definition_length\":" + RAW_MODEL_SIZE + "," +
            "\"total_definition_length\":" + RAW_MODEL_SIZE + "," +
            "\"compression_version\": 1," +
            "\"definition\": \""  + BASE_64_ENCODED_MODEL + "\"," +
            "\"eos\": true" +
            "}");
        client().performRequest(request);
    }

    private void createModelStoreIndex() throws IOException {
        Request request = new Request("PUT", "/" + MODEL_INDEX);
        request.setJsonEntity("{  " +
            "\"mappings\": {\n" +
            "    \"properties\": {\n" +
            "        \"doc_type\":    { \"type\": \"keyword\"  },\n" +
            "        \"model_id\":    { \"type\": \"keyword\"  },\n" +
            "        \"definition_length\":     { \"type\": \"long\"  },\n" +
            "        \"total_definition_length\":     { \"type\": \"long\"  },\n" +
            "        \"compression_version\":     { \"type\": \"long\"  },\n" +
            "        \"definition\":     { \"type\": \"binary\"  },\n" +
            "        \"eos\":      { \"type\": \"boolean\" },\n" +
            "        \"task_type\":      { \"type\": \"keyword\" },\n" +
            "        \"vocab\":      { \"type\": \"keyword\" },\n" +
            "        \"with_special_tokens\":      { \"type\": \"boolean\" },\n" +
            "        \"do_lower_case\":      { \"type\": \"boolean\" }\n" +
            "      }\n" +
            "    }" +
            "}");
        client().performRequest(request);
    }

    private void putTaskConfig() throws IOException {
        Request request = new Request("PUT", "/" + MODEL_INDEX + "/_doc/" + MODEL_ID + "_task_config");
        request.setJsonEntity("{  " +
                "\"task_type\": \"bert_pass_through\",\n" +
                "\"with_special_tokens\": false," +
                "\"vocab\": [\"these\", \"are\", \"my\", \"words\"]\n" +
            "}");
        client().performRequest(request);
    }

    private void createTrainedModel() throws IOException {
        Request request = new Request("PUT", "/_ml/trained_models/" + MODEL_ID);
        request.setJsonEntity("{  " +
            "    \"description\": \"simple model for testing\",\n" +
            "    \"model_type\": \"pytorch\",\n" +
            "    \"inference_config\": {\n" +
            "        \"classification\": {\n" +
            "            \"num_top_classes\": 1\n" +
            "        }\n" +
            "    },\n" +
            "    \"input\": {\n" +
            "        \"field_names\": [\"text_field\"]\n" +
            "    },\n" +
            "    \"location\": {\n" +
            "        \"index\": {\n" +
            "            \"model_id\": \"" + MODEL_ID + "\",\n" +
            "            \"name\": \"" + MODEL_INDEX + "\"\n" +
            "        }\n" +
            "    }" +
            "}");
        client().performRequest(request);
    }

    private void refreshModelStoreIndex() throws IOException {
        Request request = new Request("POST", "/" + MODEL_INDEX + "/_refresh");
        client().performRequest(request);
    }

    private void startDeployment() throws IOException {
        Request request = new Request("POST", "/_ml/trained_models/" + MODEL_ID + "/deployment/_start?timeout=40s");
        Response response = client().performRequest(request);
        logger.info("Start response: " + EntityUtils.toString(response.getEntity()));
    }

    private void stopDeployment() throws IOException {
        Request request = new Request("POST", "/_ml/trained_models/" + MODEL_ID + "/deployment/_stop");
        client().performRequest(request);
    }

    private Response infer(String input) throws IOException {
        Request request = new Request("POST", "/_ml/trained_models/" + MODEL_ID + "/deployment/_infer");
        request.setJsonEntity("{  " +
            "\"input\": \"" + input + "\"\n" +
            "}");
        return client().performRequest(request);
    }

}
