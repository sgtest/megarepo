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
package org.elasticsearch.client.ml.datafeed;

import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;

import java.io.IOException;
import java.util.Objects;

import static org.elasticsearch.common.xcontent.ConstructingObjectParser.constructorArg;
import static org.elasticsearch.common.xcontent.ConstructingObjectParser.optionalConstructorArg;

public class DatafeedTimingStats implements ToXContentObject {

    public static final ParseField JOB_ID = new ParseField("job_id");
    public static final ParseField SEARCH_COUNT = new ParseField("search_count");
    public static final ParseField TOTAL_SEARCH_TIME_MS = new ParseField("total_search_time_ms");

    public static final ParseField TYPE = new ParseField("datafeed_timing_stats");

    public static final ConstructingObjectParser<DatafeedTimingStats, Void> PARSER = createParser();

    private static ConstructingObjectParser<DatafeedTimingStats, Void> createParser() {
        ConstructingObjectParser<DatafeedTimingStats, Void> parser =
            new ConstructingObjectParser<>(
                "datafeed_timing_stats",
                true,
                args -> {
                    String jobId = (String) args[0];
                    Long searchCount = (Long) args[1];
                    Double totalSearchTimeMs = (Double) args[2];
                    return new DatafeedTimingStats(jobId, getOrDefault(searchCount, 0L), getOrDefault(totalSearchTimeMs, 0.0));
                });
        parser.declareString(constructorArg(), JOB_ID);
        parser.declareLong(optionalConstructorArg(), SEARCH_COUNT);
        parser.declareDouble(optionalConstructorArg(), TOTAL_SEARCH_TIME_MS);
        return parser;
    }

    private final String jobId;
    private long searchCount;
    private double totalSearchTimeMs;

    public DatafeedTimingStats(String jobId, long searchCount, double totalSearchTimeMs) {
        this.jobId = Objects.requireNonNull(jobId);
        this.searchCount = searchCount;
        this.totalSearchTimeMs = totalSearchTimeMs;
    }

    public String getJobId() {
        return jobId;
    }

    public long getSearchCount() {
        return searchCount;
    }

    public double getTotalSearchTimeMs() {
        return totalSearchTimeMs;
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, ToXContent.Params params) throws IOException {
        builder.startObject();
        builder.field(JOB_ID.getPreferredName(), jobId);
        builder.field(SEARCH_COUNT.getPreferredName(), searchCount);
        builder.field(TOTAL_SEARCH_TIME_MS.getPreferredName(), totalSearchTimeMs);
        builder.endObject();
        return builder;
    }

    @Override
    public boolean equals(Object obj) {
        if (this == obj) {
            return true;
        }
        if (obj == null || getClass() != obj.getClass()) {
            return false;
        }

        DatafeedTimingStats other = (DatafeedTimingStats) obj;
        return Objects.equals(this.jobId, other.jobId)
            && this.searchCount == other.searchCount
            && this.totalSearchTimeMs == other.totalSearchTimeMs;
    }

    @Override
    public int hashCode() {
        return Objects.hash(jobId, searchCount, totalSearchTimeMs);
    }

    @Override
    public String toString() {
        return Strings.toString(this);
    }

    private static <T> T getOrDefault(@Nullable T value, T defaultValue) {
        return value != null ? value : defaultValue;
    }
}
