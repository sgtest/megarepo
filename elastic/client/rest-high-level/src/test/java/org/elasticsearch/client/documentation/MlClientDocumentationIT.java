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
package org.elasticsearch.client.documentation;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.LatchedActionListener;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.client.ESRestHighLevelClientTestCase;
import org.elasticsearch.client.MachineLearningIT;
import org.elasticsearch.client.MlRestTestStateCleaner;
import org.elasticsearch.client.RequestOptions;
import org.elasticsearch.client.RestHighLevelClient;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.protocol.xpack.ml.CloseJobRequest;
import org.elasticsearch.protocol.xpack.ml.CloseJobResponse;
import org.elasticsearch.protocol.xpack.ml.DeleteJobRequest;
import org.elasticsearch.protocol.xpack.ml.DeleteJobResponse;
import org.elasticsearch.protocol.xpack.ml.GetBucketsRequest;
import org.elasticsearch.protocol.xpack.ml.GetBucketsResponse;
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
import org.elasticsearch.protocol.xpack.ml.job.results.Bucket;
import org.elasticsearch.protocol.xpack.ml.job.util.PageParams;
import org.junit.After;

import java.io.IOException;
import java.util.Collections;
import java.util.Date;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.stream.Collectors;

import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.greaterThan;
import static org.hamcrest.Matchers.hasSize;

public class MlClientDocumentationIT extends ESRestHighLevelClientTestCase {

    @After
    public void cleanUp() throws IOException {
        new MlRestTestStateCleaner(logger, client()).clearMlMetadata();
    }

    public void testCreateJob() throws Exception {
        RestHighLevelClient client = highLevelClient();

        //tag::x-pack-ml-put-job-detector
        Detector.Builder detectorBuilder = new Detector.Builder()
            .setFunction("sum")                                    // <1>
            .setFieldName("total")                                 // <2>
            .setDetectorDescription("Sum of total");               // <3>
        //end::x-pack-ml-put-job-detector

        //tag::x-pack-ml-put-job-analysis-config
        List<Detector> detectors = Collections.singletonList(detectorBuilder.build());       // <1>
        AnalysisConfig.Builder analysisConfigBuilder = new AnalysisConfig.Builder(detectors) // <2>
            .setBucketSpan(TimeValue.timeValueMinutes(10));                                  // <3>
        //end::x-pack-ml-put-job-analysis-config

        //tag::x-pack-ml-put-job-data-description
        DataDescription.Builder dataDescriptionBuilder = new DataDescription.Builder()
            .setTimeField("timestamp");  // <1>
        //end::x-pack-ml-put-job-data-description

        {
            String id = "job_1";

            //tag::x-pack-ml-put-job-config
            Job.Builder jobBuilder = new Job.Builder(id)      // <1>
                .setAnalysisConfig(analysisConfigBuilder)     // <2>
                .setDataDescription(dataDescriptionBuilder)   // <3>
                .setDescription("Total sum of requests");     // <4>
            //end::x-pack-ml-put-job-config

            //tag::x-pack-ml-put-job-request
            PutJobRequest request = new PutJobRequest(jobBuilder.build()); // <1>
            //end::x-pack-ml-put-job-request

            //tag::x-pack-ml-put-job-execute
            PutJobResponse response = client.machineLearning().putJob(request, RequestOptions.DEFAULT);
            //end::x-pack-ml-put-job-execute

            //tag::x-pack-ml-put-job-response
            Date createTime = response.getResponse().getCreateTime(); // <1>
            //end::x-pack-ml-put-job-response
            assertThat(createTime.getTime(), greaterThan(0L));
        }
        {
            String id = "job_2";
            Job.Builder jobBuilder = new Job.Builder(id)
                .setAnalysisConfig(analysisConfigBuilder)
                .setDataDescription(dataDescriptionBuilder)
                .setDescription("Total sum of requests");

            PutJobRequest request = new PutJobRequest(jobBuilder.build());
            // tag::x-pack-ml-put-job-execute-listener
            ActionListener<PutJobResponse> listener = new ActionListener<PutJobResponse>() {
                @Override
                public void onResponse(PutJobResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::x-pack-ml-put-job-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::x-pack-ml-put-job-execute-async
            client.machineLearning().putJobAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::x-pack-ml-put-job-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testGetJob() throws Exception {
        RestHighLevelClient client = highLevelClient();

        String jobId = "get-machine-learning-job1";

        Job job = MachineLearningIT.buildJob("get-machine-learning-job1");
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

        Job secondJob = MachineLearningIT.buildJob("get-machine-learning-job2");
        client.machineLearning().putJob(new PutJobRequest(secondJob), RequestOptions.DEFAULT);

        {
            //tag::x-pack-ml-get-job-request
            GetJobRequest request = new GetJobRequest("get-machine-learning-job1", "get-machine-learning-job*"); //<1>
            request.setAllowNoJobs(true); //<2>
            //end::x-pack-ml-get-job-request

            //tag::x-pack-ml-get-job-execute
            GetJobResponse response = client.machineLearning().getJob(request, RequestOptions.DEFAULT);
            long numberOfJobs = response.count(); //<1>
            List<Job> jobs = response.jobs(); //<2>
            //end::x-pack-ml-get-job-execute

            assertEquals(2, response.count());
            assertThat(response.jobs(), hasSize(2));
            assertThat(response.jobs().stream().map(Job::getId).collect(Collectors.toList()),
                containsInAnyOrder(job.getId(), secondJob.getId()));
        }
        {
            GetJobRequest request = new GetJobRequest("get-machine-learning-job1", "get-machine-learning-job*");

            // tag::x-pack-ml-get-job-listener
            ActionListener<GetJobResponse> listener = new ActionListener<GetJobResponse>() {
                @Override
                public void onResponse(GetJobResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::x-pack-ml-get-job-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::x-pack-ml-get-job-execute-async
            client.machineLearning().getJobAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::x-pack-ml-get-job-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testDeleteJob() throws Exception {
        RestHighLevelClient client = highLevelClient();

        String jobId = "my-first-machine-learning-job";

        Job job = MachineLearningIT.buildJob(jobId);
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

        Job secondJob = MachineLearningIT.buildJob("my-second-machine-learning-job");
        client.machineLearning().putJob(new PutJobRequest(secondJob), RequestOptions.DEFAULT);

        {
            //tag::x-pack-delete-ml-job-request
            DeleteJobRequest deleteJobRequest = new DeleteJobRequest("my-first-machine-learning-job");
            deleteJobRequest.setForce(false); //<1>
            DeleteJobResponse deleteJobResponse = client.machineLearning().deleteJob(deleteJobRequest, RequestOptions.DEFAULT);
            //end::x-pack-delete-ml-job-request

            //tag::x-pack-delete-ml-job-response
            boolean isAcknowledged = deleteJobResponse.isAcknowledged(); //<1>
            //end::x-pack-delete-ml-job-response
        }
        {
            //tag::x-pack-delete-ml-job-request-listener
            ActionListener<DeleteJobResponse> listener = new ActionListener<DeleteJobResponse>() {
                @Override
                public void onResponse(DeleteJobResponse deleteJobResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            //end::x-pack-delete-ml-job-request-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            //tag::x-pack-delete-ml-job-request-async
            DeleteJobRequest deleteJobRequest = new DeleteJobRequest("my-second-machine-learning-job");
            client.machineLearning().deleteJobAsync(deleteJobRequest, RequestOptions.DEFAULT, listener); // <1>
            //end::x-pack-delete-ml-job-request-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testOpenJob() throws Exception {
        RestHighLevelClient client = highLevelClient();

        Job job = MachineLearningIT.buildJob("opening-my-first-machine-learning-job");
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

        Job secondJob = MachineLearningIT.buildJob("opening-my-second-machine-learning-job");
        client.machineLearning().putJob(new PutJobRequest(secondJob), RequestOptions.DEFAULT);

        {
            //tag::x-pack-ml-open-job-request
            OpenJobRequest openJobRequest = new OpenJobRequest("opening-my-first-machine-learning-job"); //<1>
            openJobRequest.setTimeout(TimeValue.timeValueMinutes(10)); //<2>
            //end::x-pack-ml-open-job-request

            //tag::x-pack-ml-open-job-execute
            OpenJobResponse openJobResponse = client.machineLearning().openJob(openJobRequest, RequestOptions.DEFAULT);
            boolean isOpened = openJobResponse.isOpened(); //<1>
            //end::x-pack-ml-open-job-execute

        }
        {
            //tag::x-pack-ml-open-job-listener
            ActionListener<OpenJobResponse> listener = new ActionListener<OpenJobResponse>() {
                @Override
                public void onResponse(OpenJobResponse openJobResponse) {
                    //<1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            //end::x-pack-ml-open-job-listener
            OpenJobRequest openJobRequest = new OpenJobRequest("opening-my-second-machine-learning-job");
            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::x-pack-ml-open-job-execute-async
            client.machineLearning().openJobAsync(openJobRequest, RequestOptions.DEFAULT, listener); //<1>
            // end::x-pack-ml-open-job-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testCloseJob() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            Job job = MachineLearningIT.buildJob("closing-my-first-machine-learning-job");
            client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);
            client.machineLearning().openJob(new OpenJobRequest(job.getId()), RequestOptions.DEFAULT);

            //tag::x-pack-ml-close-job-request
            CloseJobRequest closeJobRequest = new CloseJobRequest("closing-my-first-machine-learning-job", "otherjobs*"); //<1>
            closeJobRequest.setForce(false); //<2>
            closeJobRequest.setAllowNoJobs(true); //<3>
            closeJobRequest.setTimeout(TimeValue.timeValueMinutes(10)); //<4>
            //end::x-pack-ml-close-job-request

            //tag::x-pack-ml-close-job-execute
            CloseJobResponse closeJobResponse = client.machineLearning().closeJob(closeJobRequest, RequestOptions.DEFAULT);
            boolean isClosed = closeJobResponse.isClosed(); //<1>
            //end::x-pack-ml-close-job-execute

        }
        {
            Job job = MachineLearningIT.buildJob("closing-my-second-machine-learning-job");
            client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);
            client.machineLearning().openJob(new OpenJobRequest(job.getId()), RequestOptions.DEFAULT);

            //tag::x-pack-ml-close-job-listener
            ActionListener<CloseJobResponse> listener = new ActionListener<CloseJobResponse>() {
                @Override
                public void onResponse(CloseJobResponse closeJobResponse) {
                    //<1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            //end::x-pack-ml-close-job-listener
            CloseJobRequest closeJobRequest = new CloseJobRequest("closing-my-second-machine-learning-job");

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::x-pack-ml-close-job-execute-async
            client.machineLearning().closeJobAsync(closeJobRequest, RequestOptions.DEFAULT, listener); //<1>
            // end::x-pack-ml-close-job-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testGetBuckets() throws IOException, InterruptedException {
        RestHighLevelClient client = highLevelClient();

        String jobId = "test-get-buckets";
        Job job = MachineLearningIT.buildJob(jobId);
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

        // Let us index a bucket
        IndexRequest indexRequest = new IndexRequest(".ml-anomalies-shared", "doc");
        indexRequest.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);
        indexRequest.source("{\"job_id\":\"test-get-buckets\", \"result_type\":\"bucket\", \"timestamp\": 1533081600000," +
                        "\"bucket_span\": 600,\"is_interim\": false, \"anomaly_score\": 80.0}", XContentType.JSON);
        client.index(indexRequest, RequestOptions.DEFAULT);

        {
            // tag::x-pack-ml-get-buckets-request
            GetBucketsRequest request = new GetBucketsRequest(jobId); // <1>
            // end::x-pack-ml-get-buckets-request

            // tag::x-pack-ml-get-buckets-timestamp
            request.setTimestamp("2018-08-17T00:00:00Z"); // <1>
            // end::x-pack-ml-get-buckets-timestamp

            // Set timestamp to null as it is incompatible with other args
            request.setTimestamp(null);

            // tag::x-pack-ml-get-buckets-anomaly-score
            request.setAnomalyScore(75.0); // <1>
            // end::x-pack-ml-get-buckets-anomaly-score

            // tag::x-pack-ml-get-buckets-desc
            request.setDescending(true); // <1>
            // end::x-pack-ml-get-buckets-desc

            // tag::x-pack-ml-get-buckets-end
            request.setEnd("2018-08-21T00:00:00Z"); // <1>
            // end::x-pack-ml-get-buckets-end

            // tag::x-pack-ml-get-buckets-exclude-interim
            request.setExcludeInterim(true); // <1>
            // end::x-pack-ml-get-buckets-exclude-interim

            // tag::x-pack-ml-get-buckets-expand
            request.setExpand(true); // <1>
            // end::x-pack-ml-get-buckets-expand

            // tag::x-pack-ml-get-buckets-page
            request.setPageParams(new PageParams(100, 200)); // <1>
            // end::x-pack-ml-get-buckets-page

            // Set page params back to null so the response contains the bucket we indexed
            request.setPageParams(null);

            // tag::x-pack-ml-get-buckets-sort
            request.setSort("anomaly_score"); // <1>
            // end::x-pack-ml-get-buckets-sort

            // tag::x-pack-ml-get-buckets-start
            request.setStart("2018-08-01T00:00:00Z"); // <1>
            // end::x-pack-ml-get-buckets-start

            // tag::x-pack-ml-get-buckets-execute
            GetBucketsResponse response = client.machineLearning().getBuckets(request, RequestOptions.DEFAULT);
            // end::x-pack-ml-get-buckets-execute

            // tag::x-pack-ml-get-buckets-response
            long count = response.count(); // <1>
            List<Bucket> buckets = response.buckets(); // <2>
            // end::x-pack-ml-get-buckets-response
            assertEquals(1, buckets.size());
        }
        {
            GetBucketsRequest request = new GetBucketsRequest(jobId);

            // tag::x-pack-ml-get-buckets-listener
            ActionListener<GetBucketsResponse> listener =
                    new ActionListener<GetBucketsResponse>() {
                        @Override
                        public void onResponse(GetBucketsResponse getBucketsResponse) {
                            // <1>
                        }

                        @Override
                        public void onFailure(Exception e) {
                            // <2>
                        }
                    };
            // end::x-pack-ml-get-buckets-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::x-pack-ml-get-buckets-execute-async
            client.machineLearning().getBucketsAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::x-pack-ml-get-buckets-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }
}
