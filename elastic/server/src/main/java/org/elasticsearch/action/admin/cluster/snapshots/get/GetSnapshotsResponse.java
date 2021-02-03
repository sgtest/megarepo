/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.action.admin.cluster.snapshots.get;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.util.set.Sets;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.snapshots.SnapshotInfo;

import java.io.IOException;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Get snapshots response
 */
public class GetSnapshotsResponse extends ActionResponse implements ToXContentObject {

    private static final ConstructingObjectParser<GetSnapshotsResponse, Void> PARSER =
            new ConstructingObjectParser<>(GetSnapshotsResponse.class.getName(), true,
                    (args) -> new GetSnapshotsResponse((List<Response>) args[0]));

    static {
        PARSER.declareObjectArray(ConstructingObjectParser.constructorArg(),
                (p, c) -> Response.fromXContent(p), new ParseField("responses"));
    }

    public GetSnapshotsResponse(StreamInput in) throws IOException {
        if (in.getVersion().onOrAfter(GetSnapshotsRequest.MULTIPLE_REPOSITORIES_SUPPORT_ADDED)) {
            Map<String, List<SnapshotInfo>> successfulResponses = in.readMapOfLists(StreamInput::readString, SnapshotInfo::new);
            Map<String, ElasticsearchException> failedResponses = in.readMap(StreamInput::readString, StreamInput::readException);
            this.successfulResponses = Collections.unmodifiableMap(successfulResponses);
            this.failedResponses = Collections.unmodifiableMap(failedResponses);
        } else {
            this.successfulResponses = Collections.singletonMap("unknown", in.readList(SnapshotInfo::new));
            this.failedResponses = Collections.emptyMap();
        }
    }


    public static class Response {
        private final String repository;
        private final List<SnapshotInfo> snapshots;
        private final ElasticsearchException error;

        private static final ConstructingObjectParser<Response, Void> RESPONSE_PARSER =
                new ConstructingObjectParser<>(Response.class.getName(), true,
                        (args) -> new Response((String) args[0],
                                (List<SnapshotInfo>) args[1], (ElasticsearchException) args[2]));

        static {
            RESPONSE_PARSER.declareString(ConstructingObjectParser.constructorArg(), new ParseField("repository"));
            RESPONSE_PARSER.declareObjectArray(ConstructingObjectParser.optionalConstructorArg(),
                    (p, c) -> SnapshotInfo.SNAPSHOT_INFO_PARSER.apply(p, c).build(), new ParseField("snapshots"));
            RESPONSE_PARSER.declareObject(ConstructingObjectParser.optionalConstructorArg(),
                    (p, c) -> ElasticsearchException.fromXContent(p), new ParseField("error"));
        }

        private Response(String repository, List<SnapshotInfo> snapshots, ElasticsearchException error) {
            this.repository = repository;
            this.snapshots = snapshots;
            this.error = error;
        }

        public static Response snapshots(String repository, List<SnapshotInfo> snapshots) {
            return new Response(repository, snapshots, null);
        }

        public static Response error(String repository, ElasticsearchException error) {
            return new Response(repository, null, error);
        }

        private static Response fromXContent(XContentParser parser) throws IOException {
            return RESPONSE_PARSER.parse(parser, null);
        }
    }

    private final Map<String, List<SnapshotInfo>> successfulResponses;
    private final Map<String, ElasticsearchException> failedResponses;

    public GetSnapshotsResponse(Collection<Response> responses) {
        Map<String, List<SnapshotInfo>> successfulResponses = new HashMap<>();
        Map<String, ElasticsearchException> failedResponses = new HashMap<>();
        for (Response response : responses) {
            if (response.snapshots != null) {
                assert response.error == null;
                successfulResponses.put(response.repository, response.snapshots);
            } else {
                assert response.snapshots == null;
                failedResponses.put(response.repository, response.error);
            }
        }
        this.successfulResponses = Collections.unmodifiableMap(successfulResponses);
        this.failedResponses = Collections.unmodifiableMap(failedResponses);
    }

    /**
     * Returns list of snapshots for the specified repository.
     * @param repo - repository name.
     * @return list of snapshots.
     * @throws IllegalArgumentException if there is no such repository in the response.
     * @throws ElasticsearchException if an exception occurred when retrieving snapshots from the repository.
     */
    public List<SnapshotInfo> getSnapshots(String repo) {
        List<SnapshotInfo> snapshots = successfulResponses.get(repo);
        if (snapshots != null) {
            return snapshots;
        }
        ElasticsearchException error = failedResponses.get(repo);
        if (error == null) {
            throw new IllegalArgumentException("No such repository");
        }
        throw error;
    }

    /**
     * Returns list of repositories for both successful and unsuccessful responses.
     */
    public Set<String> getRepositories() {
        return Sets.union(successfulResponses.keySet(), failedResponses.keySet());
    }

    /**
     * Returns a map of repository name to the list of {@link SnapshotInfo} for each successful response.
     */
    public Map<String, List<SnapshotInfo>> getSuccessfulResponses() {
        return successfulResponses;
    }

    /**
     * Returns a map of repository name to {@link ElasticsearchException} for each unsuccessful response.
     */
    public Map<String, ElasticsearchException> getFailedResponses() {
        return failedResponses;
    }

    /**
     * Returns true if there is a least one failed response.
     */
    public boolean isFailed() {
        return failedResponses.isEmpty() == false;
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.startArray("responses");

        for (Map.Entry<String, List<SnapshotInfo>> snapshots : successfulResponses.entrySet()) {
            builder.startObject();
            builder.field("repository", snapshots.getKey());
            builder.startArray("snapshots");
            for (SnapshotInfo snapshot : snapshots.getValue()) {
                snapshot.toXContent(builder, params);
            }
            builder.endArray();
            builder.endObject();
        }

        for (Map.Entry<String, ElasticsearchException> error : failedResponses.entrySet()) {
            builder.startObject();
            builder.field("repository", error.getKey());
            ElasticsearchException.generateFailureXContent(builder, params, error.getValue(), true);
            builder.endObject();
        }

        builder.endArray();
        builder.endObject();
        return builder;
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        if (out.getVersion().onOrAfter(GetSnapshotsRequest.MULTIPLE_REPOSITORIES_SUPPORT_ADDED)) {
            out.writeMapOfLists(successfulResponses, StreamOutput::writeString, (o, s) -> s.writeTo(o));
            out.writeMap(failedResponses, StreamOutput::writeString, StreamOutput::writeException);
        } else {
            if (successfulResponses.size() + failedResponses.size() != 1) {
                throw new IllegalArgumentException("Requesting snapshots from multiple repositories is not supported in versions prior " +
                        "to " + GetSnapshotsRequest.MULTIPLE_REPOSITORIES_SUPPORT_ADDED.toString());
            }

            if (successfulResponses.size() == 1) {
                out.writeList(successfulResponses.values().iterator().next());
            }

            if (failedResponses.isEmpty() == false) {
                throw failedResponses.values().iterator().next();
            }
        }
    }

    public static GetSnapshotsResponse fromXContent(XContentParser parser) throws IOException {
        return PARSER.parse(parser, null);
    }

    @Override
    public String toString() {
        return Strings.toString(this);
    }
}
