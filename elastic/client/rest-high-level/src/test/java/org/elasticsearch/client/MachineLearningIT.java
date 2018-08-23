/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */
package org.elasticsearch.client;

import com.carrotsearch.randomizedtesting.generators.CodepointSetGenerator;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.protocol.xpack.ml.CloseJobRequest;
import org.elasticsearch.protocol.xpack.ml.CloseJobResponse;
import org.elasticsearch.protocol.xpack.ml.DeleteJobRequest;
import org.elasticsearch.protocol.xpack.ml.DeleteJobResponse;
import org.elasticsearch.protocol.xpack.ml.GetJobRequest;
import org.elasticsearch.protocol.xpack.ml.GetJobResponse;
import org.elasticsearch.protocol.xpack.ml.OpenJobRequest;
import org.elasticsearch.protocol.xpack.ml.OpenJobResponse;
import org.elasticsearch.protocol.xpack.ml.PutJobRequest;
import org.elasticsearch.protocol.xpack.ml.PutJobResponse;
import org.elasticsearch.protocol.xpack.ml.job.config.AnalysisConfig;
import org.elasticsearch.protocol.xpack.ml.job.config.DataDescription;
import org.elasticsearch.protocol.xpack.ml.job.config.Detector;
import org.elasticsearch.protocol.xpack.ml.job.config.Job;
import org.junit.After;

import java.io.IOException;
import java.util.Arrays;
import java.util.concurrent.TimeUnit;
import java.util.stream.Collectors;

import static org.hamcrest.CoreMatchers.hasItems;
import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.is;

public class MachineLearningIT extends ESRestHighLevelClientTestCase {

    @After
    public void cleanUp() throws IOException {
        new MlRestTestStateCleaner(logger, client()).clearMlMetadata();
    }

    public void testPutJob() throws Exception {
        String jobId = randomValidJobId();
        Job job = buildJob(jobId);
        MachineLearningClient machineLearningClient = highLevelClient().machineLearning();

        PutJobResponse putJobResponse = execute(new PutJobRequest(job), machineLearningClient::putJob, machineLearningClient::putJobAsync);
        Job createdJob = putJobResponse.getResponse();

        assertThat(createdJob.getId(), is(jobId));
        assertThat(createdJob.getJobType(), is(Job.ANOMALY_DETECTOR_JOB_TYPE));
    }

    public void testGetJob() throws Exception {
        String jobId1 = randomValidJobId();
        String jobId2 = randomValidJobId();

        Job job1 = buildJob(jobId1);
        Job job2 = buildJob(jobId2);
        MachineLearningClient machineLearningClient = highLevelClient().machineLearning();
        machineLearningClient.putJob(new PutJobRequest(job1), RequestOptions.DEFAULT);
        machineLearningClient.putJob(new PutJobRequest(job2), RequestOptions.DEFAULT);

        GetJobRequest request = new GetJobRequest(jobId1, jobId2);

        // Test getting specific jobs
        GetJobResponse response = execute(request, machineLearningClient::getJob, machineLearningClient::getJobAsync);

        assertEquals(2, response.count());
        assertThat(response.jobs(), hasSize(2));
        assertThat(response.jobs().stream().map(Job::getId).collect(Collectors.toList()), containsInAnyOrder(jobId1, jobId2));

        // Test getting all jobs explicitly
        request = GetJobRequest.getAllJobsRequest();
        response = execute(request, machineLearningClient::getJob, machineLearningClient::getJobAsync);

        assertTrue(response.count() >= 2L);
        assertTrue(response.jobs().size() >= 2L);
        assertThat(response.jobs().stream().map(Job::getId).collect(Collectors.toList()), hasItems(jobId1, jobId2));

        // Test getting all jobs implicitly
        response = execute(new GetJobRequest(), machineLearningClient::getJob, machineLearningClient::getJobAsync);

        assertTrue(response.count() >= 2L);
        assertTrue(response.jobs().size() >= 2L);
        assertThat(response.jobs().stream().map(Job::getId).collect(Collectors.toList()), hasItems(jobId1, jobId2));
    }

    public void testDeleteJob() throws Exception {
        String jobId = randomValidJobId();
        Job job = buildJob(jobId);
        MachineLearningClient machineLearningClient = highLevelClient().machineLearning();
        machineLearningClient.putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

        DeleteJobResponse response = execute(new DeleteJobRequest(jobId),
            machineLearningClient::deleteJob,
            machineLearningClient::deleteJobAsync);

        assertTrue(response.isAcknowledged());
    }

    public void testOpenJob() throws Exception {
        String jobId = randomValidJobId();
        Job job = buildJob(jobId);
        MachineLearningClient machineLearningClient = highLevelClient().machineLearning();

        machineLearningClient.putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

        OpenJobResponse response = execute(new OpenJobRequest(jobId), machineLearningClient::openJob, machineLearningClient::openJobAsync);

        assertTrue(response.isOpened());
    }

    public void testCloseJob() throws Exception {
        String jobId = randomValidJobId();
        Job job = buildJob(jobId);
        MachineLearningClient machineLearningClient = highLevelClient().machineLearning();
        machineLearningClient.putJob(new PutJobRequest(job), RequestOptions.DEFAULT);
        machineLearningClient.openJob(new OpenJobRequest(jobId), RequestOptions.DEFAULT);

        CloseJobResponse response = execute(new CloseJobRequest(jobId),
            machineLearningClient::closeJob,
            machineLearningClient::closeJobAsync);
        assertTrue(response.isClosed());
    }

    public static String randomValidJobId() {
        CodepointSetGenerator generator = new CodepointSetGenerator("abcdefghijklmnopqrstuvwxyz0123456789".toCharArray());
        return generator.ofCodePointsLength(random(), 10, 10);
    }

    public static Job buildJob(String jobId) {
        Job.Builder builder = new Job.Builder(jobId);
        builder.setDescription(randomAlphaOfLength(10));

        Detector detector = new Detector.Builder()
            .setFieldName("total")
            .setFunction("sum")
            .setDetectorDescription(randomAlphaOfLength(10))
            .build();
        AnalysisConfig.Builder configBuilder = new AnalysisConfig.Builder(Arrays.asList(detector));
        configBuilder.setBucketSpan(new TimeValue(randomIntBetween(1, 10), TimeUnit.SECONDS));
        builder.setAnalysisConfig(configBuilder);

        DataDescription.Builder dataDescription = new DataDescription.Builder();
        dataDescription.setTimeFormat(randomFrom(DataDescription.EPOCH_MS, DataDescription.EPOCH));
        dataDescription.setTimeField(randomAlphaOfLength(10));
        builder.setDataDescription(dataDescription);

        return builder.build();
    }
}
