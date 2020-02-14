/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.analytics.topmetrics;

import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.ContextParser;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.index.query.QueryShardContext;
import org.elasticsearch.search.aggregations.AbstractAggregationBuilder;
import org.elasticsearch.search.aggregations.AggregationBuilder;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.elasticsearch.search.aggregations.AggregatorFactories.Builder;
import org.elasticsearch.search.aggregations.AggregatorFactory;
import org.elasticsearch.search.aggregations.support.MultiValuesSourceFieldConfig;
import org.elasticsearch.search.sort.SortBuilder;

import java.io.IOException;
import java.util.List;
import java.util.Map;

import static org.elasticsearch.common.xcontent.ConstructingObjectParser.constructorArg;
import static org.elasticsearch.search.builder.SearchSourceBuilder.SORT_FIELD;

public class TopMetricsAggregationBuilder extends AbstractAggregationBuilder<TopMetricsAggregationBuilder> {
    public static final String NAME = "top_metrics";
    public static final ParseField METRIC_FIELD = new ParseField("metric");

    public static final ConstructingObjectParser<TopMetricsAggregationBuilder, String> PARSER = new ConstructingObjectParser<>(NAME,
            false, (args, name) -> {
                @SuppressWarnings("unchecked")
                List<SortBuilder<?>> sorts = (List<SortBuilder<?>>) args[0];
                MultiValuesSourceFieldConfig metricField = (MultiValuesSourceFieldConfig) args[1];
                return new TopMetricsAggregationBuilder(name, sorts, metricField);
            });
    static {
        PARSER.declareField(constructorArg(), (p, n) -> SortBuilder.fromXContent(p), SORT_FIELD,
                ObjectParser.ValueType.OBJECT_ARRAY_OR_STRING);
        ContextParser<Void, MultiValuesSourceFieldConfig.Builder> metricParser = MultiValuesSourceFieldConfig.PARSER.apply(true, false);
        PARSER.declareObject(constructorArg(), (p, n) -> metricParser.parse(p, null).build(), METRIC_FIELD);
    }

    private final List<SortBuilder<?>> sortBuilders;
    // TODO MultiValuesSourceFieldConfig has more things than we support and less things than we want to support
    private final MultiValuesSourceFieldConfig metricField;

    /**
     * Ctor for parsing.
     */
    public TopMetricsAggregationBuilder(String name, List<SortBuilder<?>> sortBuilders, MultiValuesSourceFieldConfig metricField) {
        super(name);
        if (sortBuilders.size() != 1) {
            throw new IllegalArgumentException("[sort] must contain exactly one sort");
        }
        this.sortBuilders = sortBuilders;
        this.metricField = metricField;
    }

    /**
     * Cloning ctor for reducing.
     */
    public TopMetricsAggregationBuilder(TopMetricsAggregationBuilder clone, AggregatorFactories.Builder factoriesBuilder,
            Map<String, Object> metaData) {
        super(clone, factoriesBuilder, metaData);
        this.sortBuilders = clone.sortBuilders;
        this.metricField = clone.metricField;
    }

    /**
     * Read from a stream.
     */
    public TopMetricsAggregationBuilder(StreamInput in) throws IOException {
        super(in);
        @SuppressWarnings("unchecked")
        List<SortBuilder<?>> sortBuilders = (List<SortBuilder<?>>) (List<?>) in.readNamedWriteableList(SortBuilder.class); 
        this.sortBuilders = sortBuilders;
        this.metricField = new MultiValuesSourceFieldConfig(in);
    }

    @Override
    protected void doWriteTo(StreamOutput out) throws IOException {
        out.writeNamedWriteableList(sortBuilders);
        metricField.writeTo(out);
    }

    @Override
    protected AggregationBuilder shallowCopy(AggregatorFactories.Builder factoriesBuilder, Map<String, Object> metaData) {
        return new TopMetricsAggregationBuilder(this, factoriesBuilder, metaData);
    }

    @Override
    protected AggregatorFactory doBuild(QueryShardContext queryShardContext, AggregatorFactory parent, Builder subFactoriesBuilder)
            throws IOException {
        return new TopMetricsAggregatorFactory(name, queryShardContext, parent, subFactoriesBuilder, metaData, sortBuilders, metricField);
    }

    @Override
    protected XContentBuilder internalXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        {
            builder.startArray(SORT_FIELD.getPreferredName());
            for (SortBuilder<?> sort : sortBuilders) {
                sort.toXContent(builder, params);
            }
            builder.endArray();
            builder.field(METRIC_FIELD.getPreferredName(), metricField);
        }
        builder.endObject();
        return builder;
    }

    @Override
    public String getType() {
        return NAME;
    }

    List<SortBuilder<?>> getSortBuilders() {
        return sortBuilders;
    }

    MultiValuesSourceFieldConfig getMetricField() {
        return metricField;
    }
}
