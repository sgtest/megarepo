/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.license;

import org.elasticsearch.ElasticsearchSecurityException;
import org.elasticsearch.action.ingest.PutPipelineAction;
import org.elasticsearch.action.ingest.PutPipelineRequest;
import org.elasticsearch.action.ingest.SimulatePipelineAction;
import org.elasticsearch.action.ingest.SimulatePipelineRequest;
import org.elasticsearch.action.ingest.SimulatePipelineResponse;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.license.License.OperationMode;
import org.elasticsearch.persistent.PersistentTasksCustomMetaData;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.xpack.core.XPackField;
import org.elasticsearch.xpack.core.ml.action.CloseJobAction;
import org.elasticsearch.xpack.core.ml.action.DeleteDatafeedAction;
import org.elasticsearch.xpack.core.ml.action.DeleteJobAction;
import org.elasticsearch.xpack.core.ml.action.GetDatafeedsStatsAction;
import org.elasticsearch.xpack.core.ml.action.GetJobsStatsAction;
import org.elasticsearch.xpack.core.ml.action.InferModelAction;
import org.elasticsearch.xpack.core.ml.action.OpenJobAction;
import org.elasticsearch.xpack.core.ml.action.PutDatafeedAction;
import org.elasticsearch.xpack.core.ml.action.PutJobAction;
import org.elasticsearch.xpack.core.ml.action.StartDatafeedAction;
import org.elasticsearch.xpack.core.ml.action.StopDatafeedAction;
import org.elasticsearch.xpack.core.ml.datafeed.DatafeedState;
import org.elasticsearch.xpack.core.ml.inference.TrainedModelDefinition;
import org.elasticsearch.xpack.core.ml.inference.persistence.InferenceIndexConstants;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.RegressionConfig;
import org.elasticsearch.xpack.core.ml.job.config.JobState;
import org.elasticsearch.xpack.core.ml.job.persistence.AnomalyDetectorsIndex;
import org.elasticsearch.xpack.ml.support.BaseMlIntegTestCase;
import org.junit.Before;

import java.nio.charset.StandardCharsets;
import java.util.Collections;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasItem;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.not;

public class MachineLearningLicensingTests extends BaseMlIntegTestCase {

    @Before
    public void resetLicensing() {
        enableLicensing();

        ensureStableCluster(1);
        ensureYellow();
    }

    public void testMachineLearningPutJobActionRestricted() {
        String jobId = "testmachinelearningputjobactionrestricted";
        // Pick a license that does not allow machine learning
        License.OperationMode mode = randomInvalidLicenseType();
        enableLicensing(mode);
        assertMLAllowed(false);

        // test that license restricted apis do not work
        ElasticsearchSecurityException e = expectThrows(ElasticsearchSecurityException.class, () -> {
            PlainActionFuture<PutJobAction.Response> listener = PlainActionFuture.newFuture();
            client().execute(PutJobAction.INSTANCE, new PutJobAction.Request(createJob(jobId)), listener);
            listener.actionGet();
        });
        assertThat(e.status(), is(RestStatus.FORBIDDEN));
        assertThat(e.getMessage(), containsString("non-compliant"));
        assertThat(e.getMetadata(LicenseUtils.EXPIRED_FEATURE_METADATA), hasItem(XPackField.MACHINE_LEARNING));

        // Pick a license that does allow machine learning
        mode = randomValidLicenseType();
        enableLicensing(mode);
        assertMLAllowed(true);
        // test that license restricted apis do now work
        PlainActionFuture<PutJobAction.Response> listener = PlainActionFuture.newFuture();
        client().execute(PutJobAction.INSTANCE, new PutJobAction.Request(createJob(jobId)), listener);
        PutJobAction.Response response = listener.actionGet();
        assertNotNull(response);
    }

    public void testMachineLearningOpenJobActionRestricted() throws Exception {
        String jobId = "testmachinelearningopenjobactionrestricted";
        assertMLAllowed(true);
        // test that license restricted apis do now work
        PlainActionFuture<PutJobAction.Response> putJobListener = PlainActionFuture.newFuture();
        client().execute(PutJobAction.INSTANCE, new PutJobAction.Request(createJob(jobId)), putJobListener);
        PutJobAction.Response response = putJobListener.actionGet();
        assertNotNull(response);

        // Pick a license that does not allow machine learning
        License.OperationMode mode = randomInvalidLicenseType();
        enableLicensing(mode);
        assertMLAllowed(false);
        // test that license restricted apis do not work
        ElasticsearchSecurityException e = expectThrows(ElasticsearchSecurityException.class, () -> {
            PlainActionFuture<AcknowledgedResponse> listener = PlainActionFuture.newFuture();
            client().execute(OpenJobAction.INSTANCE, new OpenJobAction.Request(jobId), listener);
            listener.actionGet();
        });
        assertThat(e.status(), is(RestStatus.FORBIDDEN));
        assertThat(e.getMessage(), containsString("non-compliant"));
        assertThat(e.getMetadata(LicenseUtils.EXPIRED_FEATURE_METADATA), hasItem(XPackField.MACHINE_LEARNING));

        // Pick a license that does allow machine learning
        mode = randomValidLicenseType();
        enableLicensing(mode);
        assertMLAllowed(true);

        // now that the license is invalid, the job should get closed:
        assertBusy(() -> {
            JobState jobState = getJobStats(jobId).getState();
            assertEquals(JobState.CLOSED, jobState);
        });

        // test that license restricted apis do now work
        PlainActionFuture<AcknowledgedResponse> listener = PlainActionFuture.newFuture();
        client().execute(OpenJobAction.INSTANCE, new OpenJobAction.Request(jobId), listener);
        AcknowledgedResponse response2 = listener.actionGet();
        assertNotNull(response2);
    }

    public void testMachineLearningPutDatafeedActionRestricted() throws Exception {
        String jobId = "testmachinelearningputdatafeedactionrestricted";
        String datafeedId = jobId + "-datafeed";
        assertMLAllowed(true);
        // test that license restricted apis do now work
        PlainActionFuture<PutJobAction.Response> putJobListener = PlainActionFuture.newFuture();
        client().execute(PutJobAction.INSTANCE, new PutJobAction.Request(createJob(jobId)), putJobListener);
        PutJobAction.Response putJobResponse = putJobListener.actionGet();
        assertNotNull(putJobResponse);

        // Pick a license that does not allow machine learning
        License.OperationMode mode = randomInvalidLicenseType();
        enableLicensing(mode);
        assertMLAllowed(false);
        // test that license restricted apis do not work
        ElasticsearchSecurityException e = expectThrows(ElasticsearchSecurityException.class, () -> {
            PlainActionFuture<PutDatafeedAction.Response> listener = PlainActionFuture.newFuture();
            client().execute(PutDatafeedAction.INSTANCE, 
                new PutDatafeedAction.Request(createDatafeed(datafeedId, jobId, Collections.singletonList(jobId))), listener);
            listener.actionGet();
        });
        assertThat(e.status(), is(RestStatus.FORBIDDEN));
        assertThat(e.getMessage(), containsString("non-compliant"));
        assertThat(e.getMetadata(LicenseUtils.EXPIRED_FEATURE_METADATA), hasItem(XPackField.MACHINE_LEARNING));

        // Pick a license that does allow machine learning
        mode = randomValidLicenseType();
        enableLicensing(mode);
        assertMLAllowed(true);
        // test that license restricted apis do now work
        PlainActionFuture<PutDatafeedAction.Response> listener = PlainActionFuture.newFuture();
        client().execute(PutDatafeedAction.INSTANCE, 
                new PutDatafeedAction.Request(createDatafeed(datafeedId, jobId, Collections.singletonList(jobId))), listener);
        PutDatafeedAction.Response response = listener.actionGet();
        assertNotNull(response);
    }

    public void testAutoCloseJobWithDatafeed() throws Exception {
        String jobId = "testautoclosejobwithdatafeed";
        String datafeedId = jobId + "-datafeed";
        assertMLAllowed(true);
        String datafeedIndex = jobId + "-data";
        prepareCreate(datafeedIndex).addMapping("type", "{\"type\":{\"properties\":{\"time\":{\"type\":\"date\"}}}}",
            XContentType.JSON).get();

        // put job
        PlainActionFuture<PutJobAction.Response> putJobListener = PlainActionFuture.newFuture();
        client().execute(PutJobAction.INSTANCE, new PutJobAction.Request(createJob(jobId)), putJobListener);
        PutJobAction.Response putJobResponse = putJobListener.actionGet();
        assertNotNull(putJobResponse);
        // put datafeed
        PlainActionFuture<PutDatafeedAction.Response> putDatafeedListener = PlainActionFuture.newFuture();
        client().execute(PutDatafeedAction.INSTANCE, 
                new PutDatafeedAction.Request(createDatafeed(datafeedId, jobId,
                        Collections.singletonList(datafeedIndex))), putDatafeedListener);
        PutDatafeedAction.Response putDatafeedResponse = putDatafeedListener.actionGet();
        assertNotNull(putDatafeedResponse);
        // open job
        PlainActionFuture<AcknowledgedResponse> openJobListener = PlainActionFuture.newFuture();
        client().execute(OpenJobAction.INSTANCE, new OpenJobAction.Request(jobId), openJobListener);
        AcknowledgedResponse openJobResponse = openJobListener.actionGet();
        assertNotNull(openJobResponse);
        // start datafeed
        PlainActionFuture<AcknowledgedResponse> listener = PlainActionFuture.newFuture();
        client().execute(StartDatafeedAction.INSTANCE, new StartDatafeedAction.Request(datafeedId, 0L), listener);
        listener.actionGet();


        if (randomBoolean()) {
            enableLicensing(randomInvalidLicenseType());
        } else {
            disableLicensing();
        }
        assertMLAllowed(false);

        client().admin().indices().prepareRefresh(AnomalyDetectorsIndex.configIndexName()).get();

        // now that the license is invalid, the job should be closed and datafeed stopped:
        assertBusy(() -> {
            JobState jobState = getJobStats(jobId).getState();
            assertEquals(JobState.CLOSED, jobState);

            DatafeedState datafeedState = getDatafeedStats(datafeedId).getDatafeedState();
            assertEquals(DatafeedState.STOPPED, datafeedState);

            ClusterState state = client().admin().cluster().prepareState().get().getState();
            PersistentTasksCustomMetaData tasks = state.metaData().custom(PersistentTasksCustomMetaData.TYPE);
            assertEquals(0, tasks.taskMap().size());
        });

        enableLicensing(randomValidLicenseType());
        assertMLAllowed(true);

        // open job
        PlainActionFuture<AcknowledgedResponse> openJobListener2 = PlainActionFuture.newFuture();
        client().execute(OpenJobAction.INSTANCE, new OpenJobAction.Request(jobId), openJobListener2);
        AcknowledgedResponse openJobResponse3 = openJobListener2.actionGet();
        assertNotNull(openJobResponse3);
        // start datafeed
        PlainActionFuture<AcknowledgedResponse> listener2 = PlainActionFuture.newFuture();
        client().execute(StartDatafeedAction.INSTANCE, new StartDatafeedAction.Request(datafeedId, 0L), listener2);
        listener2.actionGet();

        assertBusy(() -> {
            JobState jobState = getJobStats(jobId).getState();
            assertEquals(JobState.OPENED, jobState);

            DatafeedState datafeedState = getDatafeedStats(datafeedId).getDatafeedState();
            assertEquals(DatafeedState.STARTED, datafeedState);

            ClusterState state = client().admin().cluster().prepareState().get().getState();
            PersistentTasksCustomMetaData tasks = state.metaData().custom(PersistentTasksCustomMetaData.TYPE);
            assertEquals(2, tasks.taskMap().size());
        });

        if (randomBoolean()) {
            enableLicensing(randomInvalidLicenseType());
        } else {
            disableLicensing();
        }
        assertMLAllowed(false);

        // now that the license is invalid, the job should be closed and datafeed stopped:
        assertBusy(() -> {
            JobState jobState = getJobStats(jobId).getState();
            assertEquals(JobState.CLOSED, jobState);

            DatafeedState datafeedState = getDatafeedStats(datafeedId).getDatafeedState();
            assertEquals(DatafeedState.STOPPED, datafeedState);

            ClusterState state = client().admin().cluster().prepareState().get().getState();
            PersistentTasksCustomMetaData tasks = state.metaData().custom(PersistentTasksCustomMetaData.TYPE);
            assertEquals(0, tasks.taskMap().size());
        });
    }

    public void testMachineLearningStartDatafeedActionRestricted() throws Exception {
        String jobId = "testmachinelearningstartdatafeedactionrestricted";
        String datafeedId = jobId + "-datafeed";
        assertMLAllowed(true);
        String datafeedIndex = jobId + "-data";
        prepareCreate(datafeedIndex).addMapping("type", "{\"type\":{\"properties\":{\"time\":{\"type\":\"date\"}}}}",
            XContentType.JSON).get();
        // test that license restricted apis do now work
        PlainActionFuture<PutJobAction.Response> putJobListener = PlainActionFuture.newFuture();
        client().execute(PutJobAction.INSTANCE, new PutJobAction.Request(createJob(jobId)), putJobListener);
        PutJobAction.Response putJobResponse = putJobListener.actionGet();
        assertNotNull(putJobResponse);
        PlainActionFuture<PutDatafeedAction.Response> putDatafeedListener = PlainActionFuture.newFuture();
        client().execute(PutDatafeedAction.INSTANCE, 
                new PutDatafeedAction.Request(createDatafeed(datafeedId, jobId,
                        Collections.singletonList(datafeedIndex))), putDatafeedListener);
        PutDatafeedAction.Response putDatafeedResponse = putDatafeedListener.actionGet();
        assertNotNull(putDatafeedResponse);
        PlainActionFuture<AcknowledgedResponse> openJobListener = PlainActionFuture.newFuture();
        client().execute(OpenJobAction.INSTANCE, new OpenJobAction.Request(jobId), openJobListener);
        AcknowledgedResponse openJobResponse = openJobListener.actionGet();
        assertNotNull(openJobResponse);

        // Pick a license that does not allow machine learning
        License.OperationMode mode = randomInvalidLicenseType();
        enableLicensing(mode);
        assertMLAllowed(false);

        // now that the license is invalid, the job should get closed:
        assertBusy(() -> {
            JobState jobState = getJobStats(jobId).getState();
            assertEquals(JobState.CLOSED, jobState);
            ClusterState state = client().admin().cluster().prepareState().get().getState();
            PersistentTasksCustomMetaData tasks = state.metaData().custom(PersistentTasksCustomMetaData.TYPE);
            assertEquals(0, tasks.taskMap().size());
        });

        // test that license restricted apis do not work
        ElasticsearchSecurityException e = expectThrows(ElasticsearchSecurityException.class, () -> {
            PlainActionFuture<AcknowledgedResponse> listener = PlainActionFuture.newFuture();
            client().execute(StartDatafeedAction.INSTANCE, new StartDatafeedAction.Request(datafeedId, 0L), listener);
            listener.actionGet();
        });
        assertThat(e.status(), is(RestStatus.FORBIDDEN));
        assertThat(e.getMessage(), containsString("non-compliant"));
        assertThat(e.getMetadata(LicenseUtils.EXPIRED_FEATURE_METADATA), hasItem(XPackField.MACHINE_LEARNING));

        // Pick a license that does allow machine learning
        mode = randomValidLicenseType();
        enableLicensing(mode);
        assertMLAllowed(true);
        // test that license restricted apis do now work
        // re-open job now that the license is valid again
        PlainActionFuture<AcknowledgedResponse> openJobListener2 = PlainActionFuture.newFuture();
        client().execute(OpenJobAction.INSTANCE, new OpenJobAction.Request(jobId), openJobListener2);
        AcknowledgedResponse openJobResponse3 = openJobListener2.actionGet();
        assertNotNull(openJobResponse3);

        PlainActionFuture<AcknowledgedResponse> listener = PlainActionFuture.newFuture();
        client().execute(StartDatafeedAction.INSTANCE, new StartDatafeedAction.Request(datafeedId, 0L), listener);
        AcknowledgedResponse response = listener.actionGet();
        assertNotNull(response);
    }

    public void testMachineLearningStopDatafeedActionNotRestricted() throws Exception {
        String jobId = "testmachinelearningstopdatafeedactionnotrestricted";
        String datafeedId = jobId + "-datafeed";
        assertMLAllowed(true);
        String datafeedIndex = jobId + "-data";
        prepareCreate(datafeedIndex).addMapping("type", "{\"type\":{\"properties\":{\"time\":{\"type\":\"date\"}}}}",
            XContentType.JSON).get();
        // test that license restricted apis do now work
        PlainActionFuture<PutJobAction.Response> putJobListener = PlainActionFuture.newFuture();
        client().execute(PutJobAction.INSTANCE, new PutJobAction.Request(createJob(jobId)), putJobListener);
        PutJobAction.Response putJobResponse = putJobListener.actionGet();
        assertNotNull(putJobResponse);
        PlainActionFuture<PutDatafeedAction.Response> putDatafeedListener = PlainActionFuture.newFuture();
        client().execute(PutDatafeedAction.INSTANCE, 
                new PutDatafeedAction.Request(createDatafeed(datafeedId, jobId,
                        Collections.singletonList(datafeedIndex))), putDatafeedListener);
        PutDatafeedAction.Response putDatafeedResponse = putDatafeedListener.actionGet();
        assertNotNull(putDatafeedResponse);
        PlainActionFuture<AcknowledgedResponse> openJobListener = PlainActionFuture.newFuture();
        client().execute(OpenJobAction.INSTANCE, new OpenJobAction.Request(jobId), openJobListener);
        AcknowledgedResponse openJobResponse = openJobListener.actionGet();
        assertNotNull(openJobResponse);
        PlainActionFuture<AcknowledgedResponse> startDatafeedListener = PlainActionFuture.newFuture();
        client().execute(StartDatafeedAction.INSTANCE, 
            new StartDatafeedAction.Request(datafeedId, 0L), startDatafeedListener);
        AcknowledgedResponse startDatafeedResponse = startDatafeedListener.actionGet();
        assertNotNull(startDatafeedResponse);

        boolean invalidLicense = randomBoolean();
        if (invalidLicense) {
            enableLicensing(randomInvalidLicenseType());
        } else {
            enableLicensing(randomValidLicenseType());
        }

        PlainActionFuture<StopDatafeedAction.Response> listener = PlainActionFuture.newFuture();
        client().execute(StopDatafeedAction.INSTANCE, new StopDatafeedAction.Request(datafeedId), listener);
        if (invalidLicense) {
            // the stop datafeed due to invalid license happens async, so check if the datafeed turns into stopped state:
            assertBusy(() -> {
                GetDatafeedsStatsAction.Response response =
                        client().execute(GetDatafeedsStatsAction.INSTANCE, new GetDatafeedsStatsAction.Request(datafeedId)).actionGet();
                assertEquals(DatafeedState.STOPPED, response.getResponse().results().get(0).getDatafeedState());
            });
        } else {
            listener.actionGet();
        }

        if (invalidLicense) {
            // the close due to invalid license happens async, so check if the job turns into closed state:
            assertBusy(() -> {
                GetJobsStatsAction.Response response =
                        client().execute(GetJobsStatsAction.INSTANCE, new GetJobsStatsAction.Request(jobId)).actionGet();
                assertEquals(JobState.CLOSED, response.getResponse().results().get(0).getState());
            });
        }
    }

    public void testMachineLearningCloseJobActionNotRestricted() throws Exception {
        String jobId = "testmachinelearningclosejobactionnotrestricted";
        assertMLAllowed(true);
        // test that license restricted apis do now work
        PlainActionFuture<PutJobAction.Response> putJobListener = PlainActionFuture.newFuture();
        client().execute(PutJobAction.INSTANCE, new PutJobAction.Request(createJob(jobId)), putJobListener);
        PutJobAction.Response putJobResponse = putJobListener.actionGet();
        assertNotNull(putJobResponse);
        PlainActionFuture<AcknowledgedResponse> openJobListener = PlainActionFuture.newFuture();
        client().execute(OpenJobAction.INSTANCE, new OpenJobAction.Request(jobId), openJobListener);
        AcknowledgedResponse openJobResponse = openJobListener.actionGet();
        assertNotNull(openJobResponse);

        boolean invalidLicense = randomBoolean();
        if (invalidLicense) {
            enableLicensing(randomInvalidLicenseType());
        } else {
            enableLicensing(randomValidLicenseType());
        }

        PlainActionFuture<CloseJobAction.Response> listener = PlainActionFuture.newFuture();
        CloseJobAction.Request request = new CloseJobAction.Request(jobId);
        request.setCloseTimeout(TimeValue.timeValueSeconds(20));
        if (invalidLicense) {
            // the close due to invalid license happens async, so check if the job turns into closed state:
            assertBusy(() -> {
                GetJobsStatsAction.Response response =
                        client().execute(GetJobsStatsAction.INSTANCE, new GetJobsStatsAction.Request(jobId)).actionGet();
                assertEquals(JobState.CLOSED, response.getResponse().results().get(0).getState());
            });
        } else {
            client().execute(CloseJobAction.INSTANCE, request, listener);
            listener.actionGet();
        }
    }

    public void testMachineLearningDeleteJobActionNotRestricted() throws Exception {
        String jobId = "testmachinelearningclosejobactionnotrestricted";
        assertMLAllowed(true);
        // test that license restricted apis do now work
        PlainActionFuture<PutJobAction.Response> putJobListener = PlainActionFuture.newFuture();
        client().execute(PutJobAction.INSTANCE, new PutJobAction.Request(createJob(jobId)), putJobListener);
        PutJobAction.Response putJobResponse = putJobListener.actionGet();
        assertNotNull(putJobResponse);

        // Pick a random license
        License.OperationMode mode = randomLicenseType();
        enableLicensing(mode);

        PlainActionFuture<AcknowledgedResponse> listener = PlainActionFuture.newFuture();
        client().execute(DeleteJobAction.INSTANCE, new DeleteJobAction.Request(jobId), listener);
        listener.actionGet();
    }

    public void testMachineLearningDeleteDatafeedActionNotRestricted() throws Exception {
        String jobId = "testmachinelearningdeletedatafeedactionnotrestricted";
        String datafeedId = jobId + "-datafeed";
        assertMLAllowed(true);
        // test that license restricted apis do now work
        PlainActionFuture<PutJobAction.Response> putJobListener = PlainActionFuture.newFuture();
        client().execute(PutJobAction.INSTANCE, new PutJobAction.Request(createJob(jobId)), putJobListener);
        PutJobAction.Response putJobResponse = putJobListener.actionGet();
        assertNotNull(putJobResponse);
        PlainActionFuture<PutDatafeedAction.Response> putDatafeedListener = PlainActionFuture.newFuture();
        client().execute(PutDatafeedAction.INSTANCE, 
                new PutDatafeedAction.Request(createDatafeed(datafeedId, jobId,
                        Collections.singletonList(jobId))), putDatafeedListener);
        PutDatafeedAction.Response putDatafeedResponse = putDatafeedListener.actionGet();
        assertNotNull(putDatafeedResponse);

        // Pick a random license
        License.OperationMode mode = randomLicenseType();
        enableLicensing(mode);

        PlainActionFuture<AcknowledgedResponse> listener = PlainActionFuture.newFuture();
        client().execute(DeleteDatafeedAction.INSTANCE, new DeleteDatafeedAction.Request(datafeedId), listener);
        listener.actionGet();
    }

    public void testMachineLearningCreateInferenceProcessorRestricted() {
        String modelId = "modelprocessorlicensetest";
        assertMLAllowed(true);
        putInferenceModel(modelId);

        String pipeline = "{" +
            "    \"processors\": [\n" +
            "      {\n" +
            "        \"inference\": {\n" +
            "          \"target_field\": \"regression_value\",\n" +
            "          \"model_id\": \"modelprocessorlicensetest\",\n" +
            "          \"inference_config\": {\"regression\": {}},\n" +
            "          \"field_mappings\": {\n" +
            "            \"col1\": \"col1\",\n" +
            "            \"col2\": \"col2\",\n" +
            "            \"col3\": \"col3\",\n" +
            "            \"col4\": \"col4\"\n" +
            "          }\n" +
            "        }\n" +
            "      }]}\n";
        // test that license restricted apis do now work
        PlainActionFuture<AcknowledgedResponse> putPipelineListener = PlainActionFuture.newFuture();
        client().execute(PutPipelineAction.INSTANCE,
            new PutPipelineRequest("test_infer_license_pipeline",
                new BytesArray(pipeline.getBytes(StandardCharsets.UTF_8)),
                XContentType.JSON),
            putPipelineListener);
        AcknowledgedResponse putPipelineResponse = putPipelineListener.actionGet();
        assertTrue(putPipelineResponse.isAcknowledged());

        String simulateSource = "{\n" +
            "  \"pipeline\": \n" +
            pipeline +
            "  ,\n" +
            "  \"docs\": [\n" +
            "    {\"_source\": {\n" +
            "      \"col1\": \"female\",\n" +
            "      \"col2\": \"M\",\n" +
            "      \"col3\": \"none\",\n" +
            "      \"col4\": 10\n" +
            "    }}]\n" +
            "}";
        PlainActionFuture<SimulatePipelineResponse> simulatePipelineListener = PlainActionFuture.newFuture();
        client().execute(SimulatePipelineAction.INSTANCE,
            new SimulatePipelineRequest(new BytesArray(simulateSource.getBytes(StandardCharsets.UTF_8)), XContentType.JSON),
            simulatePipelineListener);

        assertThat(simulatePipelineListener.actionGet().getResults(), is(not(empty())));


        // Pick a license that does not allow machine learning
        License.OperationMode mode = randomInvalidLicenseType();
        enableLicensing(mode);
        assertMLAllowed(false);

        // creating a new pipeline should fail
        ElasticsearchSecurityException e = expectThrows(ElasticsearchSecurityException.class, () -> {
            PlainActionFuture<AcknowledgedResponse> listener = PlainActionFuture.newFuture();
            client().execute(PutPipelineAction.INSTANCE,
                new PutPipelineRequest("test_infer_license_pipeline_failure",
                    new BytesArray(pipeline.getBytes(StandardCharsets.UTF_8)),
                    XContentType.JSON),
                listener);
            listener.actionGet();
        });
        assertThat(e.status(), is(RestStatus.FORBIDDEN));
        assertThat(e.getMessage(), containsString("non-compliant"));
        assertThat(e.getMetadata(LicenseUtils.EXPIRED_FEATURE_METADATA), hasItem(XPackField.MACHINE_LEARNING));

        // Simulating the pipeline should fail
        e = expectThrows(ElasticsearchSecurityException.class, () -> {
            PlainActionFuture<SimulatePipelineResponse> listener = PlainActionFuture.newFuture();
            client().execute(SimulatePipelineAction.INSTANCE,
                new SimulatePipelineRequest(new BytesArray(simulateSource.getBytes(StandardCharsets.UTF_8)), XContentType.JSON),
                listener);
            listener.actionGet();
        });
        assertThat(e.status(), is(RestStatus.FORBIDDEN));
        assertThat(e.getMessage(), containsString("non-compliant"));
        assertThat(e.getMetadata(LicenseUtils.EXPIRED_FEATURE_METADATA), hasItem(XPackField.MACHINE_LEARNING));

        // Pick a license that does allow machine learning
        mode = randomValidLicenseType();
        enableLicensing(mode);
        assertMLAllowed(true);
        // test that license restricted apis do now work
        PlainActionFuture<AcknowledgedResponse> putPipelineListenerNewLicense = PlainActionFuture.newFuture();
        client().execute(PutPipelineAction.INSTANCE,
            new PutPipelineRequest("test_infer_license_pipeline",
                new BytesArray(pipeline.getBytes(StandardCharsets.UTF_8)),
                XContentType.JSON),
            putPipelineListenerNewLicense);
        AcknowledgedResponse putPipelineResponseNewLicense = putPipelineListenerNewLicense.actionGet();
        assertTrue(putPipelineResponseNewLicense.isAcknowledged());

        PlainActionFuture<SimulatePipelineResponse> simulatePipelineListenerNewLicense = PlainActionFuture.newFuture();
        client().execute(SimulatePipelineAction.INSTANCE,
            new SimulatePipelineRequest(new BytesArray(simulateSource.getBytes(StandardCharsets.UTF_8)), XContentType.JSON),
            simulatePipelineListenerNewLicense);

        assertThat(simulatePipelineListenerNewLicense.actionGet().getResults(), is(not(empty())));
    }

    public void testMachineLearningInferModelRestricted() throws Exception {
        String modelId = "modelinfermodellicensetest";
        assertMLAllowed(true);
        putInferenceModel(modelId);


        PlainActionFuture<InferModelAction.Response> inferModelSuccess = PlainActionFuture.newFuture();
        client().execute(InferModelAction.INSTANCE, new InferModelAction.Request(
            modelId,
            Collections.singletonList(Collections.emptyMap()),
            new RegressionConfig()
        ), inferModelSuccess);
        assertThat(inferModelSuccess.actionGet().getInferenceResults(), is(not(empty())));

        // Pick a license that does not allow machine learning
        License.OperationMode mode = randomInvalidLicenseType();
        enableLicensing(mode);
        assertMLAllowed(false);

        // inferring against a model should now fail
        ElasticsearchSecurityException e = expectThrows(ElasticsearchSecurityException.class, () -> {
            PlainActionFuture<InferModelAction.Response> listener = PlainActionFuture.newFuture();
            client().execute(InferModelAction.INSTANCE, new InferModelAction.Request(
                modelId,
                Collections.singletonList(Collections.emptyMap()),
                new RegressionConfig()
            ), listener);
            listener.actionGet();
        });
        assertThat(e.status(), is(RestStatus.FORBIDDEN));
        assertThat(e.getMessage(), containsString("non-compliant"));
        assertThat(e.getMetadata(LicenseUtils.EXPIRED_FEATURE_METADATA), hasItem(XPackField.MACHINE_LEARNING));

        // Pick a license that does allow machine learning
        mode = randomValidLicenseType();
        enableLicensing(mode);
        assertMLAllowed(true);

        PlainActionFuture<InferModelAction.Response> listener = PlainActionFuture.newFuture();
        client().execute(InferModelAction.INSTANCE, new InferModelAction.Request(
            modelId,
            Collections.singletonList(Collections.emptyMap()),
            new RegressionConfig()
        ), listener);
        assertThat(listener.actionGet().getInferenceResults(), is(not(empty())));
    }

    private void putInferenceModel(String modelId) {
        String config = "" +
            "{\n" +
            "  \"model_id\": \"" + modelId + "\",\n" +
            "  \"input\":{\"field_names\":[\"col1\",\"col2\",\"col3\",\"col4\"]}," +
            "  \"description\": \"test model for classification\",\n" +
            "  \"version\": \"8.0.0\",\n" +
            "  \"created_by\": \"benwtrent\",\n" +
            "  \"estimated_heap_memory_usage_bytes\": 0,\n" +
            "  \"estimated_operations\": 0,\n" +
            "  \"created_time\": 0\n" +
            "}";
        String definition = "" +
            "{" +
            "  \"trained_model\": {\n" +
            "    \"tree\": {\n" +
            "      \"feature_names\": [\n" +
            "        \"col1_male\",\n" +
            "        \"col1_female\",\n" +
            "        \"col2_encoded\",\n" +
            "        \"col3_encoded\",\n" +
            "        \"col4\"\n" +
            "      ],\n" +
            "      \"tree_structure\": [\n" +
            "        {\n" +
            "          \"node_index\": 0,\n" +
            "            \"split_feature\": 0,\n" +
            "            \"split_gain\": 12.0,\n" +
            "            \"threshold\": 10.0,\n" +
            "            \"decision_type\": \"lte\",\n" +
            "            \"default_left\": true,\n" +
            "            \"left_child\": 1,\n" +
            "            \"right_child\": 2\n" +
            "         },\n" +
            "         {\n" +
            "           \"node_index\": 1,\n" +
            "           \"leaf_value\": 1\n" +
            "         },\n" +
            "         {\n" +
            "           \"node_index\": 2,\n" +
            "           \"leaf_value\": 2\n" +
            "         }\n" +
            "      ],\n" +
            "     \"target_type\": \"regression\"\n" +
            "    }\n" +
            "  }," +
            "  \"model_id\": \"" + modelId + "\"\n" +
            "}";
        assertThat(client().prepareIndex(InferenceIndexConstants.LATEST_INDEX_NAME)
            .setId(modelId)
            .setSource(config, XContentType.JSON)
            .setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE)
            .get().status(), equalTo(RestStatus.CREATED));
        assertThat(client().prepareIndex(InferenceIndexConstants.LATEST_INDEX_NAME)
            .setId(TrainedModelDefinition.docId(modelId))
            .setSource(definition, XContentType.JSON)
            .setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE)
            .get().status(), equalTo(RestStatus.CREATED));
    }

    private static OperationMode randomInvalidLicenseType() {
        return randomFrom(License.OperationMode.GOLD, License.OperationMode.STANDARD, License.OperationMode.BASIC);
    }

    private static OperationMode randomValidLicenseType() {
        return randomFrom(License.OperationMode.TRIAL, License.OperationMode.PLATINUM);
    }

    private static OperationMode randomLicenseType() {
        return randomFrom(License.OperationMode.values());
    }

    private static void assertMLAllowed(boolean expected) {
        for (XPackLicenseState licenseState : internalCluster().getInstances(XPackLicenseState.class)) {
            assertEquals(licenseState.isMachineLearningAllowed(), expected);
        }
    }

    public static void disableLicensing() {
        disableLicensing(randomValidLicenseType());
    }

    public static void disableLicensing(License.OperationMode operationMode) {
        for (XPackLicenseState licenseState : internalCluster().getInstances(XPackLicenseState.class)) {
            licenseState.update(operationMode, false, null);
        }
    }

    public static void enableLicensing() {
        enableLicensing(randomValidLicenseType());
    }

    public static void enableLicensing(License.OperationMode operationMode) {
        for (XPackLicenseState licenseState : internalCluster().getInstances(XPackLicenseState.class)) {
            licenseState.update(operationMode, true, null);
        }
    }
}
