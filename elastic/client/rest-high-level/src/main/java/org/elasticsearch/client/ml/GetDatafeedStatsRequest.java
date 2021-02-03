/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.client.ml;

import org.elasticsearch.client.Validatable;
import org.elasticsearch.client.ml.datafeed.DatafeedConfig;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Objects;

/**
 * Request object to get {@link org.elasticsearch.client.ml.datafeed.DatafeedStats} by their respective datafeedIds
 *
 * {@code _all} explicitly gets all the datafeeds' statistics in the cluster
 * An empty request (no {@code datafeedId}s) implicitly gets all the datafeeds' statistics in the cluster
 */
public class GetDatafeedStatsRequest implements Validatable, ToXContentObject {

    public static final ParseField ALLOW_NO_MATCH = new ParseField("allow_no_match");

    @SuppressWarnings("unchecked")
    public static final ConstructingObjectParser<GetDatafeedStatsRequest, Void> PARSER = new ConstructingObjectParser<>(
        "get_datafeed_stats_request", a -> new GetDatafeedStatsRequest((List<String>) a[0]));

    static {
        PARSER.declareField(ConstructingObjectParser.constructorArg(),
            p -> Arrays.asList(Strings.commaDelimitedListToStringArray(p.text())),
            DatafeedConfig.ID, ObjectParser.ValueType.STRING_ARRAY);
        PARSER.declareBoolean(GetDatafeedStatsRequest::setAllowNoMatch, ALLOW_NO_MATCH);
    }

    private static final String ALL_DATAFEEDS = "_all";

    private final List<String> datafeedIds;
    private Boolean allowNoMatch;

    /**
     * Explicitly gets all datafeeds statistics
     *
     * @return a {@link GetDatafeedStatsRequest} for all existing datafeeds
     */
    public static GetDatafeedStatsRequest getAllDatafeedStatsRequest(){
        return new GetDatafeedStatsRequest(ALL_DATAFEEDS);
    }

    GetDatafeedStatsRequest(List<String> datafeedIds) {
        if (datafeedIds.stream().anyMatch(Objects::isNull)) {
            throw new NullPointerException("datafeedIds must not contain null values");
        }
        this.datafeedIds = new ArrayList<>(datafeedIds);
    }

    /**
     * Get the specified Datafeed's statistics via their unique datafeedIds
     *
     * @param datafeedIds must be non-null and each datafeedId must be non-null
     */
    public GetDatafeedStatsRequest(String... datafeedIds) {
        this(Arrays.asList(datafeedIds));
    }

    /**
     * All the datafeedIds for which to get statistics
     */
    public List<String> getDatafeedIds() {
        return datafeedIds;
    }

    public Boolean getAllowNoMatch() {
        return this.allowNoMatch;
    }

    /**
     * Whether to ignore if a wildcard expression matches no datafeeds.
     *
     * This includes {@code _all} string or when no datafeeds have been specified
     *
     * @param allowNoMatch When {@code true} ignore if wildcard or {@code _all} matches no datafeeds. Defaults to {@code true}
     */
    public void setAllowNoMatch(boolean allowNoMatch) {
        this.allowNoMatch = allowNoMatch;
    }

    @Override
    public int hashCode() {
        return Objects.hash(datafeedIds, allowNoMatch);
    }

    @Override
    public boolean equals(Object other) {
        if (this == other) {
            return true;
        }

        if (other == null || getClass() != other.getClass()) {
            return false;
        }

        GetDatafeedStatsRequest that = (GetDatafeedStatsRequest) other;
        return Objects.equals(datafeedIds, that.datafeedIds) &&
            Objects.equals(allowNoMatch, that.allowNoMatch);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, ToXContent.Params params) throws IOException {
        builder.startObject();
        builder.field(DatafeedConfig.ID.getPreferredName(), Strings.collectionToCommaDelimitedString(datafeedIds));
        if (allowNoMatch != null) {
            builder.field(ALLOW_NO_MATCH.getPreferredName(), allowNoMatch);
        }
        builder.endObject();
        return builder;
    }

}
