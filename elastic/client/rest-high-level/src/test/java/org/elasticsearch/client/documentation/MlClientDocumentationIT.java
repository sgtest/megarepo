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
import org.elasticsearch.action.admin.indices.create.CreateIndexRequest;
import org.elasticsearch.action.bulk.BulkRequest;
import org.elasticsearch.action.get.GetRequest;
import org.elasticsearch.action.get.GetResponse;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.client.ESRestHighLevelClientTestCase;
import org.elasticsearch.client.MachineLearningGetResultsIT;
import org.elasticsearch.client.MachineLearningIT;
import org.elasticsearch.client.MlTestStateCleaner;
import org.elasticsearch.client.RequestOptions;
import org.elasticsearch.client.RestHighLevelClient;
import org.elasticsearch.client.ml.CloseJobRequest;
import org.elasticsearch.client.ml.CloseJobResponse;
import org.elasticsearch.client.ml.DeleteCalendarRequest;
import org.elasticsearch.client.ml.DeleteDatafeedRequest;
import org.elasticsearch.client.ml.DeleteForecastRequest;
import org.elasticsearch.client.ml.DeleteJobRequest;
import org.elasticsearch.client.ml.DeleteJobResponse;
import org.elasticsearch.client.ml.FlushJobRequest;
import org.elasticsearch.client.ml.FlushJobResponse;
import org.elasticsearch.client.ml.ForecastJobRequest;
import org.elasticsearch.client.ml.ForecastJobResponse;
import org.elasticsearch.client.ml.GetBucketsRequest;
import org.elasticsearch.client.ml.GetBucketsResponse;
import org.elasticsearch.client.ml.GetCalendarsRequest;
import org.elasticsearch.client.ml.GetCalendarsResponse;
import org.elasticsearch.client.ml.GetCategoriesRequest;
import org.elasticsearch.client.ml.GetCategoriesResponse;
import org.elasticsearch.client.ml.GetDatafeedRequest;
import org.elasticsearch.client.ml.GetDatafeedResponse;
import org.elasticsearch.client.ml.GetDatafeedStatsRequest;
import org.elasticsearch.client.ml.GetDatafeedStatsResponse;
import org.elasticsearch.client.ml.GetInfluencersRequest;
import org.elasticsearch.client.ml.GetInfluencersResponse;
import org.elasticsearch.client.ml.GetJobRequest;
import org.elasticsearch.client.ml.GetJobResponse;
import org.elasticsearch.client.ml.GetJobStatsRequest;
import org.elasticsearch.client.ml.GetJobStatsResponse;
import org.elasticsearch.client.ml.GetOverallBucketsRequest;
import org.elasticsearch.client.ml.GetOverallBucketsResponse;
import org.elasticsearch.client.ml.GetRecordsRequest;
import org.elasticsearch.client.ml.GetRecordsResponse;
import org.elasticsearch.client.ml.OpenJobRequest;
import org.elasticsearch.client.ml.OpenJobResponse;
import org.elasticsearch.client.ml.PostDataRequest;
import org.elasticsearch.client.ml.PostDataResponse;
import org.elasticsearch.client.ml.PreviewDatafeedRequest;
import org.elasticsearch.client.ml.PreviewDatafeedResponse;
import org.elasticsearch.client.ml.PutCalendarRequest;
import org.elasticsearch.client.ml.PutCalendarResponse;
import org.elasticsearch.client.ml.PutDatafeedRequest;
import org.elasticsearch.client.ml.PutDatafeedResponse;
import org.elasticsearch.client.ml.PutJobRequest;
import org.elasticsearch.client.ml.PutJobResponse;
import org.elasticsearch.client.ml.StartDatafeedRequest;
import org.elasticsearch.client.ml.StartDatafeedResponse;
import org.elasticsearch.client.ml.StopDatafeedRequest;
import org.elasticsearch.client.ml.StopDatafeedResponse;
import org.elasticsearch.client.ml.UpdateJobRequest;
import org.elasticsearch.client.ml.calendars.Calendar;
import org.elasticsearch.client.ml.datafeed.ChunkingConfig;
import org.elasticsearch.client.ml.datafeed.DatafeedConfig;
import org.elasticsearch.client.ml.datafeed.DatafeedStats;
import org.elasticsearch.client.ml.job.config.AnalysisConfig;
import org.elasticsearch.client.ml.job.config.AnalysisLimits;
import org.elasticsearch.client.ml.job.config.DataDescription;
import org.elasticsearch.client.ml.job.config.DetectionRule;
import org.elasticsearch.client.ml.job.config.Detector;
import org.elasticsearch.client.ml.job.config.Job;
import org.elasticsearch.client.ml.job.config.JobUpdate;
import org.elasticsearch.client.ml.job.config.ModelPlotConfig;
import org.elasticsearch.client.ml.job.config.Operator;
import org.elasticsearch.client.ml.job.config.RuleCondition;
import org.elasticsearch.client.ml.job.process.DataCounts;
import org.elasticsearch.client.ml.job.results.AnomalyRecord;
import org.elasticsearch.client.ml.job.results.Bucket;
import org.elasticsearch.client.ml.job.results.CategoryDefinition;
import org.elasticsearch.client.ml.job.results.Influencer;
import org.elasticsearch.client.ml.job.results.OverallBucket;
import org.elasticsearch.client.ml.job.stats.JobStats;
import org.elasticsearch.client.ml.job.util.PageParams;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.tasks.TaskId;
import org.junit.After;

import java.io.IOException;
import java.util.Arrays;
import java.util.Collections;
import java.util.Date;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.stream.Collectors;

import static org.hamcrest.Matchers.closeTo;
import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.core.Is.is;

public class MlClientDocumentationIT extends ESRestHighLevelClientTestCase {

    @After
    public void cleanUp() throws IOException {
        new MlTestStateCleaner(logger, highLevelClient().machineLearning()).clearMlMetadata();
    }

    public void testCreateJob() throws Exception {
        RestHighLevelClient client = highLevelClient();

        // tag::put-job-detector
        Detector.Builder detectorBuilder = new Detector.Builder()
            .setFunction("sum")                                    // <1>
            .setFieldName("total")                                 // <2>
            .setDetectorDescription("Sum of total");               // <3>
        // end::put-job-detector

        // tag::put-job-analysis-config
        List<Detector> detectors = Collections.singletonList(detectorBuilder.build());       // <1>
        AnalysisConfig.Builder analysisConfigBuilder = new AnalysisConfig.Builder(detectors) // <2>
            .setBucketSpan(TimeValue.timeValueMinutes(10));                                  // <3>
        // end::put-job-analysis-config

        // tag::put-job-data-description
        DataDescription.Builder dataDescriptionBuilder = new DataDescription.Builder()
            .setTimeField("timestamp");  // <1>
        // end::put-job-data-description

        {
            String id = "job_1";

            // tag::put-job-config
            Job.Builder jobBuilder = new Job.Builder(id)      // <1>
                .setAnalysisConfig(analysisConfigBuilder)     // <2>
                .setDataDescription(dataDescriptionBuilder)   // <3>
                .setDescription("Total sum of requests");     // <4>
            // end::put-job-config

            // tag::put-job-request
            PutJobRequest request = new PutJobRequest(jobBuilder.build()); // <1>
            // end::put-job-request

            // tag::put-job-execute
            PutJobResponse response = client.machineLearning().putJob(request, RequestOptions.DEFAULT);
            // end::put-job-execute

            // tag::put-job-response
            Date createTime = response.getResponse().getCreateTime(); // <1>
            // end::put-job-response
            assertThat(createTime.getTime(), greaterThan(0L));
        }
        {
            String id = "job_2";
            Job.Builder jobBuilder = new Job.Builder(id)
                .setAnalysisConfig(analysisConfigBuilder)
                .setDataDescription(dataDescriptionBuilder)
                .setDescription("Total sum of requests");

            PutJobRequest request = new PutJobRequest(jobBuilder.build());
            // tag::put-job-execute-listener
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
            // end::put-job-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::put-job-execute-async
            client.machineLearning().putJobAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::put-job-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testGetJob() throws Exception {
        RestHighLevelClient client = highLevelClient();

        Job job = MachineLearningIT.buildJob("get-machine-learning-job1");
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

        Job secondJob = MachineLearningIT.buildJob("get-machine-learning-job2");
        client.machineLearning().putJob(new PutJobRequest(secondJob), RequestOptions.DEFAULT);

        {
            // tag::get-job-request
            GetJobRequest request = new GetJobRequest("get-machine-learning-job1", "get-machine-learning-job*"); // <1>
            request.setAllowNoJobs(true); // <2>
            // end::get-job-request

            // tag::get-job-execute
            GetJobResponse response = client.machineLearning().getJob(request, RequestOptions.DEFAULT);
            // end::get-job-execute

            // tag::get-job-response
            long numberOfJobs = response.count(); // <1>
            List<Job> jobs = response.jobs(); // <2>
            // end::get-job-response
            assertEquals(2, response.count());
            assertThat(response.jobs(), hasSize(2));
            assertThat(response.jobs().stream().map(Job::getId).collect(Collectors.toList()),
                containsInAnyOrder(job.getId(), secondJob.getId()));
        }
        {
            GetJobRequest request = new GetJobRequest("get-machine-learning-job1", "get-machine-learning-job*");

            // tag::get-job-execute-listener
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
            // end::get-job-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::get-job-execute-async
            client.machineLearning().getJobAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::get-job-execute-async

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
            //tag::delete-job-request
            DeleteJobRequest deleteJobRequest = new DeleteJobRequest("my-first-machine-learning-job"); // <1>
            //end::delete-job-request

            //tag::delete-job-request-force
            deleteJobRequest.setForce(false); // <1>
            //end::delete-job-request-force

            //tag::delete-job-request-wait-for-completion
            deleteJobRequest.setWaitForCompletion(true); // <1>
            //end::delete-job-request-wait-for-completion

            //tag::delete-job-execute
            DeleteJobResponse deleteJobResponse = client.machineLearning().deleteJob(deleteJobRequest, RequestOptions.DEFAULT);
            //end::delete-job-execute

            //tag::delete-job-response
            Boolean isAcknowledged = deleteJobResponse.getAcknowledged(); // <1>
            TaskId task = deleteJobResponse.getTask(); // <2>
            //end::delete-job-response

            assertTrue(isAcknowledged);
            assertNull(task);
        }
        {
            //tag::delete-job-execute-listener
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
            // end::delete-job-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            DeleteJobRequest deleteJobRequest = new DeleteJobRequest("my-second-machine-learning-job");
            // tag::delete-job-execute-async
            client.machineLearning().deleteJobAsync(deleteJobRequest, RequestOptions.DEFAULT, listener); // <1>
            // end::delete-job-execute-async

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
            // tag::open-job-request
            OpenJobRequest openJobRequest = new OpenJobRequest("opening-my-first-machine-learning-job"); // <1>
            openJobRequest.setTimeout(TimeValue.timeValueMinutes(10)); // <2>
            // end::open-job-request

            // tag::open-job-execute
            OpenJobResponse openJobResponse = client.machineLearning().openJob(openJobRequest, RequestOptions.DEFAULT);
            // end::open-job-execute

            // tag::open-job-response
            boolean isOpened = openJobResponse.isOpened(); // <1>
            // end::open-job-response
        }
        {
            // tag::open-job-execute-listener
            ActionListener<OpenJobResponse> listener = new ActionListener<OpenJobResponse>() {
                @Override
                public void onResponse(OpenJobResponse openJobResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::open-job-execute-listener
            OpenJobRequest openJobRequest = new OpenJobRequest("opening-my-second-machine-learning-job");
            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::open-job-execute-async
            client.machineLearning().openJobAsync(openJobRequest, RequestOptions.DEFAULT, listener); // <1>
            // end::open-job-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testCloseJob() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            Job job = MachineLearningIT.buildJob("closing-my-first-machine-learning-job");
            client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);
            client.machineLearning().openJob(new OpenJobRequest(job.getId()), RequestOptions.DEFAULT);

            // tag::close-job-request
            CloseJobRequest closeJobRequest = new CloseJobRequest("closing-my-first-machine-learning-job", "otherjobs*"); // <1>
            closeJobRequest.setForce(false); // <2>
            closeJobRequest.setAllowNoJobs(true); // <3>
            closeJobRequest.setTimeout(TimeValue.timeValueMinutes(10)); // <4>
            // end::close-job-request

            // tag::close-job-execute
            CloseJobResponse closeJobResponse = client.machineLearning().closeJob(closeJobRequest, RequestOptions.DEFAULT);
            // end::close-job-execute

            // tag::close-job-response
            boolean isClosed = closeJobResponse.isClosed(); // <1>
            // end::close-job-response

        }
        {
            Job job = MachineLearningIT.buildJob("closing-my-second-machine-learning-job");
            client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);
            client.machineLearning().openJob(new OpenJobRequest(job.getId()), RequestOptions.DEFAULT);

            // tag::close-job-execute-listener
            ActionListener<CloseJobResponse> listener = new ActionListener<CloseJobResponse>() {
                @Override
                public void onResponse(CloseJobResponse closeJobResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::close-job-execute-listener
            CloseJobRequest closeJobRequest = new CloseJobRequest("closing-my-second-machine-learning-job");

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::close-job-execute-async
            client.machineLearning().closeJobAsync(closeJobRequest, RequestOptions.DEFAULT, listener); // <1>
            // end::close-job-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testUpdateJob() throws Exception {
        RestHighLevelClient client = highLevelClient();
        String jobId = "test-update-job";
        Job tempJob = MachineLearningIT.buildJob(jobId);
        Job job = new Job.Builder(tempJob)
            .setAnalysisConfig(new AnalysisConfig.Builder(tempJob.getAnalysisConfig())
                .setCategorizationFieldName("categorization-field")
                .setDetector(0,
                    new Detector.Builder().setFieldName("total")
                        .setFunction("sum")
                        .setPartitionFieldName("mlcategory")
                        .setDetectorDescription(randomAlphaOfLength(10))
                        .build()))
            .build();
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

        {

            List<DetectionRule> detectionRules = Arrays.asList(
                new DetectionRule.Builder(Arrays.asList(RuleCondition.createTime(Operator.GT, 100L))).build());
            Map<String, Object> customSettings = new HashMap<>();
            customSettings.put("custom-setting-1", "custom-value");

            // tag::update-job-detector-options
            JobUpdate.DetectorUpdate detectorUpdate = new JobUpdate.DetectorUpdate(0, // <1>
                "detector description", // <2>
                detectionRules); // <3>
            // end::update-job-detector-options

            // tag::update-job-options
            JobUpdate update = new JobUpdate.Builder(jobId) // <1>
                .setDescription("My description") // <2>
                .setAnalysisLimits(new AnalysisLimits(1000L, null)) // <3>
                .setBackgroundPersistInterval(TimeValue.timeValueHours(3)) // <4>
                .setCategorizationFilters(Arrays.asList("categorization-filter")) // <5>
                .setDetectorUpdates(Arrays.asList(detectorUpdate)) // <6>
                .setGroups(Arrays.asList("job-group-1")) // <7>
                .setResultsRetentionDays(10L) // <8>
                .setModelPlotConfig(new ModelPlotConfig(true, null)) // <9>
                .setModelSnapshotRetentionDays(7L) // <10>
                .setCustomSettings(customSettings) // <11>
                .setRenormalizationWindowDays(3L) // <12>
                .build();
            // end::update-job-options


            // tag::update-job-request
            UpdateJobRequest updateJobRequest = new UpdateJobRequest(update); // <1>
            // end::update-job-request

            // tag::update-job-execute
            PutJobResponse updateJobResponse = client.machineLearning().updateJob(updateJobRequest, RequestOptions.DEFAULT);
            // end::update-job-execute

            // tag::update-job-response
            Job updatedJob = updateJobResponse.getResponse(); // <1>
            // end::update-job-response

            assertEquals(update.getDescription(), updatedJob.getDescription());
        }
        {
            // tag::update-job-execute-listener
            ActionListener<PutJobResponse> listener = new ActionListener<PutJobResponse>() {
                @Override
                public void onResponse(PutJobResponse updateJobResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::update-job-execute-listener
            UpdateJobRequest updateJobRequest = new UpdateJobRequest(new JobUpdate.Builder(jobId).build());

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::update-job-execute-async
            client.machineLearning().updateJobAsync(updateJobRequest, RequestOptions.DEFAULT, listener); // <1>
            // end::update-job-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testPutDatafeed() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            // We need to create a job for the datafeed request to be valid
            String jobId = "put-datafeed-job-1";
            Job job = MachineLearningIT.buildJob(jobId);
            client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

            String id = "datafeed-1";

            // tag::put-datafeed-config
            DatafeedConfig.Builder datafeedBuilder = new DatafeedConfig.Builder(id, jobId) // <1>
                    .setIndices("index_1", "index_2");  // <2>
            // end::put-datafeed-config

            AggregatorFactories.Builder aggs = AggregatorFactories.builder();

            // tag::put-datafeed-config-set-aggregations
            datafeedBuilder.setAggregations(aggs); // <1>
            // end::put-datafeed-config-set-aggregations

            // Clearing aggregation to avoid complex validation rules
            datafeedBuilder.setAggregations((String) null);

            // tag::put-datafeed-config-set-chunking-config
            datafeedBuilder.setChunkingConfig(ChunkingConfig.newAuto()); // <1>
            // end::put-datafeed-config-set-chunking-config

            // tag::put-datafeed-config-set-frequency
            datafeedBuilder.setFrequency(TimeValue.timeValueSeconds(30)); // <1>
            // end::put-datafeed-config-set-frequency

            // tag::put-datafeed-config-set-query
            datafeedBuilder.setQuery(QueryBuilders.matchAllQuery()); // <1>
            // end::put-datafeed-config-set-query

            // tag::put-datafeed-config-set-query-delay
            datafeedBuilder.setQueryDelay(TimeValue.timeValueMinutes(1)); // <1>
            // end::put-datafeed-config-set-query-delay

            List<SearchSourceBuilder.ScriptField> scriptFields = Collections.emptyList();
            // tag::put-datafeed-config-set-script-fields
            datafeedBuilder.setScriptFields(scriptFields); // <1>
            // end::put-datafeed-config-set-script-fields

            // tag::put-datafeed-config-set-scroll-size
            datafeedBuilder.setScrollSize(1000); // <1>
            // end::put-datafeed-config-set-scroll-size

            // tag::put-datafeed-request
            PutDatafeedRequest request = new PutDatafeedRequest(datafeedBuilder.build()); // <1>
            // end::put-datafeed-request

            // tag::put-datafeed-execute
            PutDatafeedResponse response = client.machineLearning().putDatafeed(request, RequestOptions.DEFAULT);
            // end::put-datafeed-execute

            // tag::put-datafeed-response
            DatafeedConfig datafeed = response.getResponse(); // <1>
            // end::put-datafeed-response
            assertThat(datafeed.getId(), equalTo("datafeed-1"));
        }
        {
            // We need to create a job for the datafeed request to be valid
            String jobId = "put-datafeed-job-2";
            Job job = MachineLearningIT.buildJob(jobId);
            client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

            String id = "datafeed-2";

            DatafeedConfig datafeed = new DatafeedConfig.Builder(id, jobId).setIndices("index_1", "index_2").build();

            PutDatafeedRequest request = new PutDatafeedRequest(datafeed);
            // tag::put-datafeed-execute-listener
            ActionListener<PutDatafeedResponse> listener = new ActionListener<PutDatafeedResponse>() {
                @Override
                public void onResponse(PutDatafeedResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::put-datafeed-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::put-datafeed-execute-async
            client.machineLearning().putDatafeedAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::put-datafeed-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testGetDatafeed() throws Exception {
        RestHighLevelClient client = highLevelClient();

        Job job = MachineLearningIT.buildJob("get-datafeed-job");
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);
        String datafeedId = job.getId() + "-feed";
        DatafeedConfig datafeed = DatafeedConfig.builder(datafeedId, job.getId()).setIndices("foo").build();
        client.machineLearning().putDatafeed(new PutDatafeedRequest(datafeed), RequestOptions.DEFAULT);

        {
            // tag::get-datafeed-request
            GetDatafeedRequest request = new GetDatafeedRequest(datafeedId); // <1>
            request.setAllowNoDatafeeds(true); // <2>
            // end::get-datafeed-request

            // tag::get-datafeed-execute
            GetDatafeedResponse response = client.machineLearning().getDatafeed(request, RequestOptions.DEFAULT);
            // end::get-datafeed-execute
            
            // tag::get-datafeed-response
            long numberOfDatafeeds = response.count(); // <1>
            List<DatafeedConfig> datafeeds = response.datafeeds(); // <2>
            // end::get-datafeed-response

            assertEquals(1, numberOfDatafeeds);
            assertEquals(1, datafeeds.size());
        }
        {
            GetDatafeedRequest request = new GetDatafeedRequest(datafeedId);

            // tag::get-datafeed-execute-listener
            ActionListener<GetDatafeedResponse> listener = new ActionListener<GetDatafeedResponse>() {
                @Override
                public void onResponse(GetDatafeedResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::get-datafeed-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::get-datafeed-execute-async
            client.machineLearning().getDatafeedAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::get-datafeed-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testDeleteDatafeed() throws Exception {
        RestHighLevelClient client = highLevelClient();

        String jobId = "test-delete-datafeed-job";
        Job job = MachineLearningIT.buildJob(jobId);
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

        String datafeedId = "test-delete-datafeed";
        DatafeedConfig datafeed = DatafeedConfig.builder(datafeedId, jobId).setIndices("foo").build();
        client.machineLearning().putDatafeed(new PutDatafeedRequest(datafeed), RequestOptions.DEFAULT);

        {
            // tag::delete-datafeed-request
            DeleteDatafeedRequest deleteDatafeedRequest = new DeleteDatafeedRequest(datafeedId);
            deleteDatafeedRequest.setForce(false); // <1>
            // end::delete-datafeed-request

            // tag::delete-datafeed-execute
            AcknowledgedResponse deleteDatafeedResponse = client.machineLearning().deleteDatafeed(
                deleteDatafeedRequest, RequestOptions.DEFAULT);
            // end::delete-datafeed-execute
            
            // tag::delete-datafeed-response
            boolean isAcknowledged = deleteDatafeedResponse.isAcknowledged(); // <1>
            // end::delete-datafeed-response
        }

        // Recreate datafeed to allow second deletion
        client.machineLearning().putDatafeed(new PutDatafeedRequest(datafeed), RequestOptions.DEFAULT);

        {
            // tag::delete-datafeed-execute-listener
            ActionListener<AcknowledgedResponse> listener = new ActionListener<AcknowledgedResponse>() {
                @Override
                public void onResponse(AcknowledgedResponse acknowledgedResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::delete-datafeed-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            DeleteDatafeedRequest deleteDatafeedRequest = new DeleteDatafeedRequest(datafeedId);

            // tag::delete-datafeed-execute-async
            client.machineLearning().deleteDatafeedAsync(deleteDatafeedRequest, RequestOptions.DEFAULT, listener); // <1>
            // end::delete-datafeed-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testPreviewDatafeed() throws Exception {
        RestHighLevelClient client = highLevelClient();

        Job job = MachineLearningIT.buildJob("preview-datafeed-job");
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);
        String datafeedId = job.getId() + "-feed";
        String indexName = "preview_data_2";
        CreateIndexRequest createIndexRequest = new CreateIndexRequest(indexName);
        createIndexRequest.mapping("doc", "timestamp", "type=date", "total", "type=long");
        highLevelClient().indices().create(createIndexRequest, RequestOptions.DEFAULT);
        DatafeedConfig datafeed = DatafeedConfig.builder(datafeedId, job.getId())
            .setTypes(Arrays.asList("doc"))
            .setIndices(indexName)
            .build();
        client.machineLearning().putDatafeed(new PutDatafeedRequest(datafeed), RequestOptions.DEFAULT);
        {
            // tag::preview-datafeed-request
            PreviewDatafeedRequest request = new PreviewDatafeedRequest(datafeedId); // <1>
            // end::preview-datafeed-request

            // tag::preview-datafeed-execute
            PreviewDatafeedResponse response = client.machineLearning().previewDatafeed(request, RequestOptions.DEFAULT);
            // end::preview-datafeed-execute

            // tag::preview-datafeed-response
            BytesReference rawPreview = response.getPreview(); // <1>
            List<Map<String, Object>> semiParsedPreview = response.getDataList(); // <2>
            // end::preview-datafeed-response

            assertTrue(semiParsedPreview.isEmpty());
        }
        {
            PreviewDatafeedRequest request = new PreviewDatafeedRequest(datafeedId);

            // tag::preview-datafeed-execute-listener
            ActionListener<PreviewDatafeedResponse> listener = new ActionListener<PreviewDatafeedResponse>() {
                @Override
                public void onResponse(PreviewDatafeedResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::preview-datafeed-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::preview-datafeed-execute-async
            client.machineLearning().previewDatafeedAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::preview-datafeed-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testStartDatafeed() throws Exception {
        RestHighLevelClient client = highLevelClient();

        Job job = MachineLearningIT.buildJob("start-datafeed-job");
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);
        String datafeedId = job.getId() + "-feed";
        String indexName = "start_data_2";
        CreateIndexRequest createIndexRequest = new CreateIndexRequest(indexName);
        createIndexRequest.mapping("doc", "timestamp", "type=date", "total", "type=long");
        highLevelClient().indices().create(createIndexRequest, RequestOptions.DEFAULT);
        DatafeedConfig datafeed = DatafeedConfig.builder(datafeedId, job.getId())
            .setTypes(Arrays.asList("doc"))
            .setIndices(indexName)
            .build();
        client.machineLearning().putDatafeed(new PutDatafeedRequest(datafeed), RequestOptions.DEFAULT);
        client.machineLearning().openJob(new OpenJobRequest(job.getId()), RequestOptions.DEFAULT);
        {
            // tag::start-datafeed-request
            StartDatafeedRequest request = new StartDatafeedRequest(datafeedId); // <1>
            // end::start-datafeed-request

            // tag::start-datafeed-request-options
            request.setEnd("2018-08-21T00:00:00Z"); // <1>
            request.setStart("2018-08-20T00:00:00Z"); // <2>
            request.setTimeout(TimeValue.timeValueMinutes(10)); // <3>
            // end::start-datafeed-request-options

            // tag::start-datafeed-execute
            StartDatafeedResponse response = client.machineLearning().startDatafeed(request, RequestOptions.DEFAULT);
            // end::start-datafeed-execute
            // tag::start-datafeed-response
            boolean started = response.isStarted(); // <1>
            // end::start-datafeed-response

            assertTrue(started);
        }
        {
            StartDatafeedRequest request = new StartDatafeedRequest(datafeedId);

            // tag::start-datafeed-execute-listener
            ActionListener<StartDatafeedResponse> listener = new ActionListener<StartDatafeedResponse>() {
                @Override
                public void onResponse(StartDatafeedResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::start-datafeed-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::start-datafeed-execute-async
            client.machineLearning().startDatafeedAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::start-datafeed-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testStopDatafeed() throws Exception {
        RestHighLevelClient client = highLevelClient();

        {
            // tag::stop-datafeed-request
            StopDatafeedRequest request = new StopDatafeedRequest("datafeed_id1", "datafeed_id*"); // <1>
            // end::stop-datafeed-request
            request = StopDatafeedRequest.stopAllDatafeedsRequest();

            // tag::stop-datafeed-request-options
            request.setAllowNoDatafeeds(true); // <1>
            request.setForce(true); // <2>
            request.setTimeout(TimeValue.timeValueMinutes(10)); // <3>
            // end::stop-datafeed-request-options

            // tag::stop-datafeed-execute
            StopDatafeedResponse response = client.machineLearning().stopDatafeed(request, RequestOptions.DEFAULT);
            // end::stop-datafeed-execute
            // tag::stop-datafeed-response
            boolean stopped = response.isStopped(); // <1>
            // end::stop-datafeed-response

            assertTrue(stopped);
        }
        {
            StopDatafeedRequest request = StopDatafeedRequest.stopAllDatafeedsRequest();

            // tag::stop-datafeed-execute-listener
            ActionListener<StopDatafeedResponse> listener = new ActionListener<StopDatafeedResponse>() {
                @Override
                public void onResponse(StopDatafeedResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::stop-datafeed-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::stop-datafeed-execute-async
            client.machineLearning().stopDatafeedAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::stop-datafeed-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testGetDatafeedStats() throws Exception {
        RestHighLevelClient client = highLevelClient();

        Job job = MachineLearningIT.buildJob("get-machine-learning-datafeed-stats1");
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

        Job secondJob = MachineLearningIT.buildJob("get-machine-learning-datafeed-stats2");
        client.machineLearning().putJob(new PutJobRequest(secondJob), RequestOptions.DEFAULT);
        String datafeedId1 = job.getId() + "-feed";
        String indexName = "datafeed_stats_data_2";
        CreateIndexRequest createIndexRequest = new CreateIndexRequest(indexName);
        createIndexRequest.mapping("doc", "timestamp", "type=date", "total", "type=long");
        highLevelClient().indices().create(createIndexRequest, RequestOptions.DEFAULT);
        DatafeedConfig datafeed = DatafeedConfig.builder(datafeedId1, job.getId())
            .setTypes(Arrays.asList("doc"))
            .setIndices(indexName)
            .build();
        client.machineLearning().putDatafeed(new PutDatafeedRequest(datafeed), RequestOptions.DEFAULT);

        String datafeedId2 = secondJob.getId() + "-feed";
        DatafeedConfig secondDatafeed = DatafeedConfig.builder(datafeedId2, secondJob.getId())
            .setTypes(Arrays.asList("doc"))
            .setIndices(indexName)
            .build();
        client.machineLearning().putDatafeed(new PutDatafeedRequest(secondDatafeed), RequestOptions.DEFAULT);

        {
            //tag::get-datafeed-stats-request
            GetDatafeedStatsRequest request =
                new GetDatafeedStatsRequest("get-machine-learning-datafeed-stats1-feed", "get-machine-learning-datafeed*"); // <1>
            request.setAllowNoDatafeeds(true); // <2>
            //end::get-datafeed-stats-request

            //tag::get-datafeed-stats-execute
            GetDatafeedStatsResponse response = client.machineLearning().getDatafeedStats(request, RequestOptions.DEFAULT);
            //end::get-datafeed-stats-execute

            //tag::get-datafeed-stats-response
            long numberOfDatafeedStats = response.count(); // <1>
            List<DatafeedStats> datafeedStats = response.datafeedStats(); // <2>
            //end::get-datafeed-stats-response

            assertEquals(2, response.count());
            assertThat(response.datafeedStats(), hasSize(2));
            assertThat(response.datafeedStats().stream().map(DatafeedStats::getDatafeedId).collect(Collectors.toList()),
                containsInAnyOrder(datafeed.getId(), secondDatafeed.getId()));
        }
        {
            GetDatafeedStatsRequest request = new GetDatafeedStatsRequest("*");

            // tag::get-datafeed-stats-execute-listener
            ActionListener<GetDatafeedStatsResponse> listener = new ActionListener<GetDatafeedStatsResponse>() {
                @Override
                public void onResponse(GetDatafeedStatsResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::get-datafeed-stats-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::get-datafeed-stats-execute-async
            client.machineLearning().getDatafeedStatsAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::get-datafeed-stats-execute-async

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
            // tag::get-buckets-request
            GetBucketsRequest request = new GetBucketsRequest(jobId); // <1>
            // end::get-buckets-request

            // tag::get-buckets-timestamp
            request.setTimestamp("2018-08-17T00:00:00Z"); // <1>
            // end::get-buckets-timestamp

            // Set timestamp to null as it is incompatible with other args
            request.setTimestamp(null);

            // tag::get-buckets-anomaly-score
            request.setAnomalyScore(75.0); // <1>
            // end::get-buckets-anomaly-score

            // tag::get-buckets-desc
            request.setDescending(true); // <1>
            // end::get-buckets-desc

            // tag::get-buckets-end
            request.setEnd("2018-08-21T00:00:00Z"); // <1>
            // end::get-buckets-end

            // tag::get-buckets-exclude-interim
            request.setExcludeInterim(true); // <1>
            // end::get-buckets-exclude-interim

            // tag::get-buckets-expand
            request.setExpand(true); // <1>
            // end::get-buckets-expand

            // tag::get-buckets-page
            request.setPageParams(new PageParams(100, 200)); // <1>
            // end::get-buckets-page

            // Set page params back to null so the response contains the bucket we indexed
            request.setPageParams(null);

            // tag::get-buckets-sort
            request.setSort("anomaly_score"); // <1>
            // end::get-buckets-sort

            // tag::get-buckets-start
            request.setStart("2018-08-01T00:00:00Z"); // <1>
            // end::get-buckets-start

            // tag::get-buckets-execute
            GetBucketsResponse response = client.machineLearning().getBuckets(request, RequestOptions.DEFAULT);
            // end::get-buckets-execute

            // tag::get-buckets-response
            long count = response.count(); // <1>
            List<Bucket> buckets = response.buckets(); // <2>
            // end::get-buckets-response
            assertEquals(1, buckets.size());
        }
        {
            GetBucketsRequest request = new GetBucketsRequest(jobId);

            // tag::get-buckets-execute-listener
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
            // end::get-buckets-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::get-buckets-execute-async
            client.machineLearning().getBucketsAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::get-buckets-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testFlushJob() throws Exception {
        RestHighLevelClient client = highLevelClient();

        Job job = MachineLearningIT.buildJob("flushing-my-first-machine-learning-job");
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);
        client.machineLearning().openJob(new OpenJobRequest(job.getId()), RequestOptions.DEFAULT);

        Job secondJob = MachineLearningIT.buildJob("flushing-my-second-machine-learning-job");
        client.machineLearning().putJob(new PutJobRequest(secondJob), RequestOptions.DEFAULT);
        client.machineLearning().openJob(new OpenJobRequest(secondJob.getId()), RequestOptions.DEFAULT);

        {
            // tag::flush-job-request
            FlushJobRequest flushJobRequest = new FlushJobRequest("flushing-my-first-machine-learning-job"); // <1>
            // end::flush-job-request

            // tag::flush-job-request-options
            flushJobRequest.setCalcInterim(true); // <1>
            flushJobRequest.setAdvanceTime("2018-08-31T16:35:07+00:00"); // <2>
            flushJobRequest.setStart("2018-08-31T16:35:17+00:00"); // <3>
            flushJobRequest.setEnd("2018-08-31T16:35:27+00:00"); // <4>
            flushJobRequest.setSkipTime("2018-08-31T16:35:00+00:00"); // <5>
            // end::flush-job-request-options

            // tag::flush-job-execute
            FlushJobResponse flushJobResponse = client.machineLearning().flushJob(flushJobRequest, RequestOptions.DEFAULT);
            // end::flush-job-execute

            // tag::flush-job-response
            boolean isFlushed = flushJobResponse.isFlushed(); // <1>
            Date lastFinalizedBucketEnd = flushJobResponse.getLastFinalizedBucketEnd(); // <2>
            // end::flush-job-response

        }
        {
            // tag::flush-job-execute-listener
            ActionListener<FlushJobResponse> listener = new ActionListener<FlushJobResponse>() {
                @Override
                public void onResponse(FlushJobResponse FlushJobResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::flush-job-execute-listener
            FlushJobRequest flushJobRequest = new FlushJobRequest("flushing-my-second-machine-learning-job");

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::flush-job-execute-async
            client.machineLearning().flushJobAsync(flushJobRequest, RequestOptions.DEFAULT, listener); // <1>
            // end::flush-job-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }
    
    public void testDeleteForecast() throws Exception {
        RestHighLevelClient client = highLevelClient();

        Job job = MachineLearningIT.buildJob("deleting-forecast-for-job");
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);
        client.machineLearning().openJob(new OpenJobRequest(job.getId()), RequestOptions.DEFAULT);
        PostDataRequest.JsonBuilder builder = new PostDataRequest.JsonBuilder();
        for(int i = 0; i < 30; i++) {
            Map<String, Object> hashMap = new HashMap<>();
            hashMap.put("total", randomInt(1000));
            hashMap.put("timestamp", (i+1)*1000);
            builder.addDoc(hashMap);
        }

        PostDataRequest postDataRequest = new PostDataRequest(job.getId(), builder);
        client.machineLearning().postData(postDataRequest, RequestOptions.DEFAULT);
        client.machineLearning().flushJob(new FlushJobRequest(job.getId()), RequestOptions.DEFAULT);

        ForecastJobResponse forecastJobResponse = client.machineLearning().
            forecastJob(new ForecastJobRequest(job.getId()), RequestOptions.DEFAULT);
        String forecastId = forecastJobResponse.getForecastId();

        GetRequest request = new GetRequest(".ml-anomalies-" + job.getId());
        request.id(job.getId() + "_model_forecast_request_stats_" + forecastId);
        assertBusy(() -> {
            GetResponse getResponse = highLevelClient().get(request, RequestOptions.DEFAULT);
            assertTrue(getResponse.isExists());
            assertTrue(getResponse.getSourceAsString().contains("finished"));
        }, 30, TimeUnit.SECONDS);

        {
            // tag::delete-forecast-request
            DeleteForecastRequest deleteForecastRequest = new DeleteForecastRequest("deleting-forecast-for-job"); // <1>
            // end::delete-forecast-request

            // tag::delete-forecast-request-options
            deleteForecastRequest.setForecastIds(forecastId); // <1>
            deleteForecastRequest.timeout("30s"); // <2>
            deleteForecastRequest.setAllowNoForecasts(true); // <3>
            // end::delete-forecast-request-options

            // tag::delete-forecast-execute
            AcknowledgedResponse deleteForecastResponse = client.machineLearning().deleteForecast(deleteForecastRequest,
                RequestOptions.DEFAULT);
            // end::delete-forecast-execute

            // tag::delete-forecast-response
            boolean isAcknowledged = deleteForecastResponse.isAcknowledged(); // <1>
            // end::delete-forecast-response
        }
        {
            // tag::delete-forecast-execute-listener
            ActionListener<AcknowledgedResponse> listener = new ActionListener<AcknowledgedResponse>() {
                @Override
                public void onResponse(AcknowledgedResponse DeleteForecastResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::delete-forecast-execute-listener
            DeleteForecastRequest deleteForecastRequest = DeleteForecastRequest.deleteAllForecasts(job.getId());
            deleteForecastRequest.setAllowNoForecasts(true);

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::delete-forecast-execute-async
            client.machineLearning().deleteForecastAsync(deleteForecastRequest, RequestOptions.DEFAULT, listener); // <1>
            // end::delete-forecast-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }
    
    public void testGetJobStats() throws Exception {
        RestHighLevelClient client = highLevelClient();

        Job job = MachineLearningIT.buildJob("get-machine-learning-job-stats1");
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

        Job secondJob = MachineLearningIT.buildJob("get-machine-learning-job-stats2");
        client.machineLearning().putJob(new PutJobRequest(secondJob), RequestOptions.DEFAULT);

        {
            // tag::get-job-stats-request
            GetJobStatsRequest request = new GetJobStatsRequest("get-machine-learning-job-stats1", "get-machine-learning-job-*"); // <1>
            request.setAllowNoJobs(true); // <2>
            // end::get-job-stats-request

            // tag::get-job-stats-execute
            GetJobStatsResponse response = client.machineLearning().getJobStats(request, RequestOptions.DEFAULT);
            // end::get-job-stats-execute

            // tag::get-job-stats-response
            long numberOfJobStats = response.count(); // <1>
            List<JobStats> jobStats = response.jobStats(); // <2>
            // end::get-job-stats-response

            assertEquals(2, response.count());
            assertThat(response.jobStats(), hasSize(2));
            assertThat(response.jobStats().stream().map(JobStats::getJobId).collect(Collectors.toList()),
                containsInAnyOrder(job.getId(), secondJob.getId()));
        }
        {
            GetJobStatsRequest request = new GetJobStatsRequest("get-machine-learning-job-stats1", "get-machine-learning-job-*");

            // tag::get-job-stats-execute-listener
            ActionListener<GetJobStatsResponse> listener = new ActionListener<GetJobStatsResponse>() {
                @Override
                public void onResponse(GetJobStatsResponse response) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::get-job-stats-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::get-job-stats-execute-async
            client.machineLearning().getJobStatsAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::get-job-stats-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testForecastJob() throws Exception {
        RestHighLevelClient client = highLevelClient();

        Job job = MachineLearningIT.buildJob("forecasting-my-first-machine-learning-job");
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);
        client.machineLearning().openJob(new OpenJobRequest(job.getId()), RequestOptions.DEFAULT);

        PostDataRequest.JsonBuilder builder = new PostDataRequest.JsonBuilder();
        for(int i = 0; i < 30; i++) {
            Map<String, Object> hashMap = new HashMap<>();
            hashMap.put("total", randomInt(1000));
            hashMap.put("timestamp", (i+1)*1000);
            builder.addDoc(hashMap);
        }
        PostDataRequest postDataRequest = new PostDataRequest(job.getId(), builder);
        client.machineLearning().postData(postDataRequest, RequestOptions.DEFAULT);
        client.machineLearning().flushJob(new FlushJobRequest(job.getId()), RequestOptions.DEFAULT);

        {
            // tag::forecast-job-request
            ForecastJobRequest forecastJobRequest = new ForecastJobRequest("forecasting-my-first-machine-learning-job"); // <1>
            // end::forecast-job-request

            // tag::forecast-job-request-options
            forecastJobRequest.setExpiresIn(TimeValue.timeValueHours(48)); // <1>
            forecastJobRequest.setDuration(TimeValue.timeValueHours(24)); // <2>
            // end::forecast-job-request-options

            // tag::forecast-job-execute
            ForecastJobResponse forecastJobResponse = client.machineLearning().forecastJob(forecastJobRequest, RequestOptions.DEFAULT);
            // end::forecast-job-execute

            // tag::forecast-job-response
            boolean isAcknowledged = forecastJobResponse.isAcknowledged(); // <1>
            String forecastId = forecastJobResponse.getForecastId(); // <2>
            // end::forecast-job-response
            assertTrue(isAcknowledged);
            assertNotNull(forecastId);
        }
        {
            // tag::forecast-job-execute-listener
            ActionListener<ForecastJobResponse> listener = new ActionListener<ForecastJobResponse>() {
                @Override
                public void onResponse(ForecastJobResponse forecastJobResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::forecast-job-execute-listener
            ForecastJobRequest forecastJobRequest = new ForecastJobRequest("forecasting-my-first-machine-learning-job");

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::forecast-job-execute-async
            client.machineLearning().forecastJobAsync(forecastJobRequest, RequestOptions.DEFAULT, listener); // <1>
            // end::forecast-job-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }
    
    public void testGetOverallBuckets() throws IOException, InterruptedException {
        RestHighLevelClient client = highLevelClient();

        String jobId1 = "test-get-overall-buckets-1";
        String jobId2 = "test-get-overall-buckets-2";
        Job job1 = MachineLearningGetResultsIT.buildJob(jobId1);
        Job job2 = MachineLearningGetResultsIT.buildJob(jobId2);
        client.machineLearning().putJob(new PutJobRequest(job1), RequestOptions.DEFAULT);
        client.machineLearning().putJob(new PutJobRequest(job2), RequestOptions.DEFAULT);

        // Let us index some buckets
        BulkRequest bulkRequest = new BulkRequest();
        bulkRequest.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);

        {
            IndexRequest indexRequest = new IndexRequest(".ml-anomalies-shared", "doc");
            indexRequest.source("{\"job_id\":\"test-get-overall-buckets-1\", \"result_type\":\"bucket\", \"timestamp\": 1533081600000," +
                    "\"bucket_span\": 600,\"is_interim\": false, \"anomaly_score\": 60.0}", XContentType.JSON);
            bulkRequest.add(indexRequest);
        }
        {
            IndexRequest indexRequest = new IndexRequest(".ml-anomalies-shared", "doc");
            indexRequest.source("{\"job_id\":\"test-get-overall-buckets-2\", \"result_type\":\"bucket\", \"timestamp\": 1533081600000," +
                    "\"bucket_span\": 3600,\"is_interim\": false, \"anomaly_score\": 100.0}", XContentType.JSON);
            bulkRequest.add(indexRequest);
        }

        client.bulk(bulkRequest, RequestOptions.DEFAULT);

        {
            // tag::get-overall-buckets-request
            GetOverallBucketsRequest request = new GetOverallBucketsRequest(jobId1, jobId2); // <1>
            // end::get-overall-buckets-request

            // tag::get-overall-buckets-bucket-span
            request.setBucketSpan(TimeValue.timeValueHours(24)); // <1>
            // end::get-overall-buckets-bucket-span

            // tag::get-overall-buckets-end
            request.setEnd("2018-08-21T00:00:00Z"); // <1>
            // end::get-overall-buckets-end

            // tag::get-overall-buckets-exclude-interim
            request.setExcludeInterim(true); // <1>
            // end::get-overall-buckets-exclude-interim

            // tag::get-overall-buckets-overall-score
            request.setOverallScore(75.0); // <1>
            // end::get-overall-buckets-overall-score

            // tag::get-overall-buckets-start
            request.setStart("2018-08-01T00:00:00Z"); // <1>
            // end::get-overall-buckets-start

            // tag::get-overall-buckets-top-n
            request.setTopN(2); // <1>
            // end::get-overall-buckets-top-n

            // tag::get-overall-buckets-execute
            GetOverallBucketsResponse response = client.machineLearning().getOverallBuckets(request, RequestOptions.DEFAULT);
            // end::get-overall-buckets-execute

            // tag::get-overall-buckets-response
            long count = response.count(); // <1>
            List<OverallBucket> overallBuckets = response.overallBuckets(); // <2>
            // end::get-overall-buckets-response

            assertEquals(1, overallBuckets.size());
            assertThat(overallBuckets.get(0).getOverallScore(), is(closeTo(80.0, 0.001)));

        }
        {
            GetOverallBucketsRequest request = new GetOverallBucketsRequest(jobId1, jobId2);

            // tag::get-overall-buckets-execute-listener
            ActionListener<GetOverallBucketsResponse> listener =
                    new ActionListener<GetOverallBucketsResponse>() {
                        @Override
                        public void onResponse(GetOverallBucketsResponse getOverallBucketsResponse) {
                            // <1>
                        }

                        @Override
                        public void onFailure(Exception e) {
                            // <2>
                        }
                    };
            // end::get-overall-buckets-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::get-overall-buckets-execute-async
            client.machineLearning().getOverallBucketsAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::get-overall-buckets-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testGetRecords() throws IOException, InterruptedException {
        RestHighLevelClient client = highLevelClient();

        String jobId = "test-get-records";
        Job job = MachineLearningIT.buildJob(jobId);
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

        // Let us index a record
        IndexRequest indexRequest = new IndexRequest(".ml-anomalies-shared", "doc");
        indexRequest.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);
        indexRequest.source("{\"job_id\":\"test-get-records\", \"result_type\":\"record\", \"timestamp\": 1533081600000," +
                "\"bucket_span\": 600,\"is_interim\": false, \"record_score\": 80.0}", XContentType.JSON);
        client.index(indexRequest, RequestOptions.DEFAULT);

        {
            // tag::get-records-request
            GetRecordsRequest request = new GetRecordsRequest(jobId); // <1>
            // end::get-records-request

            // tag::get-records-desc
            request.setDescending(true); // <1>
            // end::get-records-desc

            // tag::get-records-end
            request.setEnd("2018-08-21T00:00:00Z"); // <1>
            // end::get-records-end

            // tag::get-records-exclude-interim
            request.setExcludeInterim(true); // <1>
            // end::get-records-exclude-interim

            // tag::get-records-page
            request.setPageParams(new PageParams(100, 200)); // <1>
            // end::get-records-page

            // Set page params back to null so the response contains the record we indexed
            request.setPageParams(null);

            // tag::get-records-record-score
            request.setRecordScore(75.0); // <1>
            // end::get-records-record-score

            // tag::get-records-sort
            request.setSort("probability"); // <1>
            // end::get-records-sort

            // tag::get-records-start
            request.setStart("2018-08-01T00:00:00Z"); // <1>
            // end::get-records-start

            // tag::get-records-execute
            GetRecordsResponse response = client.machineLearning().getRecords(request, RequestOptions.DEFAULT);
            // end::get-records-execute

            // tag::get-records-response
            long count = response.count(); // <1>
            List<AnomalyRecord> records = response.records(); // <2>
            // end::get-records-response
            assertEquals(1, records.size());
        }
        {
            GetRecordsRequest request = new GetRecordsRequest(jobId);

            // tag::get-records-execute-listener
            ActionListener<GetRecordsResponse> listener =
                    new ActionListener<GetRecordsResponse>() {
                        @Override
                        public void onResponse(GetRecordsResponse getRecordsResponse) {
                            // <1>
                        }

                        @Override
                        public void onFailure(Exception e) {
                            // <2>
                        }
                    };
            // end::get-records-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::get-records-execute-async
            client.machineLearning().getRecordsAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::get-records-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testPostData() throws Exception {
        RestHighLevelClient client = highLevelClient();

        Job job = MachineLearningIT.buildJob("test-post-data");
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);
        client.machineLearning().openJob(new OpenJobRequest(job.getId()), RequestOptions.DEFAULT);

        {
            // tag::post-data-request
            PostDataRequest.JsonBuilder jsonBuilder = new PostDataRequest.JsonBuilder(); // <1>
            Map<String, Object> mapData = new HashMap<>();
            mapData.put("total", 109);
            jsonBuilder.addDoc(mapData); // <2>
            jsonBuilder.addDoc("{\"total\":1000}"); // <3>
            PostDataRequest postDataRequest = new PostDataRequest("test-post-data", jsonBuilder); // <4>
            // end::post-data-request


            // tag::post-data-request-options
            postDataRequest.setResetStart("2018-08-31T16:35:07+00:00"); // <1>
            postDataRequest.setResetEnd("2018-08-31T16:35:17+00:00"); // <2>
            // end::post-data-request-options
            postDataRequest.setResetEnd(null);
            postDataRequest.setResetStart(null);

            // tag::post-data-execute
            PostDataResponse postDataResponse = client.machineLearning().postData(postDataRequest, RequestOptions.DEFAULT);
            // end::post-data-execute

            // tag::post-data-response
            DataCounts dataCounts = postDataResponse.getDataCounts(); // <1>
            // end::post-data-response
            assertEquals(2, dataCounts.getInputRecordCount());

        }
        {
            // tag::post-data-execute-listener
            ActionListener<PostDataResponse> listener = new ActionListener<PostDataResponse>() {
                @Override
                public void onResponse(PostDataResponse postDataResponse) {
                    // <1>
                }

                @Override
                public void onFailure(Exception e) {
                    // <2>
                }
            };
            // end::post-data-execute-listener
            PostDataRequest.JsonBuilder jsonBuilder = new PostDataRequest.JsonBuilder();
            Map<String, Object> mapData = new HashMap<>();
            mapData.put("total", 109);
            jsonBuilder.addDoc(mapData);
            PostDataRequest postDataRequest = new PostDataRequest("test-post-data", jsonBuilder); // <1>

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::post-data-execute-async
            client.machineLearning().postDataAsync(postDataRequest, RequestOptions.DEFAULT, listener); // <1>
            // end::post-data-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testGetInfluencers() throws IOException, InterruptedException {
        RestHighLevelClient client = highLevelClient();

        String jobId = "test-get-influencers";
        Job job = MachineLearningIT.buildJob(jobId);
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

        // Let us index a record
        IndexRequest indexRequest = new IndexRequest(".ml-anomalies-shared", "doc");
        indexRequest.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);
        indexRequest.source("{\"job_id\":\"test-get-influencers\", \"result_type\":\"influencer\", \"timestamp\": 1533081600000," +
                "\"bucket_span\": 600,\"is_interim\": false, \"influencer_score\": 80.0, \"influencer_field_name\": \"my_influencer\"," +
                "\"influencer_field_value\":\"foo\"}", XContentType.JSON);
        client.index(indexRequest, RequestOptions.DEFAULT);

        {
            // tag::get-influencers-request
            GetInfluencersRequest request = new GetInfluencersRequest(jobId); // <1>
            // end::get-influencers-request

            // tag::get-influencers-desc
            request.setDescending(true); // <1>
            // end::get-influencers-desc

            // tag::get-influencers-end
            request.setEnd("2018-08-21T00:00:00Z"); // <1>
            // end::get-influencers-end

            // tag::get-influencers-exclude-interim
            request.setExcludeInterim(true); // <1>
            // end::get-influencers-exclude-interim

            // tag::get-influencers-influencer-score
            request.setInfluencerScore(75.0); // <1>
            // end::get-influencers-influencer-score

            // tag::get-influencers-page
            request.setPageParams(new PageParams(100, 200)); // <1>
            // end::get-influencers-page

            // Set page params back to null so the response contains the influencer we indexed
            request.setPageParams(null);

            // tag::get-influencers-sort
            request.setSort("probability"); // <1>
            // end::get-influencers-sort

            // tag::get-influencers-start
            request.setStart("2018-08-01T00:00:00Z"); // <1>
            // end::get-influencers-start

            // tag::get-influencers-execute
            GetInfluencersResponse response = client.machineLearning().getInfluencers(request, RequestOptions.DEFAULT);
            // end::get-influencers-execute

            // tag::get-influencers-response
            long count = response.count(); // <1>
            List<Influencer> influencers = response.influencers(); // <2>
            // end::get-influencers-response
            assertEquals(1, influencers.size());
        }
        {
            GetInfluencersRequest request = new GetInfluencersRequest(jobId);

            // tag::get-influencers-execute-listener
            ActionListener<GetInfluencersResponse> listener =
                    new ActionListener<GetInfluencersResponse>() {
                        @Override
                        public void onResponse(GetInfluencersResponse getInfluencersResponse) {
                            // <1>
                        }

                        @Override
                        public void onFailure(Exception e) {
                            // <2>
                        }
                    };
            // end::get-influencers-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::get-influencers-execute-async
            client.machineLearning().getInfluencersAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::get-influencers-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testGetCategories() throws IOException, InterruptedException {
        RestHighLevelClient client = highLevelClient();

        String jobId = "test-get-categories";
        Job job = MachineLearningIT.buildJob(jobId);
        client.machineLearning().putJob(new PutJobRequest(job), RequestOptions.DEFAULT);

        // Let us index a category
        IndexRequest indexRequest = new IndexRequest(".ml-anomalies-shared", "doc");
        indexRequest.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);
        indexRequest.source("{\"job_id\": \"test-get-categories\", \"category_id\": 1, \"terms\": \"AAL\"," +
                " \"regex\": \".*?AAL.*\", \"max_matching_length\": 3, \"examples\": [\"AAL\"]}", XContentType.JSON);
        client.index(indexRequest, RequestOptions.DEFAULT);

        {
            // tag::get-categories-request
            GetCategoriesRequest request = new GetCategoriesRequest(jobId); // <1>
            // end::get-categories-request

            // tag::get-categories-category-id
            request.setCategoryId(1L); // <1>
            // end::get-categories-category-id

            // tag::get-categories-page
            request.setPageParams(new PageParams(100, 200)); // <1>
            // end::get-categories-page

            // Set page params back to null so the response contains the category we indexed
            request.setPageParams(null);

            // tag::get-categories-execute
            GetCategoriesResponse response = client.machineLearning().getCategories(request, RequestOptions.DEFAULT);
            // end::get-categories-execute

            // tag::get-categories-response
            long count = response.count(); // <1>
            List<CategoryDefinition> categories = response.categories(); // <2>
            // end::get-categories-response
            assertEquals(1, categories.size());
        }
        {
            GetCategoriesRequest request = new GetCategoriesRequest(jobId);

            // tag::get-categories-execute-listener
            ActionListener<GetCategoriesResponse> listener =
                    new ActionListener<GetCategoriesResponse>() {
                        @Override
                        public void onResponse(GetCategoriesResponse getcategoriesResponse) {
                            // <1>
                        }

                        @Override
                        public void onFailure(Exception e) {
                            // <2>
                        }
                    };
            // end::get-categories-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::get-categories-execute-async
            client.machineLearning().getCategoriesAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::get-categories-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testPutCalendar() throws IOException, InterruptedException {
        RestHighLevelClient client = highLevelClient();

        // tag::put-calendar-request
        Calendar calendar = new Calendar("public_holidays", Collections.singletonList("job_1"), "A calendar for public holidays");
        PutCalendarRequest request = new PutCalendarRequest(calendar); // <1>
        // end::put-calendar-request

        // tag::put-calendar-execute
        PutCalendarResponse response = client.machineLearning().putCalendar(request, RequestOptions.DEFAULT);
        // end::put-calendar-execute

        // tag::put-calendar-response
        Calendar newCalendar = response.getCalendar(); // <1>
        // end::put-calendar-response
        assertThat(newCalendar.getId(), equalTo("public_holidays"));

        // tag::put-calendar-execute-listener
        ActionListener<PutCalendarResponse> listener = new ActionListener<PutCalendarResponse>() {
            @Override
            public void onResponse(PutCalendarResponse response) {
                // <1>
            }

            @Override
            public void onFailure(Exception e) {
                // <2>
            }
        };
        // end::put-calendar-execute-listener

        // Replace the empty listener by a blocking listener in test
        final CountDownLatch latch = new CountDownLatch(1);
        listener = new LatchedActionListener<>(listener, latch);

        // tag::put-calendar-execute-async
        client.machineLearning().putCalendarAsync(request, RequestOptions.DEFAULT, listener); // <1>
        // end::put-calendar-execute-async

        assertTrue(latch.await(30L, TimeUnit.SECONDS));
    }

    public void testGetCalendar() throws IOException, InterruptedException {
        RestHighLevelClient client = highLevelClient();

        Calendar calendar = new Calendar("holidays", Collections.singletonList("job_1"), "A calendar for public holidays");
        PutCalendarRequest putRequest = new PutCalendarRequest(calendar);
        client.machineLearning().putCalendar(putRequest, RequestOptions.DEFAULT);
        {
            // tag::get-calendars-request
            GetCalendarsRequest request = new GetCalendarsRequest(); // <1>
            // end::get-calendars-request

            // tag::get-calendars-id
            request.setCalendarId("holidays"); // <1>
            // end::get-calendars-id

            // tag::get-calendars-page
            request.setPageParams(new PageParams(10, 20)); // <1>
            // end::get-calendars-page

            // reset page params
            request.setPageParams(null);

            // tag::get-calendars-execute
            GetCalendarsResponse response = client.machineLearning().getCalendars(request, RequestOptions.DEFAULT);
            // end::get-calendars-execute

            // tag::get-calendars-response
            long count = response.count(); // <1>
            List<Calendar> calendars = response.calendars(); // <2>
            // end::get-calendars-response
            assertEquals(1, calendars.size());
        }
        {
            GetCalendarsRequest request = new GetCalendarsRequest("holidays");

            // tag::get-calendars-execute-listener
            ActionListener<GetCalendarsResponse> listener =
                    new ActionListener<GetCalendarsResponse>() {
                        @Override
                        public void onResponse(GetCalendarsResponse getCalendarsResponse) {
                            // <1>
                        }

                        @Override
                        public void onFailure(Exception e) {
                            // <2>
                        }
                    };
            // end::get-calendars-execute-listener

            // Replace the empty listener by a blocking listener in test
            final CountDownLatch latch = new CountDownLatch(1);
            listener = new LatchedActionListener<>(listener, latch);

            // tag::get-calendars-execute-async
            client.machineLearning().getCalendarsAsync(request, RequestOptions.DEFAULT, listener); // <1>
            // end::get-calendars-execute-async

            assertTrue(latch.await(30L, TimeUnit.SECONDS));
        }
    }

    public void testDeleteCalendar() throws IOException, InterruptedException {
        RestHighLevelClient client = highLevelClient();

        Calendar calendar = new Calendar("holidays", Collections.singletonList("job_1"), "A calendar for public holidays");
        PutCalendarRequest putCalendarRequest = new PutCalendarRequest(calendar);
        client.machineLearning().putCalendar(putCalendarRequest, RequestOptions.DEFAULT);

        // tag::delete-calendar-request
        DeleteCalendarRequest request = new DeleteCalendarRequest("holidays"); // <1>
        // end::delete-calendar-request

        // tag::delete-calendar-execute
        AcknowledgedResponse response = client.machineLearning().deleteCalendar(request, RequestOptions.DEFAULT);
        // end::delete-calendar-execute

        // tag::delete-calendar-response
        boolean isAcknowledged = response.isAcknowledged(); // <1>
        // end::delete-calendar-response

        assertTrue(isAcknowledged);

        // tag::delete-calendar-execute-listener
        ActionListener<AcknowledgedResponse> listener = new ActionListener<AcknowledgedResponse>() {
            @Override
            public void onResponse(AcknowledgedResponse response) {
                // <1>
            }

            @Override
            public void onFailure(Exception e) {
                // <2>
            }
        };
        // end::delete-calendar-execute-listener

        // Replace the empty listener by a blocking listener in test
        final CountDownLatch latch = new CountDownLatch(1);
        listener = new LatchedActionListener<>(listener, latch);

        // tag::delete-calendar-execute-async
        client.machineLearning().deleteCalendarAsync(request, RequestOptions.DEFAULT, listener); // <1>
        // end::delete-calendar-execute-async

        assertTrue(latch.await(30L, TimeUnit.SECONDS));
    }
}
