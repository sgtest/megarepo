/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.action;

import org.elasticsearch.Version;
import org.elasticsearch.action.Action;
import org.elasticsearch.action.ActionRequestBuilder;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.FailedNodeException;
import org.elasticsearch.action.TaskOperationFailure;
import org.elasticsearch.action.support.tasks.BaseTasksRequest;
import org.elasticsearch.action.support.tasks.BaseTasksResponse;
import org.elasticsearch.client.ElasticsearchClient;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.xpack.core.ml.action.util.QueryPage;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.config.JobState;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.DataCounts;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSizeStats;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;

import java.io.IOException;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Objects;

public class GetJobsStatsAction extends Action<GetJobsStatsAction.Request, GetJobsStatsAction.Response, GetJobsStatsAction.RequestBuilder> {

    public static final GetJobsStatsAction INSTANCE = new GetJobsStatsAction();
    public static final String NAME = "cluster:monitor/xpack/ml/job/stats/get";

    private static final String DATA_COUNTS = "data_counts";
    private static final String MODEL_SIZE_STATS = "model_size_stats";
    private static final String STATE = "state";
    private static final String NODE = "node";

    private GetJobsStatsAction() {
        super(NAME);
    }

    @Override
    public RequestBuilder newRequestBuilder(ElasticsearchClient client) {
        return new RequestBuilder(client, this);
    }

    @Override
    public Response newResponse() {
        return new Response();
    }

    public static class Request extends BaseTasksRequest<Request> {

        public static final ParseField ALLOW_NO_JOBS = new ParseField("allow_no_jobs");

        private String jobId;
        private boolean allowNoJobs = true;

        // used internally to expand _all jobid to encapsulate all jobs in cluster:
        private List<String> expandedJobsIds;

        public Request(String jobId) {
            this.jobId = ExceptionsHelper.requireNonNull(jobId, Job.ID.getPreferredName());
            this.expandedJobsIds = Collections.singletonList(jobId);
        }

        public Request() {}

        public List<String> getExpandedJobsIds() { return expandedJobsIds; }

        public void setExpandedJobsIds(List<String> expandedJobsIds) { this.expandedJobsIds = expandedJobsIds; }

        public void setAllowNoJobs(boolean allowNoJobs) {
            this.allowNoJobs = allowNoJobs;
        }

        public String getJobId() {
            return jobId;
        }

        public boolean allowNoJobs() {
            return allowNoJobs;
        }

        @Override
        public boolean match(Task task) {
            return jobId.equals(MetaData.ALL) || OpenJobAction.JobTaskMatcher.match(task, jobId);
        }

        @Override
        public ActionRequestValidationException validate() {
            return null;
        }

        @Override
        public void readFrom(StreamInput in) throws IOException {
            super.readFrom(in);
            jobId = in.readString();
            expandedJobsIds = in.readList(StreamInput::readString);
            if (in.getVersion().onOrAfter(Version.V_6_1_0)) {
                allowNoJobs = in.readBoolean();
            }
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeString(jobId);
            out.writeStringList(expandedJobsIds);
            if (out.getVersion().onOrAfter(Version.V_6_1_0)) {
                out.writeBoolean(allowNoJobs);
            }
        }

        @Override
        public int hashCode() {
            return Objects.hash(jobId, allowNoJobs);
        }

        @Override
        public boolean equals(Object obj) {
            if (obj == null) {
                return false;
            }
            if (getClass() != obj.getClass()) {
                return false;
            }
            Request other = (Request) obj;
            return Objects.equals(jobId, other.jobId) && Objects.equals(allowNoJobs, other.allowNoJobs);
        }
    }

    public static class RequestBuilder extends ActionRequestBuilder<Request, Response, RequestBuilder> {

        public RequestBuilder(ElasticsearchClient client, GetJobsStatsAction action) {
            super(client, action, new Request());
        }
    }

    public static class Response extends BaseTasksResponse implements ToXContentObject {

        public static class JobStats implements ToXContentObject, Writeable {
            private final String jobId;
            private DataCounts dataCounts;
            @Nullable
            private ModelSizeStats modelSizeStats;
            @Nullable
            private TimeValue openTime;
            private JobState state;
            @Nullable
            private DiscoveryNode node;
            @Nullable
            private String assignmentExplanation;

            public JobStats(String jobId, DataCounts dataCounts, @Nullable ModelSizeStats modelSizeStats, JobState state,
                     @Nullable  DiscoveryNode node, @Nullable String assignmentExplanation, @Nullable TimeValue opentime) {
                this.jobId = Objects.requireNonNull(jobId);
                this.dataCounts = Objects.requireNonNull(dataCounts);
                this.modelSizeStats = modelSizeStats;
                this.state = Objects.requireNonNull(state);
                this.node = node;
                this.assignmentExplanation = assignmentExplanation;
                this.openTime = opentime;
            }

            public JobStats(StreamInput in) throws IOException {
                jobId = in.readString();
                dataCounts = new DataCounts(in);
                modelSizeStats = in.readOptionalWriteable(ModelSizeStats::new);
                state = JobState.fromStream(in);
                node = in.readOptionalWriteable(DiscoveryNode::new);
                assignmentExplanation = in.readOptionalString();
                openTime = in.readOptionalTimeValue();
            }

            public String getJobId() {
                return jobId;
            }

            public DataCounts getDataCounts() {
                return dataCounts;
            }

            public ModelSizeStats getModelSizeStats() {
                return modelSizeStats;
            }

            public JobState getState() {
                return state;
            }

            public DiscoveryNode getNode() {
                return node;
            }

            public String getAssignmentExplanation() {
                return assignmentExplanation;
            }

            public TimeValue getOpenTime() {
                return openTime;
            }

            @Override
            public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
                // TODO: Have callers wrap the content with an object as they choose rather than forcing it upon them
                builder.startObject();
                {
                    toUnwrappedXContent(builder);
                }
                return builder.endObject();
            }

            public XContentBuilder toUnwrappedXContent(XContentBuilder builder) throws IOException {
                builder.field(Job.ID.getPreferredName(), jobId);
                builder.field(DATA_COUNTS, dataCounts);
                if (modelSizeStats != null) {
                    builder.field(MODEL_SIZE_STATS, modelSizeStats);
                }
                builder.field(STATE, state.toString());
                if (node != null) {
                    builder.startObject(NODE);
                    builder.field("id", node.getId());
                    builder.field("name", node.getName());
                    builder.field("ephemeral_id", node.getEphemeralId());
                    builder.field("transport_address", node.getAddress().toString());

                    builder.startObject("attributes");
                    for (Map.Entry<String, String> entry : node.getAttributes().entrySet()) {
                        builder.field(entry.getKey(), entry.getValue());
                    }
                    builder.endObject();
                    builder.endObject();
                }
                if (assignmentExplanation != null) {
                    builder.field("assignment_explanation", assignmentExplanation);
                }
                if (openTime != null) {
                    builder.field("open_time", openTime.getStringRep());
                }
                return builder;
            }

            @Override
            public void writeTo(StreamOutput out) throws IOException {
                out.writeString(jobId);
                dataCounts.writeTo(out);
                out.writeOptionalWriteable(modelSizeStats);
                state.writeTo(out);
                out.writeOptionalWriteable(node);
                out.writeOptionalString(assignmentExplanation);
                out.writeOptionalTimeValue(openTime);
            }

            @Override
            public int hashCode() {
                return Objects.hash(jobId, dataCounts, modelSizeStats, state, node, assignmentExplanation, openTime);
            }

            @Override
            public boolean equals(Object obj) {
                if (obj == null) {
                    return false;
                }
                if (getClass() != obj.getClass()) {
                    return false;
                }
                JobStats other = (JobStats) obj;
                return Objects.equals(jobId, other.jobId)
                        && Objects.equals(this.dataCounts, other.dataCounts)
                        && Objects.equals(this.modelSizeStats, other.modelSizeStats)
                        && Objects.equals(this.state, other.state)
                        && Objects.equals(this.node, other.node)
                        && Objects.equals(this.assignmentExplanation, other.assignmentExplanation)
                        && Objects.equals(this.openTime, other.openTime);
            }
        }

        private QueryPage<JobStats> jobsStats;

        public Response(QueryPage<JobStats> jobsStats) {
            super(Collections.emptyList(), Collections.emptyList());
            this.jobsStats = jobsStats;
        }

        public Response(List<TaskOperationFailure> taskFailures, List<? extends FailedNodeException> nodeFailures,
                 QueryPage<JobStats> jobsStats) {
            super(taskFailures, nodeFailures);
            this.jobsStats = jobsStats;
        }

        public Response() {
            super(Collections.emptyList(), Collections.emptyList());
        }

        public QueryPage<JobStats> getResponse() {
            return jobsStats;
        }

        @Override
        public void readFrom(StreamInput in) throws IOException {
            super.readFrom(in);
            jobsStats = new QueryPage<>(in, JobStats::new);
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            jobsStats.writeTo(out);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();;
            jobsStats.doXContentBody(builder, params);
            builder.endObject();
            return builder;
        }

        @Override
        public int hashCode() {
            return Objects.hash(jobsStats);
        }

        @Override
        public boolean equals(Object obj) {
            if (obj == null) {
                return false;
            }
            if (getClass() != obj.getClass()) {
                return false;
            }
            Response other = (Response) obj;
            return Objects.equals(jobsStats, other.jobsStats);
        }

        @Override
        public final String toString() {
            return Strings.toString(this);
        }
    }

}
