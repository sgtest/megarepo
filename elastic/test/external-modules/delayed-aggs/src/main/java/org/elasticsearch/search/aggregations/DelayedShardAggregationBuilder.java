/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.search.aggregations;

import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.search.aggregations.bucket.filter.FilterAggregationBuilder;
import org.elasticsearch.search.aggregations.support.AggregationContext;

import java.io.IOException;
import java.util.Map;
import java.util.Objects;

public class DelayedShardAggregationBuilder extends AbstractAggregationBuilder<DelayedShardAggregationBuilder> {
    public static final String NAME = "shard_delay";

    private TimeValue delay;

    public DelayedShardAggregationBuilder(String name, TimeValue delay) {
        super(name);
        this.delay = delay;
    }

    public DelayedShardAggregationBuilder(StreamInput in) throws IOException {
        super(in);
        this.delay = in.readTimeValue();
    }

    @Override
    protected AggregationBuilder shallowCopy(AggregatorFactories.Builder factoriesBuilder, Map<String, Object> metaData) {
        return new DelayedShardAggregationBuilder(name, delay);
    }

    @Override
    public BucketCardinality bucketCardinality() {
        return BucketCardinality.ONE;
    }

    @Override
    public String getType() {
        return NAME;
    }

    @Override
    protected void doWriteTo(StreamOutput out) throws IOException {
        out.writeTimeValue(delay);
    }

    @Override
    protected XContentBuilder internalXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.field("value", delay.toString());
        builder.endObject();
        return builder;
    }

    static final ConstructingObjectParser<DelayedShardAggregationBuilder, String> PARSER = new ConstructingObjectParser<>(
        NAME,
        false,
        (args, name) -> new DelayedShardAggregationBuilder(name, TimeValue.parseTimeValue((String) args[0], "value"))
    );

    static {
        PARSER.declareString(ConstructingObjectParser.constructorArg(), new ParseField("value"));
    }

    @Override
    @SuppressWarnings("unchecked")
    protected AggregatorFactory doBuild(
        AggregationContext context,
        AggregatorFactory parent,
        AggregatorFactories.Builder subfactoriesBuilder
    ) throws IOException {

        // Disable the request cache
        context.nowInMillis();

        final FilterAggregationBuilder filterAgg = new FilterAggregationBuilder(name, QueryBuilders.matchAllQuery()).subAggregations(
            subfactoriesBuilder
        );
        final AggregatorFactory factory = filterAgg.build(context, parent);
        return new AggregatorFactory(name, context, parent, subfactoriesBuilder, metadata) {
            @Override
            protected Aggregator createInternal(Aggregator parent, CardinalityUpperBound cardinality, Map<String, Object> metadata)
                throws IOException {
                long start = context.getRelativeTimeInMillis();
                long sleepTime = Math.min(delay.getMillis(), 100);
                do {
                    if (context.isCancelled()) {
                        break;
                    }
                    try {
                        Thread.sleep(sleepTime);
                    } catch (InterruptedException e) {
                        throw new IOException(e);
                    }
                } while (context.getRelativeTimeInMillis() - start < delay.getMillis());
                return factory.create(parent, cardinality);
            }
        };
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        if (super.equals(o) == false) return false;
        DelayedShardAggregationBuilder that = (DelayedShardAggregationBuilder) o;
        return Objects.equals(delay, that.delay);
    }

    @Override
    public int hashCode() {
        return Objects.hash(super.hashCode(), delay);
    }
}
