/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.core.dataframe.transforms;

import org.elasticsearch.ElasticsearchParseException;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.ParsingException;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.xpack.core.dataframe.DataFrameField;

import java.io.IOException;
import java.util.Arrays;
import java.util.Collections;
import java.util.Map;
import java.util.Map.Entry;
import java.util.Objects;
import java.util.TreeMap;

import static org.elasticsearch.common.xcontent.ConstructingObjectParser.constructorArg;
import static org.elasticsearch.common.xcontent.ConstructingObjectParser.optionalConstructorArg;

/**
 * Checkpoint document to store the checkpoint of a data frame transform
 *
 * The fields:
 *
 *  timestamp the timestamp when this document has been created
 *  checkpoint the checkpoint number, incremented for every checkpoint, if -1 this is a non persisted checkpoint
 *  indices a map of the indices from the source including all checkpoints of all indices matching the source pattern, shard level
 *  time_upper_bound for time-based indices this holds the upper time boundary of this checkpoint
 *
 */
public class DataFrameTransformCheckpoint implements Writeable, ToXContentObject {

    public static DataFrameTransformCheckpoint EMPTY = new DataFrameTransformCheckpoint("empty", 0L, -1L, Collections.emptyMap(), 0L);

    // the own checkpoint
    public static final ParseField CHECKPOINT = new ParseField("checkpoint");

    // checkpoint of the indexes (sequence id's)
    public static final ParseField INDICES = new ParseField("indices");

    private static final String NAME = "data_frame_transform_checkpoint";

    private static final ConstructingObjectParser<DataFrameTransformCheckpoint, Void> STRICT_PARSER = createParser(false);
    private static final ConstructingObjectParser<DataFrameTransformCheckpoint, Void> LENIENT_PARSER = createParser(true);

    private final String transformId;
    private final long timestampMillis;
    private final long checkpoint;
    private final Map<String, long[]> indicesCheckpoints;
    private final long timeUpperBoundMillis;

    private static ConstructingObjectParser<DataFrameTransformCheckpoint, Void> createParser(boolean lenient) {
        ConstructingObjectParser<DataFrameTransformCheckpoint, Void> parser = new ConstructingObjectParser<>(NAME,
                lenient, args -> {
                    String id = (String) args[0];
                    Long timestamp = (Long) args[1];
                    Long checkpoint = (Long) args[2];

                    @SuppressWarnings("unchecked")
                    Map<String, long[]> checkpoints = (Map<String, long[]>) args[3];

                    Long timestamp_checkpoint = (Long) args[4];

                    // ignored, only for internal storage: String docType = (String) args[5];
                    return new DataFrameTransformCheckpoint(id, timestamp, checkpoint, checkpoints, timestamp_checkpoint);
                });

        parser.declareString(constructorArg(), DataFrameField.ID);

        // note: this is never parsed from the outside where timestamp can be formatted as date time
        parser.declareLong(constructorArg(), DataFrameField.TIMESTAMP_MILLIS);
        parser.declareLong(constructorArg(), CHECKPOINT);

        parser.declareObject(constructorArg(), (p,c) -> {
            Map<String, long[]> checkPointsByIndexName = new TreeMap<>();
            XContentParser.Token token = null;
            while ((token = p.nextToken()) != XContentParser.Token.END_OBJECT) {
                if (token != XContentParser.Token.FIELD_NAME) {
                    throw new ParsingException(p.getTokenLocation(), "Unexpected token " + token + " ");
                }

                final String indexName = p.currentName();
                token = p.nextToken();
                if (token != XContentParser.Token.START_ARRAY) {
                    throw new ParsingException(p.getTokenLocation(), "Unexpected token " + token + " ");
                }

                long[] checkpoints = p.listOrderedMap().stream().mapToLong(num -> ((Number) num).longValue()).toArray();
                checkPointsByIndexName.put(indexName, checkpoints);
            }
            return checkPointsByIndexName;
        }, INDICES);
        parser.declareLong(optionalConstructorArg(), DataFrameField.TIME_UPPER_BOUND_MILLIS);
        parser.declareString(optionalConstructorArg(), DataFrameField.INDEX_DOC_TYPE);

        return parser;
    }

    public DataFrameTransformCheckpoint(String transformId, Long timestamp, Long checkpoint, Map<String, long[]> checkpoints,
            Long timeUpperBound) {
        this.transformId = transformId;
        this.timestampMillis = timestamp.longValue();
        this.checkpoint = checkpoint;
        this.indicesCheckpoints = Collections.unmodifiableMap(checkpoints);
        this.timeUpperBoundMillis = timeUpperBound == null ? 0 : timeUpperBound.longValue();
    }

    public DataFrameTransformCheckpoint(StreamInput in) throws IOException {
        this.transformId = in.readString();
        this.timestampMillis = in.readLong();
        this.checkpoint = in.readLong();
        this.indicesCheckpoints = readCheckpoints(in.readMap());
        this.timeUpperBoundMillis = in.readLong();
    }

    public boolean isEmpty() {
        return indicesCheckpoints.isEmpty();
    }

    /**
     * Whether this checkpoint is a transient (non persisted) checkpoint
     *
     * @return true if this is a transient checkpoint, false otherwise
     */
    public boolean isTransient() {
        return checkpoint == -1;
    }

    /**
     * Create XContent for the purpose of storing it in the internal index
     *
     * Note:
     * @param builder the {@link XContentBuilder}
     * @param params builder specific parameters
     *
     * @return builder instance
     */
    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();

        builder.field(DataFrameField.ID.getPreferredName(), transformId);
        builder.field(CHECKPOINT.getPreferredName(), checkpoint);
        builder.field(DataFrameField.INDEX_DOC_TYPE.getPreferredName(), NAME);
        builder.startObject(INDICES.getPreferredName());
        for (Entry<String, long[]> entry : indicesCheckpoints.entrySet()) {
            builder.array(entry.getKey(), entry.getValue());
        }
        builder.endObject();

        builder.field(DataFrameField.TIMESTAMP_MILLIS.getPreferredName(), timestampMillis);

        if (timeUpperBoundMillis > 0) {
            builder.field(DataFrameField.TIME_UPPER_BOUND_MILLIS.getPreferredName(), timeUpperBoundMillis);
        }

        builder.endObject();
        return builder;
    }

    public String getTransformId() {
        return transformId;
    }

    public long getTimestamp() {
        return timestampMillis;
    }

    public long getCheckpoint() {
        return checkpoint;
    }

    public Map<String, long[]> getIndicesCheckpoints() {
        return indicesCheckpoints;
    }

    public long getTimeUpperBound() {
        return timeUpperBoundMillis;
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeString(transformId);
        out.writeLong(timestampMillis);
        out.writeLong(checkpoint);
        out.writeGenericValue(indicesCheckpoints);
        out.writeLong(timeUpperBoundMillis);
    }

    @Override
    public boolean equals(Object other) {
        if (this == other) {
            return true;
        }

        if (other == null || getClass() != other.getClass()) {
            return false;
        }

        final DataFrameTransformCheckpoint that = (DataFrameTransformCheckpoint) other;

        // compare the timestamp, id, checkpoint and than call matches for the rest
        return this.timestampMillis == that.timestampMillis && this.checkpoint == that.checkpoint
                && this.timeUpperBoundMillis == that.timeUpperBoundMillis && matches(that);
    }

    /**
     * Compares 2 checkpoints ignoring some inner fields.
     *
     * This is for comparing 2 checkpoints to check whether the data frame transform requires an update
     *
     * @param that other checkpoint
     * @return true if checkpoints match
     */
    public boolean matches (DataFrameTransformCheckpoint that) {
        if (this == that) {
            return true;
        }

        return Objects.equals(this.transformId, that.transformId)
                && this.indicesCheckpoints.size() == that.indicesCheckpoints.size() // quick check
                // do the expensive deep equal operation last
                && this.indicesCheckpoints.entrySet().stream()
                        .allMatch(e -> Arrays.equals(e.getValue(), that.indicesCheckpoints.get(e.getKey())));
    }

    @Override
    public int hashCode() {
        int hash = Objects.hash(transformId, timestampMillis, checkpoint, timeUpperBoundMillis);

        for (Entry<String, long[]> e : indicesCheckpoints.entrySet()) {
            hash = 31 * hash + Objects.hash(e.getKey(), Arrays.hashCode(e.getValue()));
        }
        return hash;
    }

    public static DataFrameTransformCheckpoint fromXContent(final XContentParser parser, boolean lenient) throws IOException {
        return lenient ? LENIENT_PARSER.apply(parser, null) : STRICT_PARSER.apply(parser, null);
    }

    public static String documentId(String transformId, long checkpoint) {
        if (checkpoint < 0) {
            throw new IllegalArgumentException("checkpoint must be a positive number");
        }

        return NAME + "-" + transformId + "-" + checkpoint;
    }

    /**
     * Calculate the diff of 2 checkpoints
     *
     * This is to get an indicator for the difference between checkpoints.
     *
     * Note: order is important
     *
     * @param oldCheckpoint the older checkpoint, if transient, newer must be transient, too
     * @param newCheckpoint the newer checkpoint, can be a transient checkpoint
     *
     * @return count number of operations the checkpoint is behind or -1L if it could not calculate the difference
     */
    public static long getBehind(DataFrameTransformCheckpoint oldCheckpoint, DataFrameTransformCheckpoint newCheckpoint) {
        if (oldCheckpoint.isTransient()) {
            if (newCheckpoint.isTransient() == false) {
                throw new IllegalArgumentException("can not compare transient against a non transient checkpoint");
            } // else: both are transient
        } else if (newCheckpoint.isTransient() == false && oldCheckpoint.getCheckpoint() > newCheckpoint.getCheckpoint()) {
            throw new IllegalArgumentException("old checkpoint is newer than new checkpoint");
        }

        // all old indices must be contained in the new ones but not vice versa
        if (newCheckpoint.indicesCheckpoints.keySet().containsAll(oldCheckpoint.indicesCheckpoints.keySet()) == false) {
            return -1L;
        }

        // get the sum of of shard checkpoints
        // note: we require shard checkpoints to strictly increase and never decrease
        long oldCheckPointSum = 0;
        long newCheckPointSum = 0;

        for (long[] v : oldCheckpoint.indicesCheckpoints.values()) {
            oldCheckPointSum += Arrays.stream(v).sum();
        }

        for (long[] v : newCheckpoint.indicesCheckpoints.values()) {
            newCheckPointSum += Arrays.stream(v).sum();
        }

        // this should not be possible
        if (newCheckPointSum < oldCheckPointSum) {
            return -1L;
        }

        return newCheckPointSum - oldCheckPointSum;
    }

    private static Map<String, long[]> readCheckpoints(Map<String, Object> readMap) {
        Map<String, long[]> checkpoints = new TreeMap<>();
        for (Map.Entry<String, Object> e : readMap.entrySet()) {
            if (e.getValue() instanceof long[]) {
                checkpoints.put(e.getKey(), (long[]) e.getValue());
            } else {
                throw new ElasticsearchParseException("expecting the checkpoints for [{}] to be a long[], but found [{}] instead",
                        e.getKey(), e.getValue().getClass());
            }
        }
        return checkpoints;
    }
}
